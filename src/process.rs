use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(any(windows, test))]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use eframe::egui;

use crate::project::{ProjectInfo, ProjectLanguage, ProjectProperties, ProjectTemplate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    Build,
    BuildRelease,
    Rebuild,
    Clean,
    Analyze,
    Publish,
    Package,
    Run,
    Debug,
}

impl ProcessAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Build => "Build",
            Self::BuildRelease => "Build Release",
            Self::Rebuild => "Rebuild",
            Self::Clean => "Clean",
            Self::Analyze => "Run Code Analysis",
            Self::Publish => "Publish",
            Self::Package => "Create Package",
            Self::Run => "Run Without Debugging",
            Self::Debug => "Debug",
        }
    }
}

#[derive(Debug)]
enum ProcessEvent {
    Output(String),
    StdinReady(Sender<String>),
    RuntimePid(u32),
    Finished(Result<(), String>),
}

#[derive(Debug)]
pub struct ProcessState {
    pub running: bool,
    pub stopping: bool,
    pub active_action: Option<ProcessAction>,
    pub output: Vec<String>,
    pub last_result: Option<Result<(), String>>,
    pub stdin_buffer: String,
    pub runtime_pid: Option<u32>,
    pub debugger_active: bool,
    pub debug_location: Option<DebugLocation>,
    pub debug_variables: Vec<DebugVariable>,
    pub debug_call_stack: Vec<DebugFrame>,
    pub selected_debug_frame: usize,
    pub debug_stop_sequence: u64,
    pub pending_evaluation: Option<String>,
    pub debug_evaluation: Option<(String, String)>,
    restart_pending: bool,
    last_launch: Option<ProcessLaunch>,
    stdin_sender: Option<Sender<String>>,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugLocation {
    pub file: PathBuf,
    pub line: usize,
    pub function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugVariable {
    pub name: String,
    pub value: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugFrame {
    pub level: usize,
    pub function: Option<String>,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub module: Option<String>,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self {
            running: false,
            stopping: false,
            active_action: None,
            output: Vec::new(),
            last_result: None,
            stdin_buffer: String::new(),
            runtime_pid: None,
            debugger_active: false,
            debug_location: None,
            debug_variables: Vec::new(),
            debug_call_stack: Vec::new(),
            selected_debug_frame: 0,
            debug_stop_sequence: 0,
            pending_evaluation: None,
            debug_evaluation: None,
            restart_pending: false,
            last_launch: None,
            stdin_sender: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone)]
struct ProcessLaunch {
    project: ProjectInfo,
    action: ProcessAction,
    breakpoints: Vec<(PathBuf, usize)>,
}

pub struct ProcessManager {
    handle: ProcessHandle,
    receiver: Receiver<ProcessEvent>,
}

#[derive(Clone)]
pub struct ProcessHandle {
    state: Arc<Mutex<ProcessState>>,
    sender: Sender<ProcessEvent>,
    repaint: egui::Context,
}

impl ProcessManager {
    pub fn new(repaint: egui::Context) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            handle: ProcessHandle {
                state: Arc::new(Mutex::new(ProcessState::default())),
                sender,
                repaint,
            },
            receiver,
        }
    }

    pub fn handle(&self) -> ProcessHandle {
        self.handle.clone()
    }

    pub fn poll(&self) {
        while let Ok(event) = self.receiver.try_recv() {
            let mut state = self
                .handle
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut restart = None;
            match event {
                ProcessEvent::Output(line) => {
                    let debugger_stopped = state.debugger_active && line.starts_with("*stopped");
                    let debugger_exited = debugger_stopped && line.contains("reason=\"exited");
                    let debugger_paused = debugger_stopped
                        && !debugger_exited
                        && parse_debug_location(&line).is_some();
                    if debugger_paused {
                        if let Some(location) = parse_debug_location(&line) {
                            state.debug_location = Some(location);
                            state.debug_stop_sequence = state.debug_stop_sequence.wrapping_add(1);
                        }
                    } else if debugger_stopped {
                        state.debug_location = None;
                        state.debug_variables.clear();
                        state.debug_call_stack.clear();
                    }
                    if line.contains("variables=[") {
                        state.debug_variables = parse_debug_variables(&line);
                    }
                    if line.contains("^done,stack=[") {
                        state.debug_call_stack = parse_debug_call_stack(&line);
                    }
                    if line.contains("^done,value=")
                        && let Some(expression) = state.pending_evaluation.take()
                        && let Some(value) = mi_field(&line, "value")
                    {
                        state.debug_evaluation = Some((expression, value));
                    }
                    state.output.push(line);
                    // MI debuggers stay alive at their prompt after the inferior
                    // exits. Close the session so the IDE cannot keep showing an
                    // active Debug action after the program window has closed.
                    if debugger_paused && let Some(stdin) = &state.stdin_sender {
                        let _ = stdin.send("-stack-list-frames\n".to_owned());
                        let _ = stdin.send("-stack-list-variables --all-values\n".to_owned());
                    }
                    if state.output.len() > 10_000 {
                        let excess = state.output.len() - 10_000;
                        state.output.drain(0..excess);
                    }
                }
                ProcessEvent::StdinReady(sender) => state.stdin_sender = Some(sender),
                ProcessEvent::RuntimePid(pid) => state.runtime_pid = Some(pid),
                ProcessEvent::Finished(result) => {
                    state.running = false;
                    state.stopping = false;
                    state.active_action = None;
                    state.stdin_sender = None;
                    state.runtime_pid = None;
                    state.debugger_active = false;
                    if state.restart_pending {
                        state.restart_pending = false;
                        restart = state.last_launch.clone();
                    }
                    if let Err(message) = &result {
                        state.output.push(message.clone());
                    }
                    state.last_result = Some(result);
                }
            }
            drop(state);
            if let Some(launch) = restart {
                if let Err(error) = self.handle.start_with_breakpoints(
                    launch.project,
                    launch.action,
                    launch.breakpoints,
                ) {
                    let mut state = self
                        .handle
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    state
                        .output
                        .push(format!("Could not restart process: {error}"));
                }
            }
        }
    }
}

impl ProcessHandle {
    pub fn state(&self) -> Arc<Mutex<ProcessState>> {
        self.state.clone()
    }

    pub fn start(&self, project: ProjectInfo, action: ProcessAction) -> Result<(), String> {
        self.start_with_breakpoints(project, action, Vec::new())
    }

    pub fn start_with_breakpoints(
        &self,
        project: ProjectInfo,
        action: ProcessAction,
        breakpoints: Vec<(PathBuf, usize)>,
    ) -> Result<(), String> {
        if project.language == ProjectLanguage::Cpp
            && project.template == ProjectTemplate::ClassLibrary
            && matches!(action, ProcessAction::Run | ProcessAction::Debug)
        {
            return Err(
                "A C++ class library cannot be run or debugged directly; create a console app that links it."
                    .to_owned(),
            );
        }
        if action == ProcessAction::Debug
            && project.language == ProjectLanguage::CSharp
            && project.template == ProjectTemplate::ClassLibrary
        {
            return Err(
                "A C# class library cannot be debugged directly; create a console or Avalonia app."
                    .to_owned(),
            );
        }
        crate::app::designer::repair_stale_avalonia_event_handlers(&project)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.running {
            return Err("A build, run, or debug process is already active.".to_owned());
        }

        state.running = true;
        state.stopping = false;
        state.active_action = Some(action);
        state
            .output
            .push(format!("\n--- {} started ---", action.label()));
        state.last_launch = Some(ProcessLaunch {
            project: project.clone(),
            action,
            breakpoints: breakpoints.clone(),
        });
        state.restart_pending = false;
        state.last_result = None;
        state.stdin_buffer.clear();
        state.runtime_pid = None;
        state.stdin_sender = None;
        state.debugger_active = action == ProcessAction::Debug;
        state.debug_location = None;
        state.debug_variables.clear();
        state.debug_call_stack.clear();
        state.selected_debug_frame = 0;
        state.debug_stop_sequence = 0;
        state.pending_evaluation = None;
        state.debug_evaluation = None;
        state.cancel.store(false, Ordering::Relaxed);
        let cancel = state.cancel.clone();
        drop(state);

        let sender = self.sender.clone();
        let repaint = self.repaint.clone();
        thread::Builder::new()
            .name(format!("cairn-{}", action.label().to_lowercase()))
            .spawn(move || {
                let result = match action {
                    ProcessAction::Run => execute_action(
                        &project,
                        ProcessAction::Build,
                        &cancel,
                        &sender,
                        &repaint,
                        &breakpoints,
                    )
                    .and_then(|()| {
                        // Avalonia applications are desktop windows. Starting them
                        // through a terminal wrapper can detach them from the
                        // graphical session (notably on Linux), leaving only the
                        // terminal visible. Run them as a managed child process;
                        // stdout/stderr and Stop remain available in the IDE.
                        if project.language == ProjectLanguage::CSharp
                            && project.template == ProjectTemplate::AvaloniaApp
                        {
                            return run_project_interactive(&project, &cancel, &sender, &repaint);
                        }
                        #[cfg(windows)]
                        {
                            launch_terminal(
                                &project,
                                false,
                                &cancel,
                                &sender,
                                &repaint,
                                &breakpoints,
                            )
                        }
                        #[cfg(target_os = "linux")]
                        {
                            launch_terminal(
                                &project,
                                false,
                                &cancel,
                                &sender,
                                &repaint,
                                &breakpoints,
                            )
                        }
                        #[cfg(not(any(windows, target_os = "linux")))]
                        {
                            run_project_interactive(&project, &cancel, &sender, &repaint)
                        }
                    }),
                    ProcessAction::Debug if project.language == ProjectLanguage::CSharp => {
                        execute_action(
                            &project,
                            ProcessAction::Build,
                            &cancel,
                            &sender,
                            &repaint,
                            &breakpoints,
                        )
                        .and_then(|()| {
                            let assembly = csharp_assembly(&project)?;
                            let debugger = crate::toolchain::ToolchainManager::netcoredbg_executable()
                                .ok_or_else(|| "NetCoreDbg is missing. Install it from Toolchain Setup or add it to PATH.".to_owned())?;
                            run_mi_debugger(
                                &debugger,
                                &[
                                    "--interpreter=mi".to_owned(),
                                    "--".to_owned(),
                                    "dotnet".to_owned(),
                                    assembly.display().to_string(),
                                ],
                                &project.path,
                                &project,
                                &breakpoints,
                                &cancel,
                                &sender,
                                &repaint,
                            )
                        })
                    }
                    ProcessAction::Debug => execute_action(
                        &project,
                        ProcessAction::Build,
                        &cancel,
                        &sender,
                        &repaint,
                        &breakpoints,
                    )
                    .and_then(|()| {
                        let executable = cpp_executable(&project)?;
                        run_mi_debugger(
                            Path::new("gdb"),
                            &[
                                "--interpreter=mi2".to_owned(),
                                "--quiet".to_owned(),
                                executable.display().to_string(),
                            ],
                            &project.path,
                            &project,
                            &breakpoints,
                            &cancel,
                            &sender,
                            &repaint,
                        )
                    }),
                    _ => execute_action(&project, action, &cancel, &sender, &repaint, &breakpoints),
                };
                let result = if cancel.load(Ordering::Relaxed)
                    && result.as_ref().is_err_and(|error| error == "Process cancelled.")
                {
                    Ok(())
                } else {
                    result
                };
                let _ = sender.send(ProcessEvent::Finished(result));
                repaint.request_repaint();
            })
            .map_err(|error| {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.running = false;
                state.active_action = None;
                format!("Could not start process worker: {error}")
            })?;
        Ok(())
    }

