pub(crate) mod designer;
pub(crate) mod layout;
mod settings_ui;
mod ui;
mod windows;

use eframe::{CreationContext, egui};
use std::time::Instant;

use crate::{
    config::{AppSettings, SETTINGS_KEY},
    process::ProcessManager,
    project::ProjectInfo,
    theme,
};

pub struct IdeApp {
    pub(super) settings: AppSettings,
    pub(super) status_message: String,
    windows: windows::WindowState,
    process: ProcessManager,
    crash_message: Option<String>,
    last_autosave: Instant,
    last_recovery_write: Instant,
    pub(super) settings_return_page: crate::config::WorkspacePage,
    pending_close: Option<PendingCloseAction>,
    allow_close: bool,
}

#[derive(Clone)]
enum PendingCloseAction {
    Home,
    NewProjectWizard,
    OpenProject(std::path::PathBuf),
    Quit,
}

impl IdeApp {
    pub fn new(creation_context: &CreationContext<'_>) -> Self {
        let mut settings: AppSettings = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, SETTINGS_KEY))
            .unwrap_or_default();
        // Keep user preferences durable even if the window is closed before
        // eframe's normal shutdown save callback runs.
        let (loaded_user_settings, settings_load_error) = match load_user_settings_file() {
            Ok(settings) => (settings, None),
            Err(error) => (None, Some(error)),
        };
        if let Some(user_settings) = loaded_user_settings {
            settings.user = user_settings;
        }
        let mut preferences_load_error = settings_load_error;
        let formatting_file_missing = match load_formatting_settings_file() {
            Ok(Some(formatting)) => {
                settings.user.formatting = formatting;
                false
            }
            Ok(None) => true,
            Err(error) => {
                if preferences_load_error.is_none() {
                    preferences_load_error = Some(error);
                }
                false
            }
        };
        let settings_need_migration =
            settings.user.schema_version < crate::settings::SETTINGS_SCHEMA_VERSION;
        settings.user.migrate();
        // Always start at the project chooser. The restored workspace remains
        // available for recovery, but opening the IDE must not skip the choice.
        settings.page = crate::config::WorkspacePage::Welcome;
        settings.user.sanitize();
        let settings_migration_error = settings_need_migration
            .then(|| save_user_settings_file(&settings.user).err())
            .flatten();
        let formatting_file_error = formatting_file_missing
            .then(|| save_formatting_settings_file(&settings.user.formatting).err())
            .flatten();
        if !settings.user.general.restore_open_files {
            settings.workspace.documents = vec![crate::project::Document::scratch()];
            settings.workspace.selected_document = 0;
        }
        if !settings.user.general.restore_window_layout {
            settings.editor_layout = crate::app::layout::EditorLayout::default();
        }
        let recovered = settings
            .user
            .general
            .crash_recovery
            .then(read_recovery_snapshot)
            .flatten();
        if let Some(workspace) = recovered.clone() {
            settings.workspace = workspace;
        }

        crate::settings::publish(&creation_context.egui_ctx, &settings.user);
        theme::apply(
            &creation_context.egui_ctx,
            &settings.user.appearance,
            &settings.user.editor,
        );

        let windows =
            windows::WindowState::new(settings.workspace.clone(), settings.editor_layout.clone());
        Self {
            settings,
            status_message: if recovered.is_some() {
                "Recovered unsaved edits from the previous session".to_owned()
            } else if let Some(error) = preferences_load_error {
                error
            } else if let Some(error) = formatting_file_error {
                format!("Could not save formatting preferences: {error}")
            } else if let Some(error) = settings_migration_error {
                format!("Could not save migrated settings: {error}")
            } else {
                "Welcome to Fugh".to_owned()
            },
            windows,
            process: ProcessManager::new(creation_context.egui_ctx.clone()),
            crash_message: None,
            last_autosave: Instant::now(),
            last_recovery_write: Instant::now(),
            settings_return_page: crate::config::WorkspacePage::Welcome,
            pending_close: None,
            allow_close: false,
        }
    }
}

