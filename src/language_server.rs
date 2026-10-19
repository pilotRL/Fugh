//! Small asynchronous LSP client transport for clangd and Roslyn.
//! Language understanding is delegated to these servers, never inferred from
//! source-text regular expressions.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use eframe::egui::Context;
use serde_json::{Value, json};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerLanguage {
    Cpp,
    CSharp,
}

impl ServerLanguage {
    fn language_id(self) -> &'static str {
        match self {
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::Cpp => "clangd",
            Self::CSharp => "roslyn-language-server",
        }
    }

    fn args(self, root: &Path) -> Vec<String> {
        match self {
            Self::Cpp => {
                let mut args = vec!["--background-index".to_owned()];
                let build = root.join("build");
                if build.join("compile_commands.json").is_file() {
                    args.push(format!("--compile-commands-dir={}", build.display()));
                } else if root.join("compile_commands.json").is_file() {
                    args.push(format!("--compile-commands-dir={}", root.display()));
                }
                args
            }
            Self::CSharp => vec!["--stdio".to_owned(), "--autoLoadProjects".to_owned()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDiagnostic {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub severity: u8,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceLocation {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub is_declaration: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyEntry {
    pub name: String,
    pub detail: Option<String>,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSymbolEntry {
    pub name: String,
    pub detail: Option<String>,
    pub kind: Option<u64>,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub depth: usize,
}

enum ClientCommand {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: u64,
        method: String,
        params: Value,
    },
    Response {
        id: Value,
        result: Value,
    },
    Shutdown,
}

struct Session {
    root: PathBuf,
    language: ServerLanguage,
    sender: Sender<ClientCommand>,
    receiver: Receiver<Value>,
    document: Option<PathBuf>,
    version: i32,
    text: String,
    open_documents: BTreeMap<PathBuf, String>,
    open_versions: BTreeMap<PathBuf, i32>,
}

pub struct LanguageServerClient {
    session: Option<Session>,
    next_id: u64,
    responses: Vec<Value>,
    diagnostics: Vec<ServerDiagnostic>,
    pending_diagnostics: BTreeMap<u64, (PathBuf, i32)>,
    status: Option<String>,
    failures: u32,
    retry_after: Option<Instant>,
    connected_at: Option<Instant>,
}

impl Default for LanguageServerClient {
    fn default() -> Self {
        Self {
            session: None,
            next_id: 10,
            responses: Vec::new(),
            diagnostics: Vec::new(),
            pending_diagnostics: BTreeMap::new(),
            status: None,
            failures: 0,
            retry_after: None,
            connected_at: None,
        }
    }
}

impl LanguageServerClient {
    pub fn sync_document(
        &mut self,
        root: &Path,
        path: &Path,
        language: ServerLanguage,
        text: &str,
        context: &Context,
    ) {
        self.poll();
        let restart = self
            .session
            .as_ref()
            .is_some_and(|session| session.root != root || session.language != language);
        if restart {
            self.stop();
            self.failures = 0;
            self.retry_after = None;
        }
        if self.session.is_none() {
            if self
                .retry_after
                .is_some_and(|retry_at| Instant::now() < retry_at)
            {
                return;
            }
            self.start(root.to_path_buf(), language, context);
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let mut refresh_diagnostics = false;
        if session.document.as_deref() != Some(path) {
            refresh_diagnostics = true;
            session.document = Some(path.to_path_buf());
            session.text = text.to_owned();
            self.responses.clear();
            if let Some(uri) = file_uri(path) {
                if let Some(open_text) = session.open_documents.get_mut(path) {
                    if open_text != text {
                        *open_text = text.to_owned();
                        session.version += 1;
                        session
                            .open_versions
                            .insert(path.to_path_buf(), session.version);
                        self.diagnostics
                            .retain(|diagnostic| diagnostic.file != path);
                        let _ = session.sender.send(ClientCommand::Notification {
                            method: "textDocument/didChange".to_owned(),
                            params: json!({"textDocument": {"uri": uri, "version": session.version},
                                "contentChanges": [{"text": text}]}),
                        });
                    }
                } else {
                    session.version += 1;
                    session
                        .open_documents
                        .insert(path.to_path_buf(), text.to_owned());
                    session
                        .open_versions
                        .insert(path.to_path_buf(), session.version);
                    let _ = session.sender.send(ClientCommand::Notification {
                        method: "textDocument/didOpen".to_owned(),
                        params: json!({"textDocument": {
                            "uri": uri,
                            "languageId": language.language_id(),
                            "version": session.version,
                            "text": text
                        }}),
                    });
                }
            }
        } else if session.text != text {
            refresh_diagnostics = true;
            session.version += 1;
            session.text.clear();
            session.text.push_str(text);
            if let Some(uri) = file_uri(path) {
                session
                    .open_documents
                    .insert(path.to_path_buf(), text.to_owned());
                session
                    .open_versions
                    .insert(path.to_path_buf(), session.version);
                self.diagnostics
                    .retain(|diagnostic| diagnostic.file != path);
                let _ = session.sender.send(ClientCommand::Notification {
                    method: "textDocument/didChange".to_owned(),
                    params: json!({"textDocument": {"uri": uri, "version": session.version},
                        "contentChanges": [{"text": text}]}),
                });
            }
        }
        if refresh_diagnostics {
            self.request_diagnostics(path);
        }
    }

    fn request_diagnostics(&mut self, path: &Path) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Some(uri) = file_uri(path) else {
            return;
        };
        let Some(version) = session.open_versions.get(path).copied() else {
            return;
        };
        let sender = session.sender.clone();
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if sender
            .send(ClientCommand::Request {
                id,
                method: "textDocument/diagnostic".to_owned(),
                params: json!({"textDocument": {"uri": uri}}),
            })
            .is_ok()
        {
            self.pending_diagnostics
                .insert(id, (path.to_path_buf(), version));
        }
    }

    pub fn request_definition(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
    ) -> Option<u64> {
        self.request_at(path, line, character, "textDocument/definition", json!({}))
    }

    pub fn request_references(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
        include_declaration: bool,
    ) -> Option<u64> {
        self.request_at(
            path,
            line,
            character,
            "textDocument/references",
            json!({"context": {"includeDeclaration": include_declaration}}),
        )
    }

    pub fn request_code_actions(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
    ) -> Option<u64> {
        let (uri, sender, text) = {
            let session = self.session.as_ref()?;
            if session.document.as_deref() != Some(path) {
                return None;
            }
            (
                file_uri(path)?,
                session.sender.clone(),
                session.text.clone(),
            )
        };
        let position_character = utf16_column(text.lines().nth(line).unwrap_or(""), character);
        let diagnostics = self
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.file == path
                    && diagnostic.line.saturating_sub(1) <= line
                    && diagnostic.end_line.saturating_sub(1) >= line
            })
            .map(|diagnostic| {
                let start_line = diagnostic.line.saturating_sub(1);
                let end_line = diagnostic.end_line.saturating_sub(1);
                let start_character = text
                    .lines()
                    .nth(start_line)
                    .map_or(diagnostic.column.saturating_sub(1), |text| {
                        utf16_column(text, diagnostic.column.saturating_sub(1))
                    });
                let end_character = text
                    .lines()
                    .nth(end_line)
                    .map_or(diagnostic.end_column.saturating_sub(1), |text| {
                        utf16_column(text, diagnostic.end_column.saturating_sub(1))
                    });
                json!({
                    "range": {
                        "start": {"line": start_line, "character": start_character},
                        "end": {"line": end_line, "character": end_character}
                    },
                    "severity": diagnostic.severity,
                    "code": diagnostic.code,
                    "source": diagnostic.source,
                    "message": diagnostic.message
                })
            })
            .collect::<Vec<_>>();
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        sender
            .send(ClientCommand::Request {
                id,
                method: "textDocument/codeAction".to_owned(),
                params: json!({
                    "textDocument": {"uri": uri},
                "range": {
                    "start": {"line": line, "character": position_character},
                    "end": {"line": line, "character": position_character}
                },
                "context": {"diagnostics": diagnostics}
                }),
            })
            .ok()?;
        Some(id)
    }

    pub fn request_document_symbols(&mut self, path: &Path) -> Option<u64> {
        let session = self.session.as_ref()?;
        if session.document.as_deref() != Some(path) {
            return None;
        }
        self.request_raw(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": file_uri(path)?}}),
        )
    }

    pub fn execute_command(&mut self, command: &str, arguments: Value) -> Option<u64> {
        if command.trim().is_empty() {
            return None;
        }
        self.request_raw(
            "workspace/executeCommand",
            json!({"command": command, "arguments": arguments}),
        )
    }

    pub fn respond_to_server_request(&self, id: Value, result: Value) -> bool {
        self.session.as_ref().is_some_and(|session| {
            session
                .sender
                .send(ClientCommand::Response { id, result })
                .is_ok()
        })
    }

    pub fn request_prepare_call_hierarchy(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
    ) -> Option<u64> {
        self.request_at(
            path,
            line,
            character,
            "textDocument/prepareCallHierarchy",
            json!({}),
        )
    }

    pub fn request_incoming_calls(&mut self, item: Value) -> Option<u64> {
        self.request_raw("callHierarchy/incomingCalls", json!({"item": item}))
    }

    pub fn request_outgoing_calls(&mut self, item: Value) -> Option<u64> {
        self.request_raw("callHierarchy/outgoingCalls", json!({"item": item}))
    }

    pub fn request_hover(&mut self, path: &Path, line: usize, character: usize) -> Option<u64> {
        self.request_at(path, line, character, "textDocument/hover", json!({}))
    }

    pub fn request_signature_help(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
        trigger_character: Option<char>,
    ) -> Option<u64> {
        let context = if let Some(character) = trigger_character {
            json!({
                "triggerKind": 2,
                "triggerCharacter": character.to_string(),
                "isRetrigger": false
            })
        } else {
            json!({
                "triggerKind": 1,
                "isRetrigger": false
            })
        };
        self.request_at(
            path,
            line,
            character,
            "textDocument/signatureHelp",
            json!({"context": context}),
        )
    }

    pub fn request_rename(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
        new_name: &str,
    ) -> Option<u64> {
        self.request_at(
            path,
            line,
            character,
            "textDocument/rename",
            json!({"newName": new_name}),
        )
    }

    pub fn request_completion(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
    ) -> Option<u64> {
        self.request_at(
            path,
            line,
            character,
            "textDocument/completion",
            json!({"context": {"triggerKind": 1}}),
        )
    }

    fn request_at(
        &mut self,
        path: &Path,
        line: usize,
        character: usize,
        method: &str,
        extra: Value,
    ) -> Option<u64> {
        let session = self.session.as_ref()?;
        if session.document.as_deref() != Some(path) {
            return None;
        }
        let uri = file_uri(path)?;
        // The editor counts Unicode scalar values; LSP positions are UTF-16
        // code units. Convert at the protocol boundary for non-ASCII source.
        let character = utf16_column(session.text.lines().nth(line).unwrap_or(""), character);
        let id = self.next_id;
        self.next_id += 1;
        let mut params = json!({
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character}
        });
        if let (Some(params), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            params.extend(extra.clone());
        }
        session
            .sender
            .send(ClientCommand::Request {
                id,
                method: method.to_owned(),
                params,
            })
            .ok()?;
        Some(id)
    }

    fn request_raw(&mut self, method: &str, params: Value) -> Option<u64> {
        let session = self.session.as_ref()?;
        let id = self.next_id;
        self.next_id += 1;
        session
            .sender
            .send(ClientCommand::Request {
                id,
                method: method.to_owned(),
                params,
            })
            .ok()?;
        Some(id)
    }

    pub fn poll(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let mut messages = Vec::new();
        let mut worker_gone = false;
        loop {
            match session.receiver.try_recv() {
                Ok(message) => messages.push(message),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    worker_gone = true;
                    break;
                }
            }
        }
        for message in messages {
            if message.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                self.read_diagnostics(&message);
            } else if message.get("method").and_then(Value::as_str)
                == Some("workspace/diagnostic/refresh")
            {
                if let Some(path) = self
                    .session
                    .as_ref()
                    .and_then(|session| session.document.clone())
                {
                    self.request_diagnostics(&path);
                }
            } else if message.get("method").and_then(Value::as_str) == Some("window/logMessage") {
                if message
                    .pointer("/params/cairnConnected")
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    self.connected_at = Some(Instant::now());
                    if let Some(session) = &self.session {
                        self.status = Some(format!("{} connected", session.language.command()));
                    }
                }
                if let Some(text) = message.pointer("/params/message").and_then(Value::as_str) {
                    let is_failure = ["error", "failed", "stopped", "exited"]
                        .iter()
                        .any(|marker| text.to_ascii_lowercase().contains(marker));
                    if self.connected_at.is_none() || is_failure {
                        self.status = Some(summarize_server_message(text));
                    }
                }
            } else if message.get("id").is_some() {
                let id = message.get("id").and_then(Value::as_u64);
                if let Some(id) = id
                    && let Some((path, version)) = self.pending_diagnostics.remove(&id)
                {
                    let current_version = self
                        .session
                        .as_ref()
                        .and_then(|session| session.open_versions.get(&path))
                        .copied();
                    if current_version == Some(version) {
                        self.read_pull_diagnostics(&message, &path);
                    }
                } else {
                    self.responses.push(message);
                }
            }
        }
        if self.responses.len() > 256 {
            let excess = self.responses.len() - 256;
            self.responses.drain(..excess);
        }
        if worker_gone {
            self.handle_worker_exit();
        }
    }

    fn handle_worker_exit(&mut self) {
        self.session = None;
        self.diagnostics.clear();
        if self
            .connected_at
            .take()
            .is_some_and(|since| since.elapsed() > Duration::from_secs(30))
        {
            self.failures = 0;
        }
        self.failures = self.failures.saturating_add(1);
        let delay = Duration::from_secs((5_u64 << self.failures.min(4)).min(60));
        self.retry_after = Some(Instant::now() + delay);
        let reason = self
            .status
            .take()
            .unwrap_or_else(|| "The language server stopped.".to_owned());
        self.status = Some(format!("{reason} Retrying in {} s.", delay.as_secs()));
    }

    pub fn diagnostics(&self) -> &[ServerDiagnostic] {
        &self.diagnostics
    }

    pub fn close_document(&mut self) {
        if let Some(session) = self.session.as_mut() {
            if let Some(path) = session.document.take()
                && let Some(uri) = file_uri(&path)
            {
                session.open_documents.remove(&path);
                session.open_versions.remove(&path);
                self.diagnostics
                    .retain(|diagnostic| diagnostic.file != path);
                let _ = session.sender.send(ClientCommand::Notification {
                    method: "textDocument/didClose".to_owned(),
                    params: json!({"textDocument": {"uri": uri}}),
                });
            }
        }
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// True after the initialize handshake has completed and the worker has
    /// sent the `initialized` notification. Semantic requests issued before
    /// this point can be answered with empty results by some language servers.
    pub fn is_ready(&self) -> bool {
        self.session.is_some() && self.connected_at.is_some()
    }

    pub fn take_responses(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.responses)
    }

    fn start(&mut self, root: PathBuf, language: ServerLanguage, context: &Context) {
        let (sender, receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let thread_root = root.clone();
        let repaint_context = context.clone();
        let spawn_result = thread::Builder::new()
            .name(format!("cairn-lsp-{}", language.language_id()))
            .spawn(move || {
                server_worker(
                    thread_root,
                    language,
                    receiver,
                    event_sender,
                    repaint_context,
                )
            });
        if let Err(error) = spawn_result {
            self.status = Some(format!("Could not start language server worker: {error}"));
            return;
        }
        self.session = Some(Session {
            root,
            language,
            sender,
            receiver: event_receiver,
            document: None,
            version: 0,
            text: String::new(),
            open_documents: BTreeMap::new(),
            open_versions: BTreeMap::new(),
        });
        self.diagnostics.clear();
        self.status = Some(format!("Starting {} language server…", language.command()));
    }

    fn read_diagnostics(&mut self, message: &Value) {
        let Some(params) = message.get("params") else {
            return;
        };
        let Some(uri) = params
            .get("uri")
            .and_then(Value::as_str)
            .and_then(parse_file_uri)
        else {
            return;
        };
        if let (Some(incoming), Some(current)) = (
            params.get("version").and_then(Value::as_i64),
            self.session
                .as_ref()
                .and_then(|session| session.open_versions.get(&uri)),
        ) && incoming < i64::from(*current)
        {
            return;
        }
        let Some(items) = params.get("diagnostics").and_then(Value::as_array) else {
            return;
        };
        self.update_diagnostics(uri, items);
    }

    fn read_pull_diagnostics(&mut self, message: &Value, path: &Path) {
        let Some(result) = message.get("result") else {
            return;
        };
        if result.get("kind").and_then(Value::as_str) != Some("full") {
            return;
        }
        let Some(items) = result.get("items").and_then(Value::as_array) else {
            return;
        };
        self.update_diagnostics(path.to_path_buf(), items);
    }

    fn update_diagnostics(&mut self, uri: PathBuf, items: &[Value]) {
        let document_text = self
            .session
            .as_ref()
            .and_then(|session| session.open_documents.get(&uri))
            .cloned();
        let diagnostics = items
            .iter()
            .filter_map(|item| {
                let range = item.get("range")?;
                let line = range.pointer("/start/line")?.as_u64()? as usize + 1;
                let units = range.pointer("/start/character")?.as_u64()? as usize;
                let end_line = range.pointer("/end/line")?.as_u64()? as usize + 1;
                let end_units = range.pointer("/end/character")?.as_u64()? as usize;
                let column = document_text
                    .as_deref()
                    .and_then(|text| text.lines().nth(line - 1))
                    .map_or(units, |text| char_column(text, units))
                    + 1;
                let end_column = document_text
                    .as_deref()
                    .and_then(|text| text.lines().nth(end_line - 1))
                    .map_or(end_units, |text| char_column(text, end_units))
                    + 1;
                Some(ServerDiagnostic {
                    file: uri.clone(),
                    line,
                    column,
                    end_line,
                    end_column,
                    severity: item.get("severity").and_then(Value::as_u64).unwrap_or(1) as u8,
                    code: item.get("code").map(|value| {
                        value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_owned)
                    }),
                    source: item
                        .get("source")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    message: item.get("message")?.as_str()?.to_owned(),
                })
            })
            .collect::<Vec<_>>();
        self.diagnostics.retain(|diagnostic| diagnostic.file != uri);
        self.diagnostics.extend(diagnostics);
    }

    pub fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.sender.send(ClientCommand::Shutdown);
        }
        self.connected_at = None;
        self.diagnostics.clear();
        self.pending_diagnostics.clear();
    }
}