    pub fn cancel(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.running {
            state.stopping = true;
            state.cancel.store(true, Ordering::Relaxed);
        }
        drop(state);
        self.repaint.request_repaint();
    }

    pub fn send_stdin(&self, text: String) -> Result<(), String> {
        let sender = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stdin_sender
            .clone()
            .ok_or_else(|| "The active process does not accept input.".to_owned())?;
        sender
            .send(text)
            .map_err(|_| "The process input stream has closed.".to_owned())
    }

    pub fn debugger_command(&self, command: String) -> Result<(), String> {
        let command = command.trim();
        if !command.starts_with('-') {
            return Err(
                "Debugger commands must use the GDB/MI command form beginning with '-'.".to_owned(),
            );
        }
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !state.debugger_active {
                return Err("No integrated debugger session is active.".to_owned());
            }
            if matches!(
                command.split_whitespace().next(),
                Some(
                    "-exec-continue"
                        | "-exec-next"
                        | "-exec-next-instruction"
                        | "-exec-step"
                        | "-exec-step-instruction"
                        | "-exec-finish"
                )
            ) {
                state.debug_location = None;
            }
        }
        self.send_stdin(format!("{command}\n"))
    }

    pub fn evaluate_expression(&self, expression: &str) -> Result<(), String> {
        self.debugger_command(format!(
            "-data-evaluate-expression {}",
            mi_quote(expression)
        ))?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.pending_evaluation = Some(expression.to_owned());
        state.debug_evaluation = None;
        Ok(())
    }

    pub fn select_debug_frame(&self, level: usize) -> Result<(), String> {
        self.debugger_command(format!("-stack-select-frame {level}"))?;
        self.debugger_command("-stack-list-variables --all-values".to_owned())?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.selected_debug_frame = level;
        state.debug_variables.clear();
        if let Some(frame) = state
            .debug_call_stack
            .iter()
            .find(|frame| frame.level == level)
        {
            if let (Some(file), Some(line)) = (&frame.file, frame.line) {
                state.debug_location = Some(DebugLocation {
                    file: file.clone(),
                    line,
                    function: frame.function.clone(),
                });
                state.debug_stop_sequence = state.debug_stop_sequence.wrapping_add(1);
            }
        }
        Ok(())
    }

    pub fn sync_breakpoints(&self, breakpoints: &[(PathBuf, usize)]) -> Result<(), String> {
        let active = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .debugger_active;
        if !active {
            return Ok(());
        }
        self.send_stdin("-break-delete\n".to_owned())?;
        for (path, line) in breakpoints {
            self.send_stdin(format!(
                "-break-insert {}\n",
                mi_quote(&format!("{}:{line}", path.display()))
            ))?;
        }
        Ok(())
    }

    pub fn restart(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.running {
            state.restart_pending = true;
            state.stopping = true;
            state.cancel.store(true, Ordering::Relaxed);
            return Ok(());
        }
        let launch = state
            .last_launch
            .clone()
            .ok_or_else(|| "There is no process to restart yet.".to_owned())?;
        drop(state);
        self.start_with_breakpoints(launch.project, launch.action, launch.breakpoints)
    }
}