impl eframe::App for IdeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let native_close_requested = ctx.input(|input| input.viewport().close_requested());
        if native_close_requested {
            if self.allow_close {
                self.allow_close = false;
            } else if self.settings.user.general.confirm_close_unsaved
                && self.has_unsaved_documents()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_close = Some(PendingCloseAction::Quit);
            }
        }
        let user_settings_before_frame = self.settings.user.clone();
        self.process.poll();
        self.settings.user.sanitize();
        self.autosave_if_due();
        self.recovery_if_due();
        crate::settings::publish(&ctx, &self.settings.user);
        theme::apply(
            &ctx,
            &self.settings.user.appearance,
            &self.settings.user.editor,
        );
        if let Some(message) = self.crash_message.clone() {
            self.draw_crash_screen(ui, &message);
            return;
        }
        // A bug in one frame must never take the whole IDE (and unsaved work) down.
        let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.draw_interface(ui);
            self.draw_pending_close_dialog(ui);
            self.windows.show(&ctx);
        }));
        let mut requested_font_size = None;
        ctx.data_mut(|data| {
            let id = egui::Id::unique("cairn.editor-font-zoom-request");
            requested_font_size = data.get_temp::<f32>(id);
            data.remove::<f32>(id);
        });
        if let Some(font_size) = requested_font_size {
            self.settings.user.editor.font_size = font_size.clamp(
                crate::settings::MIN_FONT_SIZE,
                crate::settings::MAX_FONT_SIZE,
            );
            ctx.request_repaint();
        }
        if self.settings.user != user_settings_before_frame
            && let Err(error) = save_user_settings_file(&self.settings.user)
        {
            self.status_message = format!("Could not save settings: {error}");
        }
        if let Err(payload) = drawn {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown error".to_owned());
            self.crash_message = Some(detail);
            ctx.request_repaint();
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.workspace = self.windows.workspace_snapshot();
        if self.settings.user.general.restore_window_layout {
            self.settings.editor_layout = self.windows.editor_layout_snapshot();
        }
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
        if self.settings.user.general.crash_recovery {
            write_recovery_snapshot(&self.settings.workspace);
        } else {
            remove_recovery_snapshot();
        }
    }
}

impl IdeApp {
    fn has_unsaved_documents(&self) -> bool {
        self.windows
            .workspace_snapshot()
            .documents
            .iter()
            .any(|document| document.dirty)
    }

    fn save_unsaved_documents(&mut self) -> Result<bool, String> {
        let line_ending = self.settings.user.general.line_ending;
        let formatting = self.settings.user.formatting.clone();
        let (scratch_files, project_directory) = {
            let workspace = self.windows.workspace_lock();
            (
                workspace
                    .documents
                    .iter()
                    .enumerate()
                    .filter(|(_, document)| document.dirty && document.path.is_none())
                    .map(|(index, document)| (index, document.title.clone()))
                    .collect::<Vec<_>>(),
                workspace
                    .current
                    .as_ref()
                    .map(|project| project.path.clone()),
            )
        };

        {
            let mut workspace = self.windows.workspace_lock();
            windows::save_all(&mut workspace, &formatting, line_ending);
            if workspace
                .documents
                .iter()
                .any(|document| document.dirty && document.path.is_some())
            {
                return Err(workspace.message.clone());
            }
        }

        for (index, title) in scratch_files {
            let mut dialog = rfd::FileDialog::new().set_file_name(&title);
            if let Some(directory) = &project_directory {
                dialog = dialog.set_directory(directory);
            }
            let Some(path) = dialog.save_file() else {
                return Ok(false);
            };
            self.windows
                .workspace_lock()
                .save_document_as_with_line_ending(index, &path, line_ending)?;
        }
        Ok(true)
    }

    fn request_close_action(&mut self, action: PendingCloseAction, ctx: &egui::Context) {
        if self.settings.user.general.confirm_close_unsaved && self.has_unsaved_documents() {
            self.pending_close = Some(action);
        } else {
            let discard = self.has_unsaved_documents();
            self.finish_close_action(action, discard, Some(ctx));
        }
    }