fn summarize_server_message(message: &str) -> String {
    if message.contains("System.Text.Json.JsonException")
        || message.contains("missing required properties")
    {
        "Language server rejected a request. Completion may be unavailable until it restarts."
            .to_owned()
    } else {
        message.lines().next().unwrap_or(message).trim().to_owned()
    }
}

impl Drop for LanguageServerClient {
    fn drop(&mut self) {
        self.stop();
    }
}

fn server_worker(
    root: PathBuf,
    language: ServerLanguage,
    commands: Receiver<ClientCommand>,
    events: Sender<Value>,
    context: Context,
) {
    let child = spawn_language_server(&root, language);
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            let help = if language == ServerLanguage::CSharp {
                "Fugh searched PATH and ~/.dotnet/tools. Check `dotnet tool list --global`, then use Toolchain Setup to install the .NET SDK and Roslyn if either is missing."
            } else {
                "Install the server or configure PATH."
            };
            send_event(
                &events,
                &context,
                json!({"method": "window/logMessage", "params": {
                    "message": format!("Could not start {}. {help} ({error})", language.command())
                }}),
            );
            return;
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        send_event(
            &events,
            &context,
            json!({"method": "window/logMessage", "params": {"message": "Language server has no stdin pipe."}}),
        );
        let _ = child.kill();
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return;
    };
    let (incoming_sender, incoming) = mpsc::channel();
    let reader_events = events.clone();
    let reader_context = context.clone();
    let _ = thread::Builder::new()
        .name("cairn-lsp-reader".to_owned())
        .spawn(move || read_messages(stdout, incoming_sender, reader_events, reader_context));

    let root_uri = file_uri(&root).unwrap_or_default();
    let initialize = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "processId": null,
            "clientInfo": {"name": "Fugh", "version": env!("CARGO_PKG_VERSION")},
            "rootUri": root_uri,
            "workspaceFolders": [{"uri": root_uri, "name": root.file_name().and_then(|n| n.to_str()).unwrap_or("project")}],
            "capabilities": {
                "textDocument": {
                    "synchronization": {"dynamicRegistration": false, "willSave": false, "didSave": true},
                    "diagnostic": {"dynamicRegistration": false},
                    "completion": {"completionItem": {"snippetSupport": false, "documentationFormat": ["markdown", "plaintext"]}},
                    "definition": {"linkSupport": true},
                    "references": {}, "hover": {"contentFormat": ["markdown", "plaintext"]},
                    "callHierarchy": {},
                    "signatureHelp": {
                        "dynamicRegistration": false,
                        "signatureInformation": {
                            "documentationFormat": ["markdown", "plaintext"],
                            "parameterInformation": {"labelOffsetSupport": true}
                        },
                        "contextSupport": true
                    },
                    "rename": {"prepareSupport": true}, "codeAction": {"codeActionLiteralSupport": {"codeActionKind": {"valueSet": ["quickfix", "refactor", "source"]}}},
                    "documentSymbol": {"hierarchicalDocumentSymbolSupport": true}
                },
                "workspace": {"workspaceFolders": true, "applyEdit": true, "configuration": true}
            }
        }
    });
    if write_message(&mut stdin, &initialize).is_err() {
        let _ = child.kill();
        return;
    }
    let initialized = loop {
        match incoming.recv_timeout(Duration::from_millis(100)) {
            Ok(message) if message.get("id").and_then(Value::as_u64) == Some(1) => {
                if message.get("error").is_some() {
                    send_event(
                        &events,
                        &context,
                        json!({"method":"window/logMessage","params":{"message":format!("{} rejected LSP initialization: {}", language.command(), message["error"] )}}),
                    );
                    let _ = child.kill();
                    return;
                }
                break true;
            }
            Ok(message) => {
                if message.get("method").is_some_and(Value::is_string)
                    && message.get("id").is_some()
                {
                    let _ = write_message(&mut stdin, &server_request_response(&message, &root));
                } else {
                    send_event(&events, &context, message);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if child.try_wait().ok().flatten().is_some() {
                    send_event(
                        &events,
                        &context,
                        json!({"method":"window/logMessage","params":{"message":format!("{} exited during startup.", language.command())}}),
                    );
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    };
    if initialized {
        let _ = write_message(
            &mut stdin,
            &json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        );
        send_event(
            &events,
            &context,
            json!({"method":"window/logMessage","params":{"message":format!("{} connected.", language.command()),"cairnConnected":true}}),
        );
    }

    loop {
        match commands.try_recv() {
            Ok(ClientCommand::Notification { method, params }) => {
                let message = json!({"jsonrpc":"2.0","method":method,"params":params});
                if write_message(&mut stdin, &message).is_err() {
                    break;
                }
            }
            Ok(ClientCommand::Request { id, method, params }) => {
                let message = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
                if write_message(&mut stdin, &message).is_err() {
                    break;
                }
            }
            Ok(ClientCommand::Response { id, result }) => {
                let message = json!({"jsonrpc":"2.0","id":id,"result":result});
                if write_message(&mut stdin, &message).is_err() {
                    break;
                }
            }
            Ok(ClientCommand::Shutdown) => {
                let _ = write_message(
                    &mut stdin,
                    &json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
                );
                let _ = write_message(
                    &mut stdin,
                    &json!({"jsonrpc":"2.0","method":"exit","params":null}),
                );
                let deadline = Instant::now() + Duration::from_millis(1500);
                while Instant::now() < deadline && child.try_wait().ok().flatten().is_none() {
                    thread::sleep(Duration::from_millis(25));
                }
                break;
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        match incoming.recv_timeout(Duration::from_millis(12)) {
            Ok(message) => {
                if message.get("method").is_some_and(Value::is_string)
                    && message.get("id").is_some()
                {
                    if message.get("method").and_then(Value::as_str)
                        == Some("workspace/diagnostic/refresh")
                    {
                        send_event(&events, &context, message.clone());
                    }
                    if message.get("method").and_then(Value::as_str) == Some("workspace/applyEdit")
                    {
                        send_event(&events, &context, message);
                    } else {
                        let response = server_request_response(&message, &root);
                        let _ = write_message(&mut stdin, &response);
                    }
                } else {
                    send_event(&events, &context, message);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if child.try_wait().ok().flatten().is_some() {
                    send_event(
                        &events,
                        &context,
                        json!({"method":"window/logMessage","params":{"message":format!("{} stopped.", language.command())}}),
                    );
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn_language_server(
    root: &Path,
    language: ServerLanguage,
) -> std::io::Result<std::process::Child> {
    let args = language.args(root);
    let mut command = Command::new(language.command());
    command
        .args(&args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if language == ServerLanguage::CSharp
        && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
    {
        let dotnet_root = PathBuf::from(home).join(".dotnet");
        let mut paths = vec![dotnet_root.join("tools"), dotnet_root.clone()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
        if dotnet_root.join("dotnet").is_file() {
            command.env("DOTNET_ROOT", dotnet_root);
        }
    }
    match command.spawn() {
        Ok(child) => Ok(child),
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && language == ServerLanguage::CSharp =>
        {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
            if let Some(home) = home {
                let dotnet_root = PathBuf::from(home).join(".dotnet");
                let installed = dotnet_root.join("tools").join("roslyn-language-server");
                let mut paths = vec![dotnet_root.join("tools"), dotnet_root.clone()];
                if let Some(existing) = std::env::var_os("PATH") {
                    paths.extend(std::env::split_paths(&existing));
                }
                let mut fallback = Command::new(installed);
                fallback
                    .args(args)
                    .current_dir(root)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                if dotnet_root.join("dotnet").is_file() {
                    fallback.env("DOTNET_ROOT", dotnet_root);
                }
                if let Ok(path) = std::env::join_paths(paths) {
                    fallback.env("PATH", path);
                }
                fallback.spawn()
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    }
}

fn send_event(events: &Sender<Value>, context: &Context, message: Value) {
    let _ = events.send(message);
    context.request_repaint();
}

fn read_messages(
    stdout: impl Read,
    sender: Sender<Value>,
    events: Sender<Value>,
    context: Context,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) if line.trim().is_empty() => break,
                Ok(_) => {
                    if let Some((key, value)) = line.split_once(':')
                        && key.eq_ignore_ascii_case("content-length")
                    {
                        content_length = value.trim().parse::<usize>().ok();
                    }
                }
                Err(error) => {
                    send_event(
                        &events,
                        &context,
                        json!({"method":"window/logMessage","params":{"message":format!("Language server read failed: {error}")}}),
                    );
                    return;
                }
            }
        }
        let Some(length) = content_length.filter(|length| *length <= 32 * 1024 * 1024) else {
            send_event(
                &events,
                &context,
                json!({"method":"window/logMessage","params":{"message":"Language server sent an invalid Content-Length header."}}),
            );
            return;
        };
        let mut body = vec![0_u8; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        match serde_json::from_slice::<Value>(&body) {
            Ok(message) => {
                if sender.send(message).is_err() {
                    return;
                }
                context.request_repaint();
            }
            Err(error) => {
                send_event(
                    &events,
                    &context,
                    json!({"method":"window/logMessage","params":{"message":format!("Language server sent invalid JSON: {error}")}}),
                );
            }
        }
    }
}

fn write_message(stdin: &mut ChildStdin, message: &Value) -> Result<(), std::io::Error> {
    let body = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len())?;
    stdin.write_all(&body)?;
    stdin.flush()
}

fn server_request_response(request: &Value, root: &Path) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let result = match method {
        "workspace/applyEdit" => {
            json!({"applied": false, "failureReason": "Workspace edits are unavailable until the editor finishes initializing."})
        }
        "workspace/workspaceFolders" => json!([{
            "uri": file_uri(root).unwrap_or_default(),
            "name": root.file_name().and_then(|name| name.to_str()).unwrap_or("project")
        }]),
        "workspace/configuration" => {
            let count = request
                .pointer("/params/items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            json!(vec![Value::Null; count])
        }
        _ => Value::Null,
    };
    json!({"jsonrpc":"2.0", "id":id, "result":result})
}

fn utf16_column(line: &str, char_column: usize) -> usize {
    line.chars().take(char_column).map(char::len_utf16).sum()
}

fn char_column(line: &str, utf16_units: usize) -> usize {
    let mut units = 0;
    for (index, character) in line.chars().enumerate() {
        if units >= utf16_units {
            return index;
        }
        units += character.len_utf16();
    }
    line.chars().count()
}

fn file_uri(path: &Path) -> Option<String> {
    Url::from_file_path(path).ok().map(|url| url.to_string())
}

pub fn parse_file_uri(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

pub fn definition_location(response: &Value) -> Option<(PathBuf, usize)> {
    let result = response.get("result")?;
    let location = result
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(result);
    let uri = location
        .get("uri")
        .or_else(|| location.get("targetUri"))?
        .as_str()?;
    let line = location
        .pointer("/range/start/line")
        .or_else(|| location.pointer("/targetSelectionRange/start/line"))
        .or_else(|| location.pointer("/targetRange/start/line"))?
        .as_u64()? as usize
        + 1;
    Some((parse_file_uri(uri)?, line))
}

pub fn reference_locations(response: &Value) -> Vec<ReferenceLocation> {
    response
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|location| {
            let uri = location
                .get("uri")
                .or_else(|| location.get("targetUri"))?
                .as_str()?;
            let range = location
                .get("range")
                .or_else(|| location.get("targetSelectionRange"))
                .or_else(|| location.get("targetRange"))?;
            let start = range.get("start")?;
            let end = range.get("end").unwrap_or(start);
            Some(ReferenceLocation {
                path: parse_file_uri(uri)?,
                line: start.get("line")?.as_u64()? as usize + 1,
                column: start.get("character")?.as_u64()? as usize + 1,
                end_line: end.get("line")?.as_u64()? as usize + 1,
                end_column: end.get("character")?.as_u64()? as usize + 1,
                is_declaration: location.get("isDeclaration").and_then(Value::as_bool),
            })
        })
        .collect()
}

pub fn call_hierarchy_items(response: &Value) -> Vec<Value> {
    response
        .get("result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub fn document_symbol_entries(response: &Value, default_path: &Path) -> Vec<DocumentSymbolEntry> {
    fn append(
        symbol: &Value,
        parent_path: &Path,
        depth: usize,
        output: &mut Vec<DocumentSymbolEntry>,
    ) {
        let location = symbol.get("location");
        let path = location
            .and_then(|location| location.get("uri"))
            .and_then(Value::as_str)
            .and_then(parse_file_uri)
            .unwrap_or_else(|| parent_path.to_path_buf());
        let position = symbol
            .pointer("/selectionRange/start")
            .or_else(|| symbol.pointer("/range/start"))
            .or_else(|| location.and_then(|location| location.pointer("/range/start")));
        if let (Some(name), Some(position)) = (symbol.get("name").and_then(Value::as_str), position)
        {
            output.push(DocumentSymbolEntry {
                name: name.to_owned(),
                detail: symbol
                    .get("detail")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                kind: symbol.get("kind").and_then(Value::as_u64),
                path: path.clone(),
                line: position.get("line").and_then(Value::as_u64).unwrap_or(0) as usize + 1,
                column: position
                    .get("character")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize
                    + 1,
                depth,
            });
        }
        if let Some(children) = symbol.get("children").and_then(Value::as_array) {
            for child in children {
                append(child, &path, depth.saturating_add(1), output);
            }
        }
    }

    let mut symbols = Vec::new();
    for symbol in response
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        append(symbol, default_path, 0, &mut symbols);
    }
    symbols
}

pub fn call_hierarchy_entries(response: &Value, incoming: bool) -> Vec<CallHierarchyEntry> {
    response
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|call| {
            let Some(item) = call.get(if incoming { "from" } else { "to" }) else {
                return Vec::new();
            };
            let Some(uri) = item.get("uri").and_then(Value::as_str) else {
                return Vec::new();
            };
            let Some(path) = parse_file_uri(uri) else {
                return Vec::new();
            };
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                return Vec::new();
            };
            let detail = item
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let positions = if incoming {
                call.get("fromRanges")
                    .and_then(Value::as_array)
                    .map(|ranges| {
                        ranges
                            .iter()
                            .filter_map(|range| range.get("start"))
                            .collect::<Vec<_>>()
                    })
                    .filter(|positions: &Vec<_>| !positions.is_empty())
                    .unwrap_or_else(|| {
                        item.pointer("/selectionRange/start")
                            .or_else(|| item.pointer("/range/start"))
                            .into_iter()
                            .collect()
                    })
            } else {
                item.pointer("/selectionRange/start")
                    .or_else(|| item.pointer("/range/start"))
                    .into_iter()
                    .collect()
            };
            positions
                .into_iter()
                .filter_map(move |position| {
                    Some(CallHierarchyEntry {
                        name: name.to_owned(),
                        detail: detail.clone(),
                        path: path.clone(),
                        line: position.get("line")?.as_u64()? as usize + 1,
                        column: position.get("character")?.as_u64()? as usize + 1,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn hover_contents(response: &Value) -> Option<String> {
    let contents = response.pointer("/result/contents")?;
    let mut parts = Vec::new();
    match contents {
        Value::String(text) => parts.push(text.as_str()),
        Value::Array(items) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    parts.push(text);
                } else if let Some(text) = item.get("value").and_then(Value::as_str) {
                    parts.push(text);
                }
            }
        }
        Value::Object(_) => {
            if let Some(text) = contents.get("value").and_then(Value::as_str) {
                parts.push(text);
            }
        }
        _ => {}
    }
    // Language servers commonly return hover markup as a fenced code block,
    // e.g. ```csharp\nvoid Foo(...)\n```. The tooltip is already visually
    // distinct, so the fence and language tag only add noise there.
    let text = parts
        .into_iter()
        .map(clean_hover_markup)
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.trim().is_empty()).then_some(text)
}

fn clean_hover_markup(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        if let Some((language, body)) = rest.split_once('\n') {
            if body.trim_end().ends_with("```") {
                let body = body.trim_end().strip_suffix("```").unwrap_or(body).trim();
                let _ = language; // The tooltip already conveys that this is code.
                return body.to_owned();
            }
        }
    }
    trimmed.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        LanguageServerClient, ServerLanguage, Session, definition_location, file_uri,
        hover_contents, parse_file_uri, reference_locations, server_request_response,
    };
    use std::collections::BTreeMap;
    use std::sync::mpsc;

    #[test]
    fn file_uris_round_trip_paths_with_spaces() {
        let path = std::env::temp_dir()
            .join("Cairn Project")
            .join("src")
            .join("main.cpp");
        let uri = file_uri(&path).unwrap();
        assert!(uri.contains("Cairn%20Project"));
        assert_eq!(parse_file_uri(&uri).unwrap(), path);
    }

    #[test]
    fn cpp_server_receives_the_generated_cmake_compilation_database_path() {
        let root = std::env::temp_dir().join("cairn-lsp-compile-db");
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::write(root.join("build/compile_commands.json"), "[]").unwrap();
        let args = ServerLanguage::Cpp.args(&root);
        assert!(
            args.iter()
                .any(|arg| arg
                    == &format!("--compile-commands-dir={}", root.join("build").display()))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn definition_location_supports_location_links() {
        let path = std::env::temp_dir().join("definition target.cpp");
        let uri = file_uri(&path).unwrap();
        let response = serde_json::json!({
            "result": [{
                "targetUri": uri,
                "targetSelectionRange": {"start": {"line": 8, "character": 2}}
            }]
        });
        assert_eq!(definition_location(&response), Some((path, 9)));
    }

    #[test]
    fn reference_locations_parse_all_file_uris_and_one_based_lines() {
        let first = std::env::temp_dir().join("reference one.cpp");
        let second = std::env::temp_dir().join("reference two.cpp");
        let response = serde_json::json!({"result": [
            {"uri": file_uri(&first).unwrap(), "range": {"start": {"line": 2, "character": 4}, "end": {"line": 2, "character": 9}}, "isDeclaration": false},
            {"uri": file_uri(&second).unwrap(), "range": {"start": {"line": 8, "character": 0}, "end": {"line": 8, "character": 5}}, "isDeclaration": true}
        ]});
        let locations = reference_locations(&response);
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].path, first);
        assert_eq!(locations[0].line, 3);
        assert_eq!(locations[0].column, 5);
        assert_eq!(locations[0].is_declaration, Some(false));
        assert_eq!(locations[1].path, second);
        assert_eq!(locations[1].line, 9);
        assert_eq!(locations[1].is_declaration, Some(true));
    }

    #[test]
    fn call_hierarchy_entries_read_incoming_and_outgoing_locations() {
        let caller = std::env::temp_dir().join("caller.cpp");
        let callee = std::env::temp_dir().join("callee.cpp");
        let response = serde_json::json!({"result": [{
            "from": {"name": "caller", "uri": file_uri(&caller).unwrap(), "selectionRange": {"start": {"line": 4, "character": 3}}},
            "to": {"name": "callee", "detail": "void()", "uri": file_uri(&callee).unwrap(), "range": {"start": {"line": 10, "character": 1}}},
            "fromRanges": [{"start": {"line": 7, "character": 8}, "end": {"line": 7, "character": 14}}]
        }]});
        let incoming = super::call_hierarchy_entries(&response, true);
        let outgoing = super::call_hierarchy_entries(&response, false);
        assert_eq!(incoming[0].name, "caller");
        assert_eq!(incoming[0].line, 8);
        assert_eq!(incoming[0].column, 9);
        assert_eq!(outgoing[0].name, "callee");
        assert_eq!(outgoing[0].line, 11);
        assert_eq!(outgoing[0].detail.as_deref(), Some("void()"));
    }

    #[test]
    fn hover_contents_accepts_markdown_and_markup_arrays() {
        let markdown = serde_json::json!({"result": {"contents": {
            "kind": "markdown", "value": "`Widget`\n\nA widget."
        }}});
        assert_eq!(
            hover_contents(&markdown).as_deref(),
            Some("`Widget`\n\nA widget.")
        );
        let array = serde_json::json!({"result": {"contents": [
            {"language": "csharp", "value": "public class Widget"},
            "A widget."
        ]}});
        assert_eq!(
            hover_contents(&array).as_deref(),
            Some("public class Widget\n\nA widget.")
        );
        let fenced = serde_json::json!({"result": {"contents": {
            "kind": "markdown", "value": "```csharp\nvoid Run(string first, string second)\n```"
        }}});
        assert_eq!(
            hover_contents(&fenced).as_deref(),
            Some("void Run(string first, string second)")
        );
    }

    #[test]
    fn server_configuration_requests_receive_a_protocol_response() {
        let request = serde_json::json!({
            "jsonrpc":"2.0", "id": 31, "method":"workspace/configuration",
            "params": {"items": [{"section":"csharp"}, {"section":"editor"}]}
        });
        let response = server_request_response(&request, &std::env::temp_dir());
        assert_eq!(response["id"], 31);
        assert_eq!(response["result"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn pull_diagnostics_are_added_to_the_problems_list_data() {
        let path = std::env::temp_dir().join("diagnostic sample.cs");
        let (sender, _commands) = mpsc::channel();
        let (_events, receiver) = mpsc::channel();
        let mut client = LanguageServerClient::default();
        let mut open_documents = BTreeMap::new();
        open_documents.insert(path.clone(), "void Run() {\n    sadassdsas;\n}".to_owned());
        let mut open_versions = BTreeMap::new();
        open_versions.insert(path.clone(), 4);
        client.session = Some(Session {
            root: std::env::temp_dir(),
            language: ServerLanguage::CSharp,
            sender,
            receiver,
            document: Some(path.clone()),
            version: 4,
            text: open_documents[&path].clone(),
            open_documents,
            open_versions,
        });
        let response = serde_json::json!({
            "result": {
                "kind": "full",
                "items": [{
                    "range": {
                        "start": {"line": 1, "character": 4},
                        "end": {"line": 1, "character": 14}
                    },
                    "severity": 1,
                    "code": "CS0103",
                    "source": "Roslyn",
                    "message": "The name 'sadassdsas' does not exist in the current context."
                }]
            }
        });
        client.read_pull_diagnostics(&response, &path);
        assert_eq!(client.diagnostics.len(), 1);
        assert_eq!(client.diagnostics[0].severity, 1);
        assert_eq!(client.diagnostics[0].line, 2);
        assert_eq!(client.diagnostics[0].column, 5);
        assert!(client.diagnostics[0].message.contains("sadassdsas"));
    }
}