fn execute_action(
    project: &ProjectInfo,
    action: ProcessAction,
    cancel: &AtomicBool,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
    breakpoints: &[(PathBuf, usize)],
) -> Result<(), String> {
    validate_build_platform(project)?;
    if project.language == ProjectLanguage::Cpp
        && project.template == ProjectTemplate::ClassLibrary
        && matches!(action, ProcessAction::Run | ProcessAction::Debug)
    {
        return Err(
            "A C++ class library cannot be run or debugged directly; create a console app that links it."
                .to_owned(),
        );
    }
    if project.template == ProjectTemplate::VisualStudioCpp {
        return Err(
            "This Visual Studio C++ project was imported for browsing and editing. Building .vcxproj files is not supported by the current CMake build runner.".to_owned(),
        );
    }
    match (project.language, action) {
        (
            ProjectLanguage::Cpp,
            ProcessAction::Build | ProcessAction::BuildRelease | ProcessAction::Rebuild,
        ) => {
            let configuration = project_build_configuration(project, action)?;
            let properties_include = write_cpp_cmake_properties(project)?
                .to_string_lossy()
                .replace('\\', "/");
            let configure_args = vec![
                "-S".to_owned(),
                project.path.display().to_string(),
                "-B".to_owned(),
                project.path.join("build").display().to_string(),
                format!("-DCMAKE_BUILD_TYPE={configuration}"),
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON".to_owned(),
                format!("-DCMAKE_PROJECT_INCLUDE={properties_include}"),
            ];
            #[cfg(windows)]
            let configure_args = {
                let mut configure_args = configure_args;
                if tool_available("g++") || tool_available("clang++") {
                    if tool_available("mingw32-make") {
                        configure_args.extend(["-G".to_owned(), "MinGW Makefiles".to_owned()]);
                    } else if tool_available("ninja") {
                        configure_args.extend(["-G".to_owned(), "Ninja".to_owned()]);
                    }
                }
                configure_args
            };
            run_checked(
                "cmake",
                configure_args,
                &project.path,
                cancel,
                sender,
                repaint,
            )?;
            let mut build_args = vec![
                "--build".to_owned(),
                project.path.join("build").display().to_string(),
                "--config".to_owned(),
                configuration.clone(),
            ];
            if action == ProcessAction::Rebuild {
                build_args.push("--clean-first".to_owned());
            }
            run_checked("cmake", build_args, &project.path, cancel, sender, repaint)
        }
        (ProjectLanguage::Cpp, ProcessAction::Clean) => {
            let build_directory = project.path.join("build");
            if !build_directory.join("CMakeCache.txt").is_file() {
                return Ok(());
            }
            run_checked(
                "cmake",
                [
                    "--build".to_owned(),
                    build_directory.display().to_string(),
                    "--target".to_owned(),
                    "clean".to_owned(),
                ],
                &project.path,
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::Cpp, ProcessAction::Analyze) => {
            run_cpp_code_analysis(project, cancel, sender, repaint)
        }
        (ProjectLanguage::Cpp, ProcessAction::Publish) => {
            execute_action(
                project,
                ProcessAction::BuildRelease,
                cancel,
                sender,
                repaint,
                breakpoints,
            )?;
            run_checked(
                "cmake",
                [
                    "--install".to_owned(),
                    project.path.join("build").display().to_string(),
                    "--config".to_owned(),
                    "Release".to_owned(),
                    "--prefix".to_owned(),
                    project.path.join("publish").display().to_string(),
                ],
                &project.path,
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::Cpp, ProcessAction::Package) => {
            execute_action(
                project,
                ProcessAction::BuildRelease,
                cancel,
                sender,
                repaint,
                breakpoints,
            )?;
            if !project.path.join("build/CPackConfig.cmake").is_file() {
                return Err(
                    "This CMake project has no CPack configuration. Add include(CPack) to CMakeLists.txt, then retry packaging."
                        .to_owned(),
                );
            }
            run_checked(
                "cmake",
                [
                    "--build".to_owned(),
                    project.path.join("build").display().to_string(),
                    "--config".to_owned(),
                    "Release".to_owned(),
                    "--target".to_owned(),
                    "package".to_owned(),
                ],
                &project.path,
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Build | ProcessAction::BuildRelease) => {
            let configuration = project_build_configuration(project, action)?;
            let mut args = vec![
                "build".to_owned(),
                csproj_path(project).display().to_string(),
                "--configuration".to_owned(),
                configuration,
                "--verbosity".to_owned(),
                "minimal".to_owned(),
            ];
            args.extend(csharp_build_properties(project));
            run_checked_for_project(
                "dotnet",
                args,
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Rebuild) => {
            let configuration = project_build_configuration(project, action)?;
            let mut clean_args = vec![
                "clean".to_owned(),
                csproj_path(project).display().to_string(),
                "--configuration".to_owned(),
                configuration.clone(),
                "--verbosity".to_owned(),
                "minimal".to_owned(),
            ];
            clean_args.extend(csharp_build_properties(project));
            run_checked_for_project(
                "dotnet",
                clean_args,
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )?;
            let mut build_args = vec![
                "build".to_owned(),
                csproj_path(project).display().to_string(),
                "--configuration".to_owned(),
                configuration,
                "--verbosity".to_owned(),
                "minimal".to_owned(),
            ];
            build_args.extend(csharp_build_properties(project));
            run_checked_for_project(
                "dotnet",
                build_args,
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Clean) => {
            let mut args = vec![
                "clean".to_owned(),
                csproj_path(project).display().to_string(),
                "--verbosity".to_owned(),
                "minimal".to_owned(),
            ];
            args.extend(csharp_build_properties(project));
            run_checked_for_project(
                "dotnet",
                args,
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Analyze) => run_checked_for_project(
            "dotnet",
            [
                "format".to_owned(),
                "analyzers".to_owned(),
                csproj_path(project).display().to_string(),
                "--severity".to_owned(),
                "info".to_owned(),
                "--verify-no-changes".to_owned(),
            ],
            &project.path,
            Some(project),
            cancel,
            sender,
            repaint,
        ),
        (ProjectLanguage::CSharp, ProcessAction::Publish) => {
            let mut args = vec![
                "publish".to_owned(),
                csproj_path(project).display().to_string(),
                "--configuration".to_owned(),
                "Release".to_owned(),
                "--verbosity".to_owned(),
                "minimal".to_owned(),
            ];
            args.extend(csharp_build_properties(project));
            run_checked_for_project(
                "dotnet",
                args,
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Package) => {
            if project.template != ProjectTemplate::ClassLibrary {
                return Err("Create Package is available for C# class libraries.".to_owned());
            }
            let mut args = vec![
                "pack".to_owned(),
                csproj_path(project).display().to_string(),
                "--configuration".to_owned(),
                "Release".to_owned(),
                "--verbosity".to_owned(),
                "minimal".to_owned(),
            ];
            args.extend(csharp_build_properties(project));
            run_checked_for_project(
                "dotnet",
                args,
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::Cpp, ProcessAction::Run) => {
            let executable = cpp_executable(project)?;
            run_checked_for_project(
                executable.to_str().ok_or_else(|| {
                    format!(
                        "Executable path is not valid Unicode: {}",
                        executable.display()
                    )
                })?,
                Vec::<String>::new(),
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Run)
            if matches!(
                project.template,
                ProjectTemplate::Console | ProjectTemplate::AvaloniaApp
            ) =>
        {
            let configuration = project_build_configuration(project, ProcessAction::Build)?;
            let mut args = vec![
                "run".to_owned(),
                "--no-launch-profile".to_owned(),
                "--configuration".to_owned(),
                configuration,
                "--project".to_owned(),
                csproj_path(project).display().to_string(),
            ];
            args.extend(csharp_run_properties(project));
            run_checked_for_project(
                "dotnet",
                args,
                &project.path,
                Some(project),
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Run) => Err(
            "Class libraries cannot be run directly; create a console application instead."
                .to_owned(),
        ),
        (ProjectLanguage::Cpp, ProcessAction::Debug) => {
            let executable = cpp_executable(project)?;
            let executable_text = executable.to_str().ok_or_else(|| {
                format!(
                    "Executable path is not valid Unicode: {}",
                    executable.display()
                )
            })?;
            run_checked(
                "gdb",
                gdb_debug_arguments(breakpoints)
                    .into_iter()
                    .chain(["--args".to_owned(), executable_text.to_owned()]),
                &project.path,
                cancel,
                sender,
                repaint,
            )
        }
        (ProjectLanguage::CSharp, ProcessAction::Debug) => Err(
            "A C# class library cannot be debugged directly; create a console or Avalonia app."
                .to_owned(),
        ),
    }
}

fn run_cpp_code_analysis(
    project: &ProjectInfo,
    cancel: &AtomicBool,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
) -> Result<(), String> {
    let build_directory = project.path.join("build");
    if !build_directory.join("compile_commands.json").is_file() {
        return Err(
            "Build the C++ project once first so CMake can generate compile_commands.json for clang-tidy."
                .to_owned(),
        );
    }
    let sources = cpp_analysis_sources(&project.path)?;
    if sources.is_empty() {
        return Err("No C++ source files were found for code analysis.".to_owned());
    }
    let mut args = vec!["-p".to_owned(), build_directory.display().to_string()];
    args.extend(sources.into_iter().map(|path| path.display().to_string()));
    run_checked_for_project(
        "clang-tidy",
        args,
        &project.path,
        Some(project),
        cancel,
        sender,
        repaint,
    )
}

fn cpp_analysis_sources(project_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![project_root.to_path_buf()];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| {
            format!(
                "Could not scan '{}' for C++ files: {error}",
                directory.display()
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("Could not read project entry: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Could not inspect project entry: {error}"))?;
            if file_type.is_dir() {
                if !matches!(
                    entry.file_name().to_str(),
                    Some("build" | "target" | "bin" | "obj" | ".git" | ".fugh" | ".vs")
                ) {
                    pending.push(entry.path());
                }
            } else if file_type.is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "c" | "cc" | "cpp" | "cxx"
                        )
                    })
            {
                sources.push(entry.path());
            }
        }
    }
    sources.sort();
    Ok(sources)
}

fn csproj_path(project: &ProjectInfo) -> PathBuf {
    project.project_file.as_ref().map_or_else(
        || project.path.join(format!("{}.csproj", project.name)),
        |relative| project.path.join(relative),
    )
}

fn write_cpp_cmake_properties(project: &ProjectInfo) -> Result<PathBuf, String> {
    let standard = project.properties.cpp.standard.trim();
    if !standard.is_empty() && !matches!(standard, "14" | "17" | "20" | "23" | "26") {
        return Err(format!(
            "Unsupported C++ standard '{standard}' in project properties."
        ));
    }
    let values = project
        .properties
        .cpp
        .include_directories
        .iter()
        .chain(&project.properties.cpp.library_directories)
        .chain(&project.properties.cpp.libraries)
        .chain(&project.properties.cpp.definitions)
        .chain(&project.properties.cpp.compiler_options)
        .chain(&project.properties.cpp.linker_options)
        .chain(&project.properties.cpp.runtime_files);
    if values.into_iter().any(|value| {
        value
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | '\0'))
    }) {
        return Err(
            "C++ project property values cannot contain line breaks or null characters.".to_owned(),
        );
    }
    let directory = project.path.join(".fugh");
    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Could not create generated CMake properties directory '{}': {error}",
            directory.display()
        )
    })?;
    let include_file = directory.join("cmake-project-properties.cmake");
    fs::write(
        &include_file,
        render_cpp_cmake_properties(&project.properties),
    )
    .map_err(|error| {
        format!(
            "Could not write generated CMake project properties '{}': {error}",
            include_file.display()
        )
    })?;
    Ok(include_file)
}

fn render_cpp_cmake_properties(properties: &ProjectProperties) -> String {
    let cpp = &properties.cpp;
    let architecture_options = match cpp.platform.trim().to_ascii_lowercase().as_str() {
        "" | "host" => Vec::new(),
        "x64" => vec!["-m64".to_owned()],
        "x86" => vec!["-m32".to_owned()],
        _ => Vec::new(),
    };
    let mut contents = String::from(
        "# Generated by Fugh. Edit this through Project > Properties.\ninclude_guard(GLOBAL)\n",
    );
    append_cmake_variable(
        &mut contents,
        "_fugh_include_directories",
        &cpp.include_directories,
    );
    append_cmake_variable(
        &mut contents,
        "_fugh_library_directories",
        &cpp.library_directories,
    );
    append_cmake_variable(&mut contents, "_fugh_libraries", &cpp.libraries);
    append_cmake_variable(&mut contents, "_fugh_definitions", &cpp.definitions);
    append_cmake_variable(
        &mut contents,
        "_fugh_compiler_options",
        &cpp.compiler_options,
    );
    append_cmake_variable(&mut contents, "_fugh_linker_options", &cpp.linker_options);
    append_cmake_variable(
        &mut contents,
        "_fugh_architecture_options",
        &architecture_options,
    );
    append_cmake_variable(&mut contents, "_fugh_runtime_files", &cpp.runtime_files);
    contents.push_str("set(_fugh_project_root \"${CMAKE_CURRENT_SOURCE_DIR}\")\n");
    if !cpp.standard.trim().is_empty() {
        contents.push_str(&format!(
            "set(_fugh_cxx_standard {})\n",
            cpp.standard.trim()
        ));
    }
    contents.push_str(
        "function(_fugh_apply_project_properties)\n  if(_fugh_architecture_options AND NOT CMAKE_CXX_COMPILER_ID MATCHES \"GNU|Clang\")\n    message(FATAL_ERROR \"The selected x86/x64 C++ target requires a GCC or Clang toolchain.\")\n  endif()\n  set(_fugh_pending \"${_fugh_project_root}\")\n  while(_fugh_pending)\n    list(POP_FRONT _fugh_pending _fugh_directory)\n    get_property(_fugh_targets DIRECTORY \"${_fugh_directory}\" PROPERTY BUILDSYSTEM_TARGETS)\n    foreach(_fugh_target IN LISTS _fugh_targets)\n      get_property(_fugh_imported TARGET \"${_fugh_target}\" PROPERTY IMPORTED)\n      if(_fugh_imported)\n        continue()\n      endif()\n      get_property(_fugh_type TARGET \"${_fugh_target}\" PROPERTY TYPE)\n      if(_fugh_type STREQUAL \"UTILITY\")\n        continue()\n      endif()\n      if(_fugh_type STREQUAL \"INTERFACE_LIBRARY\")\n        set(_fugh_scope INTERFACE)\n      else()\n        set(_fugh_scope PRIVATE)\n      endif()\n      if(NOT \"${_fugh_cxx_standard}\" STREQUAL \"\")\n        if(_fugh_scope STREQUAL \"INTERFACE\")\n          set_property(TARGET \"${_fugh_target}\" PROPERTY INTERFACE_CXX_STANDARD \"${_fugh_cxx_standard}\")\n        else()\n          set_property(TARGET \"${_fugh_target}\" PROPERTY CXX_STANDARD \"${_fugh_cxx_standard}\")\n          set_property(TARGET \"${_fugh_target}\" PROPERTY CXX_STANDARD_REQUIRED ON)\n          set_property(TARGET \"${_fugh_target}\" PROPERTY CXX_EXTENSIONS OFF)\n        endif()\n      endif()\n      if(_fugh_include_directories)\n        target_include_directories(\"${_fugh_target}\" SYSTEM ${_fugh_scope} ${_fugh_include_directories})\n      endif()\n      if(_fugh_library_directories)\n        target_link_directories(\"${_fugh_target}\" ${_fugh_scope} ${_fugh_library_directories})\n      endif()\n      if(_fugh_libraries)\n        target_link_libraries(\"${_fugh_target}\" ${_fugh_scope} ${_fugh_libraries})\n      endif()\n      if(_fugh_definitions)\n        target_compile_definitions(\"${_fugh_target}\" ${_fugh_scope} ${_fugh_definitions})\n      endif()\n      if(_fugh_compiler_options)\n        target_compile_options(\"${_fugh_target}\" ${_fugh_scope} ${_fugh_compiler_options})\n      endif()\n      if(_fugh_architecture_options)\n        target_compile_options(\"${_fugh_target}\" ${_fugh_scope} ${_fugh_architecture_options})\n        if(_fugh_type STREQUAL \"INTERFACE_LIBRARY\" OR _fugh_type STREQUAL \"EXECUTABLE\" OR _fugh_type STREQUAL \"SHARED_LIBRARY\" OR _fugh_type STREQUAL \"MODULE_LIBRARY\")\n          target_link_options(\"${_fugh_target}\" ${_fugh_scope} ${_fugh_architecture_options})\n        endif()\n      endif()\n      if(_fugh_linker_options AND (_fugh_type STREQUAL \"EXECUTABLE\" OR _fugh_type STREQUAL \"SHARED_LIBRARY\" OR _fugh_type STREQUAL \"MODULE_LIBRARY\"))\n        target_link_options(\"${_fugh_target}\" ${_fugh_scope} ${_fugh_linker_options})\n      endif()\n    endforeach()\n    get_property(_fugh_subdirectories DIRECTORY \"${_fugh_directory}\" PROPERTY SUBDIRECTORIES)\n    list(APPEND _fugh_pending ${_fugh_subdirectories})\n  endwhile()\nendfunction()\nif(CMAKE_VERSION VERSION_GREATER_EQUAL \"3.19\")\n  cmake_language(DEFER CALL _fugh_apply_project_properties)\nelse()\n  if(NOT \"${_fugh_cxx_standard}\" STREQUAL \"\")\n    set(CMAKE_CXX_STANDARD \"${_fugh_cxx_standard}\")\n    set(CMAKE_CXX_STANDARD_REQUIRED ON)\n    set(CMAKE_CXX_EXTENSIONS OFF)\n  endif()\n  if(_fugh_include_directories)\n    include_directories(SYSTEM ${_fugh_include_directories})\n  endif()\n  if(_fugh_library_directories)\n    link_directories(${_fugh_library_directories})\n  endif()\n  if(_fugh_libraries)\n    link_libraries(${_fugh_libraries})\n  endif()\n  if(_fugh_definitions)\n    add_compile_definitions(${_fugh_definitions})\n  endif()\n  if(_fugh_compiler_options)\n    add_compile_options(${_fugh_compiler_options})\n  endif()\n  if(_fugh_architecture_options)\n    if(NOT CMAKE_CXX_COMPILER_ID MATCHES \"GNU|Clang\")\n      message(FATAL_ERROR \"The selected x86/x64 C++ target requires a GCC or Clang toolchain.\")\n    endif()\n    add_compile_options(${_fugh_architecture_options})\n    add_link_options(${_fugh_architecture_options})\n  endif()\n  if(_fugh_linker_options)\n    add_link_options(${_fugh_linker_options})\n  endif()\nendif()\n",
    );
    contents.push_str(
        "function(_fugh_copy_runtime_files)\n  if(NOT _fugh_runtime_files)\n    return()\n  endif()\n  set(_fugh_pending \"${_fugh_project_root}\")\n  while(_fugh_pending)\n    list(POP_FRONT _fugh_pending _fugh_directory)\n    get_property(_fugh_targets DIRECTORY \"${_fugh_directory}\" PROPERTY BUILDSYSTEM_TARGETS)\n    foreach(_fugh_target IN LISTS _fugh_targets)\n      get_property(_fugh_type TARGET \"${_fugh_target}\" PROPERTY TYPE)\n      if(_fugh_type STREQUAL \"EXECUTABLE\")\n        foreach(_fugh_runtime_file IN LISTS _fugh_runtime_files)\n          get_filename_component(_fugh_runtime_absolute \"${_fugh_runtime_file}\" ABSOLUTE BASE_DIR \"${_fugh_project_root}\")\n          if(NOT EXISTS \"${_fugh_runtime_absolute}\")\n            message(FATAL_ERROR \"Configured runtime library does not exist: ${_fugh_runtime_absolute}\")\n          endif()\n          add_custom_command(TARGET \"${_fugh_target}\" POST_BUILD COMMAND \"${CMAKE_COMMAND}\" -E copy_if_different \"${_fugh_runtime_absolute}\" \"$<TARGET_FILE_DIR:${_fugh_target}>\" VERBATIM)\n        endforeach()\n        if(UNIX AND NOT APPLE)\n          set_property(TARGET \"${_fugh_target}\" APPEND PROPERTY BUILD_RPATH \"$ORIGIN\")\n        elseif(APPLE)\n          set_property(TARGET \"${_fugh_target}\" APPEND PROPERTY BUILD_RPATH \"@loader_path\")\n        endif()\n      endif()\n    endforeach()\n    get_property(_fugh_subdirectories DIRECTORY \"${_fugh_directory}\" PROPERTY SUBDIRECTORIES)\n    list(APPEND _fugh_pending ${_fugh_subdirectories})\n  endwhile()\nendfunction()\nif(CMAKE_VERSION VERSION_GREATER_EQUAL \"3.19\")\n  cmake_language(DEFER CALL _fugh_copy_runtime_files)\nelseif(_fugh_runtime_files)\n  message(FATAL_ERROR \"Adding runtime .dll/.so files through Project Properties requires CMake 3.19 or newer.\")\nendif()\n",
    );
    contents.replace(
        "      get_property(_fugh_type TARGET \"${_fugh_target}\" PROPERTY TYPE)\n      if(_fugh_type STREQUAL \"INTERFACE_LIBRARY\")",
        "      get_property(_fugh_type TARGET \"${_fugh_target}\" PROPERTY TYPE)\n      if(_fugh_type STREQUAL \"UTILITY\")\n        continue()\n      endif()\n      if(_fugh_type STREQUAL \"INTERFACE_LIBRARY\")",
    )
}

fn append_cmake_variable(contents: &mut String, name: &str, values: &[String]) {
    let values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    contents.push_str(&format!("set({name}\n"));
    for value in values {
        contents.push_str("  ");
        contents.push_str(&cmake_quote(value));
        contents.push('\n');
    }
    contents.push_str(")\n");
}

fn cmake_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace(';', "\\;")
            .replace('$', "\\$")
    )
}