    fn draw_pending_close_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(action) = self.pending_close.clone() else {
            return;
        };
        let unsaved_names = self
            .windows
            .workspace_snapshot()
            .documents
            .iter()
            .filter(|document| document.dirty)
            .map(|document| document.title.clone())
            .collect::<Vec<_>>();
        let (save_label, discard_label) = match &action {
            PendingCloseAction::Home => ("Save and Continue", "Don't Save"),
            PendingCloseAction::NewProjectWizard => ("Save and Continue", "Don't Save"),
            PendingCloseAction::OpenProject(_) => ("Save Files and Open Project", "Don't Save"),
            PendingCloseAction::Quit => ("Save and Quit", "Don't Save"),
        };
        let prompt = match &action {
            PendingCloseAction::Home => {
                "Save changes to your open files before returning to the start page?"
            }
            PendingCloseAction::NewProjectWizard => {
                "Save changes to your open files before creating a project?"
            }
            PendingCloseAction::OpenProject(_) => {
                "Save changes to your open files before opening this project?"
            }
            PendingCloseAction::Quit => "Save changes before closing Fugh?",
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("cairn.pending-project-close")).show(ui.ctx(), |ui| {
            ui.set_min_width(390.0);
            ui.heading("Unsaved files");
            ui.label(prompt);
            if !unsaved_names.is_empty() {
                ui.label(
                    egui::RichText::new(unsaved_names.join(", "))
                        .small()
                        .color(theme::muted()),
                );
            }
            if self.status_message.starts_with("Could not") {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.status_message);
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(save_label).clicked() {
                    choice = Some(Some(true));
                }
                if ui.button(discard_label).clicked() {
                    choice = Some(Some(false));
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(None);
                }
            });
        });
        if let Some(choice) = choice {
            let Some(save) = choice else {
                self.pending_close = None;
                return;
            };
            if save {
                match self.save_unsaved_documents() {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        self.status_message = error;
                        return;
                    }
                }
                if self.has_unsaved_documents() {
                    self.status_message = self.windows.workspace_snapshot().message;
                    return;
                }
            }
            self.pending_close = None;
            self.finish_close_action(action, !save, Some(ui.ctx()));
        }
    }

    fn finish_close_action(
        &mut self,
        action: PendingCloseAction,
        discard: bool,
        ctx: Option<&egui::Context>,
    ) {
        match action {
            PendingCloseAction::Home => {
                if discard || !self.has_unsaved_documents() {
                    self.windows.close_project();
                    self.settings.page = crate::config::WorkspacePage::Welcome;
                }
            }
            PendingCloseAction::NewProjectWizard => {
                if discard || !self.has_unsaved_documents() {
                    self.windows.close_project();
                    self.open_project_wizard_now();
                }
            }
            PendingCloseAction::OpenProject(path) => {
                if discard || !self.has_unsaved_documents() {
                    self.open_project_now(&path);
                }
            }
            PendingCloseAction::Quit => {
                if discard {
                    self.windows.close_project();
                    remove_recovery_snapshot();
                }
                self.allow_close = true;
                if let Some(ctx) = ctx {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

fn user_settings_file_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(std::path::PathBuf::from);

    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".config"))
        });

    base.map(|base| base.join("Fugh").join("settings.json"))
}

fn formatting_settings_file_path() -> Option<std::path::PathBuf> {
    user_settings_file_path().map(|path| path.with_file_name("formatting.json"))
}

fn load_user_settings_file() -> Result<Option<crate::settings::UserSettings>, String> {
    let Some(path) = user_settings_file_path() else {
        return Ok(None);
    };
    let backup = path.with_extension("json.bak");
    let mut found_file = false;
    let mut errors = Vec::new();
    for candidate in [&path, &backup] {
        let contents = match std::fs::read(candidate) {
            Ok(contents) => {
                found_file = true;
                contents
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                found_file = true;
                errors.push(format!("{}: {error}", candidate.display()));
                continue;
            }
        };
        match serde_json::from_slice(&contents) {
            Ok(settings) => return Ok(Some(settings)),
            Err(error) => errors.push(format!("{}: {error}", candidate.display())),
        }
    }
    if found_file {
        Err(format!(
            "Could not load saved settings. Tried {} and {}. {}",
            path.display(),
            backup.display(),
            errors.join("; ")
        ))
    } else {
        Ok(None)
    }
}