fn project_build_configuration(
    project: &ProjectInfo,
    action: ProcessAction,
) -> Result<String, String> {
    if action == ProcessAction::BuildRelease {
        return Ok("Release".to_owned());
    }
    let selected = match project.language {
        ProjectLanguage::Cpp => &project.properties.cpp.build_configuration,
        ProjectLanguage::CSharp => &project.properties.csharp.build_configuration,
    };
    let selected = selected.trim();
    if selected.is_empty() {
        return Ok("Debug".to_owned());
    }
    if selected
        .chars()
        .any(|character| character.is_control() || matches!(character, '/' | '\\' | ';' | '$'))
    {
        return Err(
            "Build configuration cannot contain path separators, control characters, semicolons, or '$'.".to_owned(),
        );
    }
    Ok(selected.to_owned())
}

fn validate_build_platform(project: &ProjectInfo) -> Result<(), String> {
    match project.language {
        ProjectLanguage::Cpp
            if !matches!(
                project.properties.cpp.platform.trim(),
                "" | "Host" | "x86" | "x64"
            ) =>
        {
            Err("Unsupported C++ target platform. Choose Host, x86, or x64.".to_owned())
        }
        ProjectLanguage::CSharp
            if !matches!(
                project.properties.csharp.platform_target.trim(),
                "" | "AnyCPU" | "AnyCpu" | "x86" | "x64" | "ARM" | "ARM64"
            ) =>
        {
            Err(
                "Unsupported C# target platform. Choose Any CPU, x86, x64, ARM, or ARM64."
                    .to_owned(),
            )
        }
        _ => Ok(()),
    }
}

fn csharp_build_properties(project: &ProjectInfo) -> Vec<String> {
    let settings = &project.properties.csharp;
    [
        ("TargetFramework", &settings.target_framework),
        ("RuntimeIdentifier", &settings.runtime_identifier),
        ("AssemblyName", &settings.assembly_name),
        ("RootNamespace", &settings.root_namespace),
        ("LangVersion", &settings.language_version),
        ("Nullable", &settings.nullable),
        ("ImplicitUsings", &settings.implicit_usings),
        ("TreatWarningsAsErrors", &settings.treat_warnings_as_errors),
        ("PlatformTarget", &settings.platform_target),
    ]
    .into_iter()
    .filter_map(|(key, value)| {
        let value = value.trim();
        (!value.is_empty()).then(|| format!("-p:{key}={value}"))
    })
    .collect()
}

fn csharp_run_properties(project: &ProjectInfo) -> Vec<String> {
    csharp_build_properties(project)
        .into_iter()
        .map(|property| format!("--property:{}", property.trim_start_matches("-p:")))
        .collect()
}

fn csharp_target_framework(project: &ProjectInfo) -> Result<String, String> {
    let override_value = project.properties.csharp.target_framework.trim();
    if !override_value.is_empty() {
        return Ok(override_value
            .split(';')
            .next()
            .unwrap_or(override_value)
            .trim()
            .to_owned());
    }
    let contents = fs::read_to_string(csproj_path(project))
        .map_err(|error| format!("Could not read C# project file: {error}"))?;
    let document = roxmltree::Document::parse(&contents)
        .map_err(|error| format!("Could not parse C# project file: {error}"))?;
    document
        .descendants()
        .find(|node| {
            matches!(
                node.tag_name().name(),
                "TargetFramework" | "TargetFrameworks"
            )
        })
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_owned())
        .ok_or_else(|| "Could not determine the C# target framework.".to_owned())
}

fn csharp_assembly_name(project: &ProjectInfo) -> Result<String, String> {
    let configured = project.properties.csharp.assembly_name.trim();
    if !configured.is_empty() {
        return Ok(configured.to_owned());
    }
    let contents = fs::read_to_string(csproj_path(project))
        .map_err(|error| format!("Could not read C# project file: {error}"))?;
    let document = roxmltree::Document::parse(&contents)
        .map_err(|error| format!("Could not parse C# project file: {error}"))?;
    Ok(document
        .descendants()
        .find(|node| node.tag_name().name() == "AssemblyName")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&project.name)
        .to_owned())
}