fn load_formatting_settings_file() -> Result<Option<crate::settings::FormattingSettings>, String> {
    let Some(path) = formatting_settings_file_path() else {
        return Ok(None);
    };
    let backup = path.with_extension("json.bak");
    let mut found_file = false;
    let mut errors = Vec::new();
    for candidate in [&path, &backup] {
        let contents = match std::fs::read(candidate) {
            Ok(contents) => {
                found_file = true;
                contents
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                found_file = true;
                errors.push(format!("{}: {error}", candidate.display()));
                continue;
            }
        };
        match serde_json::from_slice(&contents) {
            Ok(settings) => return Ok(Some(settings)),
            Err(error) => errors.push(format!("{}: {error}", candidate.display())),
        }
    }
    if found_file {
        Err(format!(
            "Could not load formatting preferences. Tried {} and {}. {}",
            path.display(),
            backup.display(),
            errors.join("; ")
        ))
    } else {
        Ok(None)
    }
}

fn save_user_settings_file(settings: &crate::settings::UserSettings) -> Result<(), String> {
    let path = user_settings_file_path().ok_or_else(|| {
        "the operating system did not provide a user configuration directory".to_owned()
    })?;
    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("could not serialize preferences: {error}"))?;
    write_preferences_file(&path, &contents)
}

fn save_formatting_settings_file(
    settings: &crate::settings::FormattingSettings,
) -> Result<(), String> {
    let path = formatting_settings_file_path().ok_or_else(|| {
        "the operating system did not provide a user configuration directory".to_owned()
    })?;
    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("could not serialize formatting preferences: {error}"))?;
    write_preferences_file(&path, &contents)
}

fn write_preferences_file(path: &std::path::Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "the settings file has no parent directory".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, contents)
        .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
    // Windows cannot replace a file with rename. Move the old file out of the
    // way first, and remove stale backups so a previous interrupted save does
    // not permanently block all later preference updates.
    let backup = path.with_extension("json.bak");
    let had_current = path.exists();
    if had_current && backup.exists() {
        std::fs::remove_file(&backup)
            .map_err(|error| format!("could not remove stale preferences backup: {error}"))?;
    }
    if had_current {
        std::fs::rename(&path, &backup)
            .map_err(|error| format!("could not preserve existing preferences: {error}"))?;
    }
    match std::fs::rename(&temporary, &path) {
        Ok(()) => {
            if backup.exists() {
                let _ = std::fs::remove_file(backup);
            }
            Ok(())
        }
        Err(error) => {
            if had_current {
                let _ = std::fs::rename(&backup, &path);
            }
            Err(format!("could not save {}: {error}", path.display()))
        }
    }
}

impl IdeApp {
    fn draw_interface(&mut self, root_ui: &mut egui::Ui) {
        use crate::config::WorkspacePage;

        // One window only: every screen is drawn inside the main window.
        if self.windows.wizard_active() {
            if self.windows.draw_wizard(root_ui) {
                self.settings.page = WorkspacePage::Workspace;
                self.status_message = "Project created".to_owned();
            }
            return;
        }
        match self.settings.page {
            WorkspacePage::Welcome => ui::draw_start(self, root_ui),
            WorkspacePage::Workspace => {
                let process = self.process.handle();
                match self.windows.draw_editor(root_ui, process) {
                    Some(windows::EditorCmd::Home) => {
                        self.request_close_action(PendingCloseAction::Home, root_ui.ctx())
                    }
                    Some(windows::EditorCmd::NewProject) => self.open_project_wizard(),
                    Some(windows::EditorCmd::Settings) => {
                        self.open_settings();
                    }
                    Some(windows::EditorCmd::OpenProject(path)) => {
                        self.open_project(&path);
                    }
                    Some(windows::EditorCmd::Quit) => {
                        self.request_close_action(PendingCloseAction::Quit, root_ui.ctx())
                    }
                    None => {}
                }
            }
            WorkspacePage::Toolchains | WorkspacePage::Settings => {
                ui::draw_simple_page(self, root_ui)
            }
        }
    }

    pub(super) fn open_settings(&mut self) {
        self.settings_return_page = self.settings.page;
        self.settings.page = crate::config::WorkspacePage::Settings;
    }

    pub(super) fn open_toolchain_window(&self) {
        self.windows.open_toolchains();
    }

    pub(super) fn open_project_wizard(&mut self) {
        if self.settings.user.general.confirm_close_unsaved && self.has_unsaved_documents() {
            self.pending_close = Some(PendingCloseAction::NewProjectWizard);
            return;
        }
        if self.has_unsaved_documents() {
            self.windows.close_project();
        }
        self.open_project_wizard_now();
    }

    fn open_project_wizard_now(&self) {
        let location = self.settings.user.general.default_project_location.clone();
        if !location.is_empty() {
            self.windows.set_project_location(&location);
        }
        self.windows.open_project_wizard();
    }

    pub(super) fn open_project(&mut self, path: &std::path::Path) -> bool {
        // Crash recovery can restore the previous project behind the Welcome
        // screen. Selecting that same project is a return to its editor, not a
        // project switch, so recovered edits should not trigger a save prompt.
        if let Some(current) = self.windows.workspace_snapshot().current {
            let requested_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let current_path =
                std::fs::canonicalize(&current.path).unwrap_or_else(|_| current.path.clone());
            if requested_path == current_path {
                self.settings.page = crate::config::WorkspacePage::Workspace;
                self.status_message = "Project reopened".to_owned();
                return true;
            }
        }
        if self.settings.user.general.confirm_close_unsaved && self.has_unsaved_documents() {
            self.pending_close = Some(PendingCloseAction::OpenProject(path.to_path_buf()));
            return true;
        }
        self.open_project_now(path)
    }

    fn open_project_now(&mut self, path: &std::path::Path) -> bool {
        match self.windows.open_project_path(path) {
            Ok(()) => {
                self.status_message = "Project opened".to_owned();
                self.settings.page = crate::config::WorkspacePage::Workspace;
                true
            }
            Err(error) => {
                self.status_message = error;
                false
            }
        }
    }

    pub(super) fn current_project(&self) -> Option<ProjectInfo> {
        self.windows.workspace_snapshot().current
    }

    pub(super) fn toolchain_manager(&self) -> crate::toolchain::ToolchainManager {
        self.windows.toolchain_manager()
    }

    pub(super) fn recent_projects(&self) -> Vec<ProjectInfo> {
        self.windows.workspace_snapshot().recent
    }

    fn autosave_if_due(&mut self) {
        let general = self.settings.user.general.clone();
        if !general.autosave
            || self.last_autosave.elapsed().as_secs()
                < u64::from(general.autosave_interval_minutes) * 60
        {
            return;
        }
        self.last_autosave = Instant::now();
        let mut workspace = self.windows.workspace_lock();
        windows::save_all(
            &mut workspace,
            &self.settings.user.formatting,
            general.line_ending,
        );
    }

    fn recovery_if_due(&mut self) {
        if !self.settings.user.general.crash_recovery {
            remove_recovery_snapshot();
            return;
        }
        if self.last_recovery_write.elapsed().as_secs() < 3 {
            return;
        }
        self.last_recovery_write = Instant::now();
        let workspace = self.windows.workspace_snapshot();
        write_recovery_snapshot(&workspace);
    }
}

fn recovery_snapshot_path() -> std::path::PathBuf {
    std::env::temp_dir().join("cairn-unsaved-recovery.json")
}

fn remove_recovery_snapshot() {
    let _ = std::fs::remove_file(recovery_snapshot_path());
}

fn read_recovery_snapshot() -> Option<crate::project::Workspace> {
    let path = recovery_snapshot_path();
    let workspace = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<crate::project::Workspace>(&bytes).ok());
    if workspace
        .as_ref()
        .is_some_and(|workspace| workspace.documents.iter().any(|document| document.dirty))
    {
        let _ = std::fs::remove_file(path);
        workspace
    } else {
        None
    }
}

fn write_recovery_snapshot(workspace: &crate::project::Workspace) {
    let path = recovery_snapshot_path();
    if !workspace.documents.iter().any(|document| document.dirty) {
        let _ = std::fs::remove_file(path);
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(workspace) {
        let _ = std::fs::write(path, bytes);
    }
}

impl IdeApp {
    fn draw_crash_screen(&mut self, ui: &mut egui::Ui, message: &str) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(40.0);
            ui.heading("Something went wrong");
            ui.add_space(8.0);
            ui.label("Fugh recovered from an internal error. Your open files were not closed.");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(message)
                    .monospace()
                    .color(theme::muted()),
            );
            ui.label(format!(
                "Details were written to {}",
                crate::crash_log_path().display()
            ));
            ui.add_space(14.0);
            if ui.button("Return to the editor").clicked() {
                self.crash_message = None;
            }
            if ui.button("Go to the start screen").clicked() {
                self.crash_message = None;
                self.settings.page = crate::config::WorkspacePage::Welcome;
            }
        });
    }
}