fn native_runtime_paths(project: &ProjectInfo) -> Vec<PathBuf> {
    project
        .properties
        .cpp
        .runtime_library_directories
        .iter()
        .chain(&project.properties.cpp.library_directories)
        .map(|path| {
            let path = PathBuf::from(path.trim());
            if path.is_absolute() {
                path
            } else {
                project.path.join(path)
            }
        })
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

fn configure_native_runtime_environment(command: &mut Command, project: &ProjectInfo) {
    let paths = native_runtime_paths(project);
    if paths.is_empty() {
        return;
    }
    #[cfg(unix)]
    {
        let variable = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let mut joined_paths = paths;
        if let Some(existing) = std::env::var_os(variable) {
            joined_paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(value) = std::env::join_paths(joined_paths) {
            command.env(variable, value);
        }
    }
    #[cfg(windows)]
    {
        let mut joined_paths = paths;
        if let Some(existing) = std::env::var_os("PATH") {
            joined_paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(value) = std::env::join_paths(joined_paths) {
            command.env("PATH", value);
        }
    }
}

fn gdb_debug_arguments(breakpoints: &[(PathBuf, usize)]) -> Vec<String> {
    let mut arguments = vec![
        "--batch".to_owned(),
        "--ex".to_owned(),
        "break main".to_owned(),
    ];
    for (path, line) in breakpoints {
        let escaped_path = path
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        arguments.extend([
            "--ex".to_owned(),
            format!("break \"{escaped_path}\":{line}"),
        ]);
    }
    arguments.extend([
        "--ex".to_owned(),
        "run".to_owned(),
        "--ex".to_owned(),
        "bt".to_owned(),
        "--ex".to_owned(),
        "continue".to_owned(),
    ]);
    arguments
}

fn cpp_executable(project: &ProjectInfo) -> Result<PathBuf, String> {
    let stem = &project.name;
    let configuration = project_build_configuration(project, ProcessAction::Build)?;
    let mut candidates = vec![
        project.path.join("build").join(&configuration).join(stem),
        project
            .path
            .join("build")
            .join(&configuration)
            .join(format!("{stem}.exe")),
        project.path.join("build").join(stem),
        project.path.join("build").join(format!("{stem}.exe")),
    ];
    candidates.dedup();
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "Built executable not found. Build the C++ project first.".to_owned())
}

/// Runs the already-built program under the process manager so cancellation,
/// output capture, and console input all belong to the IDE process lifecycle.
fn run_project_interactive(
    project: &ProjectInfo,
    cancel: &AtomicBool,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
) -> Result<(), String> {
    let (program, args) = match project.language {
        ProjectLanguage::Cpp if project.template != ProjectTemplate::ClassLibrary => {
            (cpp_executable(project)?, Vec::<String>::new())
        }
        ProjectLanguage::Cpp => {
            return Err(
                "A C++ class library cannot be run directly; create a console app that links it."
                    .to_owned(),
            );
        }
        ProjectLanguage::CSharp
            if matches!(
                project.template,
                ProjectTemplate::Console | ProjectTemplate::AvaloniaApp
            ) =>
        {
            let configuration = project_build_configuration(project, ProcessAction::Build)?;
            let mut args = vec![
                "run".into(),
                "--no-build".into(),
                "--no-launch-profile".into(),
                "--configuration".into(),
                configuration,
                "--project".into(),
                csproj_path(project).to_string_lossy().into_owned(),
            ];
            args.extend(csharp_run_properties(project));
            (PathBuf::from("dotnet"), args)
        }
        ProjectLanguage::CSharp => return Err("Class libraries cannot be run directly.".to_owned()),
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("Process cancelled.".to_owned());
    }
    let _ = sender.send(ProcessEvent::Output(format!("Starting {}", project.name)));
    let mut command = Command::new(&program);
    command
        .envs(crate::toolchain::toolchain_environment())
        .args(&args)
        .current_dir(&project.path);
    configure_native_runtime_environment(&mut command, project);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    // Linux distributions can provide only a newer shared .NET runtime (for
    // example, .NET 10 for a net9.0 project). Keep normal runtime selection
    // unless this is an Avalonia desktop app, where we allow the next major as
    // a fallback while still preferring the requested major when installed.
    if project.template == ProjectTemplate::AvaloniaApp {
        command.env("DOTNET_ROLL_FORWARD", "Major");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start '{}'. Check that the required tool is installed and available on PATH. ({error})", program.display()))?;
    let stdin = child.stdin.take().expect("stdin was piped");
    let (input_tx, input_rx) = mpsc::channel::<String>();
    let _ = sender.send(ProcessEvent::StdinReady(input_tx));
    repaint.request_repaint();
    let input_thread = thread::Builder::new()
        .name("cairn-process-input".into())
        .spawn(move || {
            let mut stdin = stdin;
            while let Ok(text) = input_rx.recv() {
                if stdin
                    .write_all(text.as_bytes())
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        })
        .map_err(|error| {
            let _ = child.kill();
            format!("Could not start process input writer: {error}")
        })?;
    let stdout_thread = pipe_lines(
        child.stdout.take().expect("stdout was piped"),
        sender.clone(),
        repaint.clone(),
        None,
    )?;
    let stderr_thread = pipe_lines(
        child.stderr.take().expect("stderr was piped"),
        sender.clone(),
        repaint.clone(),
        None,
    )?;
    let status = wait_interactive_or_cancel(&mut child, cancel);
    drop(input_thread);
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    let status = status?;
    let code = status.code().map_or_else(
        || "terminated by signal".to_owned(),
        |code| code.to_string(),
    );
    let _ = sender.send(ProcessEvent::Output(format!(
        "Process exited with code {code}."
    )));
    if status.success() {
        Ok(())
    } else {
        Err(format!("Process exited with code {code}."))
    }
}

fn wait_interactive_or_cancel(
    child: &mut Child,
    cancel: &AtomicBool,
) -> Result<std::process::ExitStatus, String> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            #[cfg(unix)]
            {
                terminate_process_group(child)?;
            }
            #[cfg(not(unix))]
            {
                child
                    .kill()
                    .map_err(|error| format!("Could not cancel process: {error}"))?;
                let _ = child.wait();
            }
            return Err("Process cancelled.".to_owned());
        }
        match child
            .try_wait()
            .map_err(|error| format!("Could not check process status: {error}"))?
        {
            Some(status) => return Ok(status),
            None => thread::sleep(Duration::from_millis(80)),
        }
    }
}

#[cfg(unix)]
fn terminate_process_group(child: &mut Child) -> Result<(), String> {
    let process_group = -(child.id() as i32);
    let term = unsafe { libc::kill(process_group, libc::SIGTERM) };
    if term != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(format!("Could not stop process group: {error}"));
        }
    }
    // Allow the app to close cleanly, then kill any descendants that ignored
    // SIGTERM. Waiting only for dotnet here can leave the actual GUI running.
    thread::sleep(Duration::from_millis(500));
    let _ = unsafe { libc::kill(process_group, libc::SIGKILL) };
    child
        .wait()
        .map(|_| ())
        .map_err(|error| format!("Could not reap stopped process: {error}"))
}

fn mi_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

fn parse_debug_location(record: &str) -> Option<DebugLocation> {
    let frame_start = record.find("frame={").map(|index| index + 7)?;
    let frame_end = balanced_end(record, frame_start - 1, b'{', b'}')?;
    let frame = &record[frame_start..frame_end];
    let file = mi_field(frame, "fullname").or_else(|| mi_field(frame, "file"))?;
    let line = mi_field(frame, "line")?.parse().ok()?;
    Some(DebugLocation {
        file: PathBuf::from(file),
        line,
        function: mi_field(frame, "func"),
    })
}

fn parse_debug_variables(record: &str) -> Vec<DebugVariable> {
    let Some(start) = record.find("variables=[").map(|index| index + 11) else {
        return Vec::new();
    };
    let end = balanced_end(record, start - 1, b'[', b']').unwrap_or(record.len());
    let section = &record[start..end];
    let mut variables = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = section[cursor..].find('{') {
        let open = cursor + relative;
        let Some(close) = balanced_end(section, open, b'{', b'}') else {
            break;
        };
        let item = &section[open + 1..close];
        if let Some(name) = mi_field(item, "name") {
            variables.push(DebugVariable {
                name,
                value: mi_field(item, "value").unwrap_or_else(|| "<unavailable>".to_owned()),
                type_name: mi_field(item, "type").unwrap_or_default(),
            });
        }
        cursor = close + 1;
        if cursor >= section.len() {
            break;
        }
    }
    variables
}

fn parse_debug_call_stack(record: &str) -> Vec<DebugFrame> {
    let Some(start) = record.find("stack=[").map(|index| index + 7) else {
        return Vec::new();
    };
    let end = balanced_end(record, start - 1, b'[', b']').unwrap_or(record.len());
    let section = &record[start..end];
    let mut frames = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = section[cursor..].find("frame={") {
        let open = cursor + relative + 6;
        let Some(close) = balanced_end(section, open, b'{', b'}') else {
            break;
        };
        let item = &section[open + 1..close];
        let parsed_level = mi_field(item, "level").and_then(|value| value.parse().ok());
        frames.push(DebugFrame {
            level: parsed_level.unwrap_or(frames.len()),
            function: mi_field(item, "func"),
            file: mi_field(item, "fullname")
                .or_else(|| mi_field(item, "file"))
                .map(PathBuf::from),
            line: mi_field(item, "line").and_then(|value| value.parse().ok()),
            module: mi_field(item, "from"),
        });
        cursor = close + 1;
        if cursor >= section.len() {
            break;
        }
    }
    frames
}

fn mi_field(record: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = record.find(&needle)? + needle.len();
    let tail = &record[start..];
    if let Some(quoted) = tail.strip_prefix('"') {
        let mut value = String::new();
        let mut chars = quoted.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '"' => return Some(value),
                '\\' => match chars.next()? {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    escaped => value.push(escaped),
                },
                other => value.push(other),
            }
        }
        None
    } else {
        Some(
            tail.split([',', '}', ']'])
                .next()
                .unwrap_or_default()
                .to_owned(),
        )
    }
}

fn balanced_end(text: &str, open: usize, left: u8, right: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, &byte) in bytes.iter().enumerate().skip(open) {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            ch if ch == left => depth += 1,
            ch if ch == right => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn wait_debugger_or_cancel(
    child: &mut Child,
    cancel: &AtomicBool,
    input: &Sender<String>,
    inferior_exited: &AtomicBool,
) -> Result<std::process::ExitStatus, String> {
    loop {
        if inferior_exited.load(Ordering::Relaxed) {
            // The inferior has already exited. Ask MI to close, then force-close
            // the debugger group if the backend leaves its prompt open.
            let _ = input.send("-gdb-exit\n".to_owned());
            let deadline = std::time::Instant::now() + Duration::from_millis(800);
            while std::time::Instant::now() < deadline {
                if let Some(status) = child.try_wait().map_err(|error| {
                    format!("Could not check debugger status after program exit: {error}")
                })? {
                    return Ok(status);
                }
                thread::sleep(Duration::from_millis(50));
            }
            terminate_debugger_process_group(child)?;
            return child
                .wait()
                .map_err(|error| format!("Could not close debugger after program exit: {error}"));
        }
        if cancel.load(Ordering::Relaxed) {
            // Ask the backend to terminate the debuggee first so Stop does not
            // leave the user's program running after the debugger exits.
            let _ = input.send("-exec-abort\n".to_owned());
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                if let Some(status) = child.try_wait().map_err(|error| {
                    format!("Could not check debugger status while stopping: {error}")
                })? {
                    // A debugger can exit after aborting the inferior while a
                    // launched target is still alive. Clean up its process group.
                    terminate_debugger_process_group(child)?;
                    return Ok(status);
                }
                thread::sleep(Duration::from_millis(80));
            }
            terminate_debugger_process_group(child)?;
            let status = child
                .wait()
                .map_err(|error| format!("Could not reap stopped debugger: {error}"))?;
            return Ok(status);
        }

        match child
            .try_wait()
            .map_err(|error| format!("Could not check debugger status: {error}"))?
        {
            Some(status) => return Ok(status),
            None => thread::sleep(Duration::from_millis(80)),
        }
    }
}

fn run_mi_debugger(
    debugger: &Path,
    args: &[String],
    cwd: &Path,
    project: &ProjectInfo,
    breakpoints: &[(PathBuf, usize)],
    cancel: &AtomicBool,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Process cancelled.".to_owned());
    }
    let debugger_name = debugger
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("debugger");
    let _ = sender.send(ProcessEvent::Output(format!(
        "Starting {debugger_name} debugger (MI)"
    )));
    repaint.request_repaint();
    let mut debugger_command = Command::new(debugger);
    debugger_command
        .envs(crate::toolchain::toolchain_environment())
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        debugger_command.process_group(0);
    }
    #[cfg(unix)]
    if debugger_name == "netcoredbg" {
        crate::toolchain::set_library_path_for_binary(&mut debugger_command, debugger);
    }
    configure_native_runtime_environment(&mut debugger_command, project);
    let mut child = debugger_command
        .spawn()
        .map_err(|error| format!("Could not start {}. Install it from Toolchain Setup or ensure it is available on PATH. ({error})", debugger.display()))?;
    let stdin = child.stdin.take().expect("debugger stdin was piped");
    let (input_tx, input_rx) = mpsc::channel::<String>();
    let inferior_exited = Arc::new(AtomicBool::new(false));
    let _ = sender.send(ProcessEvent::StdinReady(input_tx.clone()));
    let _input_thread = thread::Builder::new()
        .name("cairn-debugger-input".into())
        .spawn(move || {
            let mut stdin = stdin;
            while let Ok(command) = input_rx.recv() {
                if stdin
                    .write_all(command.as_bytes())
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        })
        .map_err(|error| {
            let _ = child.kill();
            format!("Could not start debugger input writer: {error}")
        })?;
    let stdout_thread = pipe_lines(
        child.stdout.take().expect("debugger stdout was piped"),
        sender.clone(),
        repaint.clone(),
        Some(inferior_exited.clone()),
    )?;
    let stderr_thread = pipe_lines(
        child.stderr.take().expect("debugger stderr was piped"),
        sender.clone(),
        repaint.clone(),
        None,
    )?;
    let _ = sender.send(ProcessEvent::RuntimePid(child.id()));
    repaint.request_repaint();
    if debugger_name != "netcoredbg" {
        for command in ["-gdb-set pagination off\n", "-gdb-set mi-async on\n"] {
            let _ = input_tx.send(command.to_owned());
        }
    }
    for (path, line) in breakpoints {
        let location = mi_quote(&format!("{}:{line}", path.display()));
        let _ = input_tx.send(format!("-break-insert {location}\n"));
    }
    let _ = input_tx.send("-exec-run\n".to_owned());
    let status = wait_debugger_or_cancel(&mut child, cancel, &input_tx, &inferior_exited);
    drop(input_tx);
    // ProcessState retains a sender until it receives Finished. Joining here
    // would deadlock because the writer waits for every sender to be dropped.
    // Dropping the JoinHandle detaches it; Finished clears the retained sender,
    // which closes the channel and lets the writer exit.
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    let status = status?;
    if cancel.load(Ordering::Relaxed) {
        let _ = sender.send(ProcessEvent::Output("Debug session stopped.".to_owned()));
        repaint.request_repaint();
        return Ok(());
    }
    if inferior_exited.load(Ordering::Relaxed) {
        let _ = sender.send(ProcessEvent::Output("Debug session completed.".to_owned()));
        repaint.request_repaint();
        return Ok(());
    }
    let code = status.code().map_or_else(
        || "terminated by signal".to_owned(),
        |code| code.to_string(),
    );
    let _ = sender.send(ProcessEvent::Output(format!(
        "Debugger process exited with code {code}."
    )));
    if status.success() {
        Ok(())
    } else {
        Err(format!("Debugger process exited with code {code}."))
    }
}

fn terminate_debugger_process_group(child: &mut Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        let process_group = -(child.id() as i32);
        // The debugger and its launched target share this dedicated group.
        // Killing the group prevents a stopped IDE session from orphaning the target.
        let result = unsafe { libc::kill(process_group, libc::SIGKILL) };
        if result == 0 {
            return Ok(());
        }
        if child
            .try_wait()
            .map_err(|error| format!("Could not check debugger status: {error}"))?
            .is_some()
        {
            return Ok(());
        }
        return Err(format!(
            "Could not stop debugger process group: {}",
            std::io::Error::last_os_error()
        ));
    }
    #[cfg(not(unix))]
    {
        child
            .kill()
            .map_err(|error| format!("Could not stop debugger: {error}"))
    }
}

#[cfg(windows)]
fn tool_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn launch_terminal(
    project: &ProjectInfo,
    debug: bool,
    cancel: &AtomicBool,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
    _breakpoints: &[(PathBuf, usize)],
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Process cancelled.".to_owned());
    }

    let (program, args) = match project.language {
        ProjectLanguage::Cpp if !debug => (cpp_executable(project)?, Vec::<String>::new()),
        ProjectLanguage::Cpp => {
            return Err(
                "C++ terminal debugging is not supported through this launch path.".to_owned(),
            );
        }
        ProjectLanguage::CSharp
            if matches!(
                project.template,
                ProjectTemplate::Console | ProjectTemplate::AvaloniaApp
            ) && debug =>
        {
            if !cfg!(target_os = "linux") {
                return Err(
                    "C# debugging through NetCoreDbg is currently supported on Linux only."
                        .to_owned(),
                );
            }
            let debugger = crate::toolchain::ToolchainManager::netcoredbg_executable()
                .unwrap_or_else(|| PathBuf::from("netcoredbg"));
            Command::new(&debugger)
                .arg("--help")
                .output()
                .map_err(|error| {
                    format!(
                        "C# debugging requires NetCoreDbg. Install it from Toolchain Setup and try again. ({error})"
                    )
                })?;
            (debugger, netcoredbg_arguments(&csharp_assembly(project)?))
        }
        ProjectLanguage::CSharp
            if matches!(
                project.template,
                ProjectTemplate::Console | ProjectTemplate::AvaloniaApp
            ) =>
        {
            let configuration = project_build_configuration(project, ProcessAction::Build)?;
            let mut args = vec![
                "run".to_owned(),
                "--no-build".to_owned(),
                "--no-launch-profile".to_owned(),
                "--configuration".to_owned(),
                configuration,
                "--project".to_owned(),
                csproj_path(project).display().to_string(),
            ];
            args.extend(csharp_run_properties(project));
            (PathBuf::from("dotnet"), args)
        }
        ProjectLanguage::CSharp => {
            return Err("Class libraries cannot be run directly.".to_owned());
        }
    };

    let program_text = program.to_string_lossy().into_owned();
    let _ = sender.send(ProcessEvent::Output(format!(
        "Opening a separate terminal for {}",
        project.name
    )));
    repaint.request_repaint();

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let command = windows_console_command(&program_text, &args);
        Command::new("cmd.exe")
            .arg("/K")
            .arg(command)
            .current_dir(&project.path)
            .creation_flags(0x0000_0010)
            .spawn()
            .map_err(|error| {
                format!("Could not open a new Command Prompt for '{program_text}': {error}")
            })?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let configured = crate::settings::current(repaint).general;
        let terminals: Vec<(String, Vec<&str>)> = if configured.terminal.is_empty() {
            vec![
                ("gnome-terminal".to_owned(), vec!["--wait", "--"]),
                // Keep the launcher attached to the terminal lifetime so Run can
                // remain in Stop state and stopping the process can close it.
                ("konsole".to_owned(), vec!["--nofork", "-e"]),
                ("x-terminal-emulator".to_owned(), vec!["-e"]),
                ("xterm".to_owned(), vec!["-e"]),
            ]
        } else {
            let terminal = configured.terminal;
            let prefix = if terminal.contains("gnome-terminal") {
                vec!["--wait", "--"]
            } else {
                vec!["-e"]
            };
            vec![(terminal, prefix)]
        };
        let mut errors = Vec::new();
        for (terminal, prefix) in terminals {
            let cancel_path = std::env::temp_dir().join(format!(
                "cairn-run-cancel-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let pid_path = cancel_path.with_extension("pid");
            let invocation = linux_terminal_invocation_with_shell(
                &program,
                &args,
                &cancel_path,
                &pid_path,
                &configured.shell,
            );
            let mut terminal_command = Command::new(&terminal);
            terminal_command
                .envs(crate::toolchain::toolchain_environment())
                .args(prefix)
                .args(invocation)
                .current_dir(&project.path);
            configure_native_runtime_environment(&mut terminal_command, project);
            let result = terminal_command.spawn();
            match result {
                Ok(mut child) => {
                    let _ = sender.send(ProcessEvent::Output(format!(
                        "Running {} in {terminal}. Close the terminal or use Stop to end it.",
                        project.name
                    )));
                    repaint.request_repaint();
                    let status = wait_terminal_or_cancel(
                        &mut child,
                        cancel,
                        &cancel_path,
                        &pid_path,
                        sender,
                        repaint,
                    )?;
                    let _ = std::fs::remove_file(&cancel_path);
                    let _ = std::fs::remove_file(&pid_path);
                    let code = status.code().map_or_else(
                        || "terminated by signal".to_owned(),
                        |code| code.to_string(),
                    );
                    let _ = sender.send(ProcessEvent::Output(format!(
                        "Terminal process exited with code {code}."
                    )));
                    repaint.request_repaint();
                    if status.success() {
                        return Ok(());
                    }
                    return Err(format!("Terminal process exited with code {code}."));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => errors.push(format!("{terminal}: {error}")),
            }
        }
        let detail = if errors.is_empty() {
            "No supported terminal emulator was found (tried gnome-terminal, konsole, x-terminal-emulator, and xterm).".to_owned()
        } else {
            errors.join("; ")
        };
        return Err(format!(
            "Could not open a terminal for '{program_text}': {detail}"
        ));
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (program, args, cancel, sender, repaint);
        Err(
            "Opening projects in a separate terminal is not supported on this platform yet."
                .to_owned(),
        )
    }
}

fn csharp_assembly(project: &ProjectInfo) -> Result<PathBuf, String> {
    let framework = csharp_target_framework(project)?;
    let configuration = project_build_configuration(project, ProcessAction::Build)?;
    let mut directories = vec![
        project
            .path
            .join("bin")
            .join(&configuration)
            .join(&framework),
    ];
    let runtime_identifier = project.properties.csharp.runtime_identifier.trim();
    if !runtime_identifier.is_empty() {
        directories.push(directories[0].join(runtime_identifier));
    }
    directories.reverse();
    let assembly_name = csharp_assembly_name(project)?;
    directories
        .into_iter()
        .map(|directory| directory.join(format!("{assembly_name}.dll")))
        .find(|assembly| assembly.is_file())
        .ok_or_else(|| {
            format!(
                "Built C# assembly '{}' was not found under bin/{configuration}/{framework}. Build the project first.",
                assembly_name
            )
        })
}

fn netcoredbg_arguments(assembly: &Path) -> Vec<String> {
    vec![
        "--interpreter=cli".to_owned(),
        "--".to_owned(),
        "dotnet".to_owned(),
        assembly.display().to_string(),
    ]
}

#[cfg(any(target_os = "linux", test))]
fn linux_terminal_invocation(
    program: &Path,
    args: &[String],
    cancel_path: &Path,
    pid_path: &Path,
) -> Vec<std::ffi::OsString> {
    linux_terminal_invocation_with_shell(program, args, cancel_path, pid_path, "bash")
}

#[cfg(any(target_os = "linux", test))]
fn linux_terminal_invocation_with_shell(
    program: &Path,
    args: &[String],
    cancel_path: &Path,
    pid_path: &Path,
    shell: &str,
) -> Vec<std::ffi::OsString> {
    let mut invocation = vec![
        if shell.trim().is_empty() { "bash" } else { shell.trim() }.into(),
        "-lc".into(),
        concat!(
            "cancel_file=$1; pid_file=$2; shift 2; ",
            "setsid \"$@\" & app_pid=$!; printf '%s' \"$app_pid\" > \"$pid_file\"; ",
            "(while kill -0 \"$app_pid\" 2>/dev/null; do ",
            "if [ -f \"$cancel_file\" ]; then ",
            "kill -TERM -- -\"$app_pid\" 2>/dev/null || kill -TERM \"$app_pid\" 2>/dev/null; ",
            "sleep 1; kill -KILL -- -\"$app_pid\" 2>/dev/null || true; exit; fi; sleep .1; done) & watcher=$!; ",
            "wait \"$app_pid\"; status=$?; kill \"$watcher\" 2>/dev/null || true; ",
            "if [ -f \"$cancel_file\" ]; then rm -f \"$cancel_file\"; exit 130; fi; ",
            "printf '\\n[Fugh] Process exited with code %s.\\n' \"$status\"; ",
            "if [ -t 0 ]; then read -r -p 'Press Enter to close this window...' _; fi; ",
            "exit \"$status\""
        )
        .into(),
        "cairn-run".into(),
        cancel_path.as_os_str().to_owned(),
        pid_path.as_os_str().to_owned(),
        program.as_os_str().to_owned(),
    ];
    invocation.extend(args.iter().map(std::ffi::OsString::from));
    invocation
}

#[cfg(target_os = "linux")]
fn wait_terminal_or_cancel(
    child: &mut Child,
    cancel: &AtomicBool,
    cancel_path: &Path,
    pid_path: &Path,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
) -> Result<std::process::ExitStatus, String> {
    let mut cancel_requested_at = None;
    let mut reported_pid = None;
    loop {
        if reported_pid.is_none()
            && let Ok(pid) = std::fs::read_to_string(pid_path)
            && let Ok(pid) = pid.trim().parse::<u32>()
        {
            reported_pid = Some(pid);
            let _ = sender.send(ProcessEvent::RuntimePid(pid));
            repaint.request_repaint();
        }
        if cancel.load(Ordering::Relaxed) && cancel_requested_at.is_none() {
            let _ = std::fs::write(cancel_path, b"stop");
            cancel_requested_at = Some(Instant::now());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Could not check terminal status: {error}"))?
        {
            if cancel_requested_at.is_some() {
                return Err("Process cancelled.".to_owned());
            }
            return Ok(status);
        }
        if cancel_requested_at.is_some_and(|started| started.elapsed() >= Duration::from_secs(3)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Process cancelled; terminal was closed.".to_owned());
        }
        thread::sleep(Duration::from_millis(80));
    }
}

#[cfg(any(windows, test))]
fn windows_console_command(program: &str, args: &[String]) -> String {
    let encoded_executable = STANDARD.encode(program.as_bytes());
    let encoded_arguments = args
        .iter()
        .map(|argument| {
            format!(
                "[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{}'))",
                STANDARD.encode(argument.as_bytes())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "$exe=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded_executable}'));$args=@({encoded_arguments});& $exe @args;exit $LASTEXITCODE"
    );
    let encoded_script = STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    format!("powershell.exe -NoProfile -EncodedCommand {encoded_script}")
}

fn run_checked(
    program: &str,
    args: impl IntoIterator<Item = String>,
    cwd: &Path,
    cancel: &AtomicBool,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
) -> Result<(), String> {
    run_checked_for_project(program, args, cwd, None, cancel, sender, repaint)
}

fn run_checked_for_project(
    program: &str,
    args: impl IntoIterator<Item = String>,
    cwd: &Path,
    project: Option<&ProjectInfo>,
    cancel: &AtomicBool,
    sender: &Sender<ProcessEvent>,
    repaint: &egui::Context,
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Process cancelled.".to_owned());
    }

    let args: Vec<String> = args.into_iter().collect();
    let _ = sender.send(ProcessEvent::Output(format!(
        "> {} {}",
        program,
        args.join(" ")
    )));
    repaint.request_repaint();
    let mut command = Command::new(program);
    command
        .envs(crate::toolchain::toolchain_environment())
        .args(&args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(project) = project {
        configure_native_runtime_environment(&mut command, project);
    }
    // Keep build tools (cmake, dotnet, compilers) in their own process group
    // so Stop can terminate the entire toolchain tree on Linux and macOS.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| {
            format!(
                "Could not start '{program}'. Check that the required tool is installed and available on PATH. ({error})"
            )
        })?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let stdout_thread = match pipe_lines(stdout, sender.clone(), repaint.clone(), None) {
        Ok(thread) => thread,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stderr_thread = match pipe_lines(stderr, sender.clone(), repaint.clone(), None) {
        Ok(thread) => thread,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            return Err(error);
        }
    };
    let status = wait_or_cancel(&mut child, cancel);
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    let status = status?;

    if status.success() {
        let _ = sender.send(ProcessEvent::Output(format!(
            "'{program}' completed with exit code {}.",
            status.code().unwrap_or(0)
        )));
        repaint.request_repaint();
        Ok(())
    } else {
        Err(format!(
            "'{program}' exited with status {}.",
            status.code().map_or_else(
                || "terminated by signal".to_owned(),
                |code| code.to_string()
            )
        ))
    }
}

fn pipe_lines<R: std::io::Read + Send + 'static>(
    stream: R,
    sender: Sender<ProcessEvent>,
    repaint: egui::Context,
    inferior_exited: Option<Arc<AtomicBool>>,
) -> Result<thread::JoinHandle<()>, String> {
    thread::Builder::new()
        .name("cairn-process-output".to_owned())
        .spawn(move || {
            for line in BufReader::new(stream).lines() {
                match line {
                    Ok(line) => {
                        if line.starts_with("*stopped")
                            && line.contains("reason=\"exited")
                            && let Some(exited) = &inferior_exited
                        {
                            exited.store(true, Ordering::Relaxed);
                        }
                        let _ = sender.send(ProcessEvent::Output(line));
                        repaint.request_repaint();
                    }
                    Err(error) => {
                        let _ = sender.send(ProcessEvent::Output(format!(
                            "Could not read process output: {error}"
                        )));
                        repaint.request_repaint();
                        break;
                    }
                }
            }
        })
        .map_err(|error| format!("Could not start process output reader: {error}"))
}

fn wait_or_cancel(
    child: &mut Child,
    cancel: &AtomicBool,
) -> Result<std::process::ExitStatus, String> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            #[cfg(unix)]
            terminate_process_group(child)?;
            #[cfg(not(unix))]
            {
                child
                    .kill()
                    .map_err(|error| format!("Could not cancel process: {error}"))?;
                let _ = child.wait();
            }
            return Err("Process cancelled.".to_owned());
        }

        match child
            .try_wait()
            .map_err(|error| format!("Could not check process status: {error}"))?
        {
            Some(status) => return Ok(status),
            None => thread::sleep(Duration::from_millis(80)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::linux_terminal_invocation;
    use super::windows_console_command;
    use super::{
        ProcessAction, ProcessEvent, ProcessManager, csharp_build_properties,
        csharp_run_properties, execute_action, gdb_debug_arguments, netcoredbg_arguments,
        render_cpp_cmake_properties, run_project_interactive,
    };
    use crate::project::{
        ProjectInfo, ProjectLanguage, ProjectProperties, ProjectTemplate, Workspace,
    };
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        sync::{atomic::AtomicBool, mpsc},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn available(program: &str) -> bool {
        Command::new(program)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn temp_dir(suffix: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cairn-process-test-{}-{suffix}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary directory");
        path
    }

    #[test]
    fn cpp_properties_generate_target_scoped_cmake_settings() {
        let mut properties = ProjectProperties::default();
        properties.cpp.standard = "20".to_owned();
        properties
            .cpp
            .include_directories
            .push("third party/include".to_owned());
        properties
            .cpp
            .library_directories
            .push("vendor/lib".to_owned());
        properties.cpp.libraries.push("vendorlib".to_owned());
        properties.cpp.definitions.push("FEATURE=1".to_owned());
        properties.cpp.compiler_options.push("-Wall".to_owned());
        properties
            .cpp
            .linker_options
            .push("-Wl,--as-needed".to_owned());
        properties
            .cpp
            .runtime_files
            .push("runtime/custom.so".to_owned());
        let generated = render_cpp_cmake_properties(&properties);
        assert!(generated.contains("set(_fugh_cxx_standard 20)"));
        assert!(generated.contains("target_include_directories("));
        assert!(generated.contains("target_link_directories("));
        assert!(generated.contains("target_link_libraries("));
        assert!(generated.contains("target_compile_definitions("));
        assert!(generated.contains("target_compile_options("));
        assert!(generated.contains("target_link_options("));
        assert!(generated.contains("copy_if_different"));
        assert!(generated.contains("BUILD_RPATH \"$ORIGIN\""));
        assert!(generated.contains("third party/include"));
    }

    #[test]
    fn csharp_properties_are_passed_to_msbuild_for_build_and_run() {
        let project = ProjectInfo {
            name: "Demo".to_owned(),
            path: PathBuf::new(),
            language: ProjectLanguage::CSharp,
            template: ProjectTemplate::Console,
            entry_file: PathBuf::from("Program.cs"),
            project_file: Some(PathBuf::from("Demo.csproj")),
            properties: crate::project::ProjectProperties {
                csharp: crate::project::CSharpProjectProperties {
                    target_framework: "net10.0".to_owned(),
                    runtime_identifier: "linux-x64".to_owned(),
                    assembly_name: "Demo.Cli".to_owned(),
                    root_namespace: "Demo".to_owned(),
                    language_version: "preview".to_owned(),
                    nullable: "enable".to_owned(),
                    implicit_usings: "disable".to_owned(),
                    treat_warnings_as_errors: "true".to_owned(),
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let build = csharp_build_properties(&project);
        assert!(build.contains(&"-p:TargetFramework=net10.0".to_owned()));
        assert!(build.contains(&"-p:RuntimeIdentifier=linux-x64".to_owned()));
        assert!(build.contains(&"-p:AssemblyName=Demo.Cli".to_owned()));
        assert!(build.contains(&"-p:Nullable=enable".to_owned()));
        let run = csharp_run_properties(&project);
        assert!(run.contains(&"--property:TargetFramework=net10.0".to_owned()));
        assert!(run.contains(&"--property:RuntimeIdentifier=linux-x64".to_owned()));
    }

    #[test]
    fn windows_console_invocation_does_not_interpolate_project_paths_into_cmd() {
        let malicious_looking_path = r"C:\Projects\Demo & echo bad %PATH%";
        let command = windows_console_command(malicious_looking_path, &["hello".to_owned()]);

        assert!(command.starts_with("powershell.exe -NoProfile -EncodedCommand "));
        assert!(!command.contains(malicious_looking_path));
        assert!(!command.contains("& echo bad"));
    }

    #[test]
    fn csharp_debugger_launches_netcoredbg_with_dotnet_and_the_assembly() {
        let assembly = PathBuf::from("/tmp/My App/bin/Debug/net10.0/My App.dll");
        assert_eq!(
            netcoredbg_arguments(&assembly),
            [
                "--interpreter=cli",
                "--",
                "dotnet",
                "/tmp/My App/bin/Debug/net10.0/My App.dll"
            ]
        );
    }

    #[test]
    fn gdb_debug_arguments_include_editor_breakpoints() {
        let args = gdb_debug_arguments(&[(PathBuf::from("/work/main.cpp"), 17)]);
        assert!(
            args.windows(2)
                .any(|pair| { pair[0] == "--ex" && pair[1] == "break \"/work/main.cpp\":17" })
        );
        assert!(args.iter().any(|argument| argument == "break main"));
        assert!(args.iter().any(|argument| argument == "run"));
    }

    #[test]
    fn linux_terminal_keeps_output_visible_and_passes_arguments_separately() {
        let executable = PathBuf::from("/tmp/project with spaces/app");
        let args = vec!["value with spaces".to_owned(), "& echo unsafe".to_owned()];
        let cancel_path = PathBuf::from("/tmp/cancel marker");
        let pid_path = PathBuf::from("/tmp/runtime pid");
        let invocation = linux_terminal_invocation(&executable, &args, &cancel_path, &pid_path);

        assert_eq!(invocation[0], "bash");
        assert!(
            invocation[2]
                .to_string_lossy()
                .contains("Press Enter to close")
        );
        assert_eq!(invocation[4], cancel_path.as_os_str());
        assert_eq!(invocation[5], pid_path.as_os_str());
        assert_eq!(invocation[6], executable.as_os_str());
        assert_eq!(invocation[7], "value with spaces");
        assert_eq!(invocation[8], "& echo unsafe");
    }

    fn collect_output(receiver: mpsc::Receiver<ProcessEvent>) -> String {
        receiver
            .try_iter()
            .filter_map(|event| match event {
                ProcessEvent::Output(line) => Some(line),
                ProcessEvent::Finished(_) => None,
                ProcessEvent::StdinReady(_) => None,
                ProcessEvent::RuntimePid(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn cpp_class_libraries_reject_run_and_debug_before_starting_a_process() {
        let manager = ProcessManager::new(eframe::egui::Context::default());
        let project = ProjectInfo {
            name: "UtilityLibrary".to_owned(),
            path: PathBuf::new(),
            language: ProjectLanguage::Cpp,
            template: ProjectTemplate::ClassLibrary,
            entry_file: PathBuf::from("src/library.cpp"),
            project_file: None,
            properties: Default::default(),
        };
        for action in [ProcessAction::Run, ProcessAction::Debug] {
            let error = manager
                .handle()
                .start(project.clone(), action)
                .expect_err("a class library has no executable to run or debug");
            assert!(error.contains("class library"));
        }
    }

    #[test]
    fn generated_cpp_project_builds_and_runs_when_toolchain_is_installed() {
        if !available("cmake")
            || !(available("g++") || available("clang++"))
            || (cfg!(windows) && !available("mingw32-make") && !available("ninja"))
        {
            eprintln!("Skipping C++ integration test: CMake/compiler/build tool unavailable.");
            return;
        }

        let parent = temp_dir("cpp");
        let mut workspace = Workspace::default();
        let mut project = workspace
            .create_project(
                &parent,
                "Starter",
                ProjectLanguage::Cpp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("C++ project template created");
        let configured_include = project.path.join("third party headers");
        fs::create_dir_all(&configured_include).expect("custom include directory created");
        fs::write(
            configured_include.join("property header.hpp"),
            "inline const char* property_message() { return \"Hello from Starter!\"; }\n",
        )
        .expect("custom include header written");
        project
            .properties
            .cpp
            .include_directories
            .push("third party headers".to_owned());
        project
            .properties
            .cpp
            .definitions
            .push("FUGH_PROJECT_PROPERTY_TEST=1".to_owned());
        project
            .properties
            .cpp
            .compiler_options
            .push("-Wall".to_owned());
        let runtime_file = project.path.join("vendor/runtime/property-runtime.so");
        fs::create_dir_all(runtime_file.parent().expect("runtime file parent"))
            .expect("runtime library directory created");
        fs::write(&runtime_file, b"runtime copy test").expect("runtime library written");
        project
            .properties
            .cpp
            .runtime_files
            .push("vendor/runtime/property-runtime.so".to_owned());
        fs::write(
            project.path.join("src/main.cpp"),
            "#include <iostream>\n#include \"property header.hpp\"\n#ifndef FUGH_PROJECT_PROPERTY_TEST\n#error Project compile definitions were not applied\n#endif\nint main() { std::cout << property_message() << '\\n'; return 0; }\n",
        )
        .expect("test source written");
        let (sender, receiver) = mpsc::channel();
        let context = eframe::egui::Context::default();
        let cancel = AtomicBool::new(false);

        if let Err(error) = execute_action(
            &project,
            ProcessAction::Build,
            &cancel,
            &sender,
            &context,
            &[],
        ) {
            panic!(
                "generated C++ project build failed: {error}\n{}",
                receiver
                    .try_iter()
                    .filter_map(|event| match event {
                        ProcessEvent::Output(line) => Some(line),
                        ProcessEvent::Finished(_) => None,
                        ProcessEvent::StdinReady(_) => None,
                        ProcessEvent::RuntimePid(_) => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        run_project_interactive(&project, &cancel, &sender, &context)
            .expect("generated C++ project runs");

        let output = collect_output(receiver);
        assert!(output.contains("Hello from Starter!"), "{output}");
        let executable = cpp_executable(&project).expect("built program found");
        assert!(
            executable
                .parent()
                .expect("executable directory")
                .join("property-runtime.so")
                .is_file(),
            "configured runtime library copied beside the executable"
        );

        fs::remove_dir_all(parent).expect("temporary project removed");
    }

    #[test]
    fn generated_cpp_class_library_builds_when_toolchain_is_installed() {
        if !available("cmake")
            || !(available("g++") || available("clang++"))
            || (cfg!(windows) && !available("mingw32-make") && !available("ninja"))
        {
            eprintln!(
                "Skipping C++ library integration test: CMake/compiler/build tool unavailable."
            );
            return;
        }

        let parent = temp_dir("cpp-library");
        let mut workspace = Workspace::default();
        let project = workspace
            .create_project(
                &parent,
                "UtilityLibrary",
                ProjectLanguage::Cpp,
                ProjectTemplate::ClassLibrary,
                "net10.0",
            )
            .expect("C++ class library template created");
        let (sender, receiver) = mpsc::channel();
        let context = eframe::egui::Context::default();
        let cancel = AtomicBool::new(false);
        if let Err(error) = execute_action(
            &project,
            ProcessAction::Build,
            &cancel,
            &sender,
            &context,
            &[],
        ) {
            panic!(
                "generated C++ library build failed: {error}\n{}",
                collect_output(receiver)
            );
        }
        fs::remove_dir_all(parent).expect("temporary library removed");
    }

    #[test]
    fn generated_csharp_project_builds_and_runs_when_sdk_is_installed() {
        let sdk = Command::new("dotnet")
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
        let Some(sdk) = sdk else {
            eprintln!("Skipping C# integration test: .NET SDK unavailable.");
            return;
        };
        let major = sdk
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .unwrap_or_default();
        if major < 9 {
            eprintln!("Skipping C# integration test: local SDK cannot target net9.0.");
            return;
        }

        let parent = temp_dir("csharp");
        let mut workspace = Workspace::default();
        let project = workspace
            .create_project(
                &parent,
                "Starter",
                ProjectLanguage::CSharp,
                ProjectTemplate::Console,
                "net9.0",
            )
            .expect("C# project template created");
        let (sender, receiver) = mpsc::channel();
        let context = eframe::egui::Context::default();
        let cancel = AtomicBool::new(false);

        if let Err(error) = execute_action(
            &project,
            ProcessAction::Build,
            &cancel,
            &sender,
            &context,
            &[],
        ) {
            panic!(
                "generated C# project build failed: {error}\n{}",
                receiver
                    .try_iter()
                    .filter_map(|event| match event {
                        ProcessEvent::Output(line) => Some(line),
                        ProcessEvent::Finished(_) => None,
                        ProcessEvent::StdinReady(_) => None,
                        ProcessEvent::RuntimePid(_) => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        run_project_interactive(&project, &cancel, &sender, &context)
            .expect("generated C# project runs");

        let output = collect_output(receiver);
        assert!(output.contains("Hello from Starter!"), "{output}");

        let mut library_workspace = Workspace::default();
        let library = library_workspace
            .create_project(
                &parent,
                "ReusableLibrary",
                ProjectLanguage::CSharp,
                ProjectTemplate::ClassLibrary,
                "net9.0",
            )
            .expect("C# class-library template created");
        let (library_sender, library_receiver) = mpsc::channel();
        execute_action(
            &library,
            ProcessAction::Build,
            &cancel,
            &library_sender,
            &context,
            &[],
        )
        .expect("generated C# class library builds");
        let library_output = collect_output(library_receiver);
        assert!(
            library
                .path
                .join("bin/Debug/net9.0/ReusableLibrary.dll")
                .is_file(),
            "Class-library assembly was not produced. {library_output}"
        );
        fs::remove_dir_all(parent).expect("temporary project removed");
    }
}
