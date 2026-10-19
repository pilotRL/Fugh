//! User preferences: data model, validation, and key bindings.
//!
//! Everything here is plain data plus pure logic so it can be unit-tested
//! without a window. `UserSettings::sanitize` must be called on every value
//! loaded from disk; the UI only ever produces values that already pass it.

use eframe::egui;
use serde::{Deserialize, Serialize};

pub const MIN_UI_SCALE: f32 = 0.75;
pub const MAX_UI_SCALE: f32 = 2.0;
pub const MIN_FONT_SIZE: f32 = 9.0;
pub const MAX_FONT_SIZE: f32 = 32.0;
pub const SETTINGS_SCHEMA_VERSION: u32 = 2;

fn legacy_settings_schema() -> u32 {
    0
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileEncoding {
    #[default]
    Utf8,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdeLanguage {
    #[default]
    English,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    CairnDark,
    Graphite,
    Light,
    Dracula,
    Nord,
    SolarizedDark,
    Monokai,
    System,
}

impl ThemeChoice {
    pub const ALL: [Self; 8] = [
        Self::CairnDark,
        Self::Graphite,
        Self::Light,
        Self::Dracula,
        Self::Nord,
        Self::SolarizedDark,
        Self::Monokai,
        Self::System,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::CairnDark => "Fugh Dark",
            Self::Graphite => "Graphite",
            Self::Light => "Fugh Light",
            Self::Dracula => "Dracula",
            Self::Nord => "Nord",
            Self::SolarizedDark => "Solarized Dark",
            Self::Monokai => "Monokai",
            Self::System => "System",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccentChoice {
    #[default]
    Violet,
    Blue,
    Teal,
    Amber,
    Rose,
}

impl AccentChoice {
    pub const ALL: [Self; 5] = [
        Self::Violet,
        Self::Blue,
        Self::Teal,
        Self::Amber,
        Self::Rose,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Violet => "Violet",
            Self::Blue => "Blue",
            Self::Teal => "Teal",
            Self::Amber => "Amber",
            Self::Rose => "Rose",
        }
    }

    /// Base accent colour as RGB.
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            Self::Violet => [113, 96, 232],
            Self::Blue => [64, 132, 232],
            Self::Teal => [36, 160, 150],
            Self::Amber => [200, 140, 30],
            Self::Rose => [208, 88, 130],
        }
    }

    /// Lighter variant used for hover states and highlights.
    pub const fn hover_rgb(self) -> [u8; 3] {
        match self {
            Self::Violet => [138, 124, 240],
            Self::Blue => [100, 160, 245],
            Self::Teal => [70, 190, 180],
            Self::Amber => [228, 170, 60],
            Self::Rose => [228, 120, 160],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub language: IdeLanguage,
    /// Folder offered by the New Project wizard. Empty means "use the default".
    pub default_project_location: String,
    pub reopen_last_project: bool,
    pub autosave: bool,
    pub autosave_interval_minutes: u8,
    pub show_welcome_screen: bool,
    pub show_recent_projects: bool,
    pub recent_projects_count: u8,
    pub restore_open_files: bool,
    pub restore_window_layout: bool,
    pub crash_recovery: bool,
    pub file_encoding: FileEncoding,
    pub line_ending: LineEnding,
    pub terminal: String,
    pub shell: String,
    pub external_browser: String,
    pub file_manager: String,
    pub notifications: bool,
    pub confirm_close_unsaved: bool,
    pub confirm_delete: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: IdeLanguage::English,
            default_project_location: String::new(),
            reopen_last_project: false,
            autosave: false,
            autosave_interval_minutes: 5,
            show_welcome_screen: true,
            show_recent_projects: true,
            recent_projects_count: 12,
            restore_open_files: true,
            restore_window_layout: true,
            crash_recovery: true,
            file_encoding: FileEncoding::Utf8,
            line_ending: LineEnding::Lf,
            terminal: String::new(),
            shell: String::new(),
            external_browser: String::new(),
            file_manager: String::new(),
            notifications: true,
            confirm_close_unsaved: true,
            confirm_delete: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub theme: ThemeChoice,
    pub accent: AccentChoice,
    pub ui_scale: f32,
    pub ui_font_size: f32,
    pub ui_font_family: UiFontFamily,
    pub density: InterfaceDensity,
    pub custom_colors: Option<CustomThemeColors>,
    pub toolbar_visible: bool,
    pub status_bar_visible: bool,
    pub tabs_rounded: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiFontFamily {
    #[default]
    Proportional,
    Monospace,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterfaceDensity {
    Compact,
    #[default]
    Comfortable,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorFontFamily {
    #[default]
    Monospace,
    Proportional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomThemeColors {
    pub accent: u32,
    pub background: u32,
    pub panel: u32,
    pub card: u32,
    pub hover: u32,
    pub border: u32,
    pub editor_background: u32,
    pub foreground: u32,
    pub muted: u32,
    pub selection: u32,
    pub cursor: u32,
}

impl Default for CustomThemeColors {
    fn default() -> Self {
        Self {
            accent: 0x7160e8,
            background: 0x1f1f1f,
            panel: 0x252526,
            card: 0x2d2d30,
            hover: 0x3e3e42,
            border: 0x3f3f46,
            editor_background: 0x1e1e1e,
            foreground: 0xe6e6e6,
            muted: 0x9d9da3,
            selection: 0x7160e8,
            cursor: 0xffffff,
        }
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::default(),
            accent: AccentChoice::default(),
            ui_scale: 1.0,
            ui_font_size: 14.0,
            ui_font_family: UiFontFamily::Proportional,
            density: InterfaceDensity::Comfortable,
            custom_colors: None,
            toolbar_visible: true,
            status_bar_visible: true,
            tabs_rounded: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    pub font_size: f32,
    pub font_family: EditorFontFamily,
    pub line_height: f32,
    pub word_wrap: bool,
    pub tab_width: u8,
    pub use_tabs: bool,
    pub auto_indent: bool,
    pub auto_pairs: bool,
    pub line_numbers: bool,
    pub show_whitespace_spaces: bool,
    pub show_whitespace_tabs: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CppFormatStyle {
    #[default]
    LLVM,
    Google,
    Microsoft,
    Chromium,
    Mozilla,
    WebKit,
    GNU,
}

impl CppFormatStyle {
    pub const ALL: [Self; 7] = [
        Self::LLVM,
        Self::Google,
        Self::Microsoft,
        Self::Chromium,
        Self::Mozilla,
        Self::WebKit,
        Self::GNU,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::LLVM => "LLVM",
            Self::Google => "Google",
            Self::Microsoft => "Microsoft",
            Self::Chromium => "Chromium",
            Self::Mozilla => "Mozilla",
            Self::WebKit => "WebKit",
            Self::GNU => "GNU",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CppBraceStyle {
    #[default]
    Attach,
    Allman,
    Linux,
    Stroustrup,
}

impl CppBraceStyle {
    pub const ALL: [Self; 4] = [Self::Attach, Self::Allman, Self::Linux, Self::Stroustrup];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Attach => "Same line",
            Self::Allman => "Allman",
            Self::Linux => "Linux",
            Self::Stroustrup => "Stroustrup",
        }
    }

    pub const fn clang_value(self) -> &'static str {
        match self {
            Self::Attach => "Attach",
            Self::Allman => "Allman",
            Self::Linux => "Linux",
            Self::Stroustrup => "Stroustrup",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CSharpBraceStyle {
    #[default]
    SameLine,
    Allman,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormattingSettings {
    pub cpp_style: CppFormatStyle,
    pub cpp_braces: CppBraceStyle,
    pub csharp_braces: CSharpBraceStyle,
    pub indent_width: u8,
    pub use_tabs: bool,
    pub line_length: u16,
    pub format_on_save: bool,
    pub respect_project_config: bool,
}

impl Default for FormattingSettings {
    fn default() -> Self {
        Self {
            cpp_style: CppFormatStyle::LLVM,
            cpp_braces: CppBraceStyle::Attach,
            csharp_braces: CSharpBraceStyle::SameLine,
            indent_width: 4,
            use_tabs: false,
            line_length: 100,
            format_on_save: true,
            respect_project_config: false,
        }
    }
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_size: 14.5,
            font_family: EditorFontFamily::Monospace,
            line_height: 1.0,
            word_wrap: true,
            tab_width: 4,
            use_tabs: false,
            auto_indent: true,
            auto_pairs: true,
            line_numbers: true,
            show_whitespace_spaces: false,
            show_whitespace_tabs: false,
        }
    }
}

/// Commands that can be bound to a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Save,
    CloseTab,
    ToggleSidePanel,
    ToggleOutputPanel,
    FindInFiles,
    GoToDeclaration,
    FormatDocument,
    DebugContinue,
    DebugStop,
    DebugRestart,
    DebugStepOver,
    DebugStepInto,
    DebugStepOut,
    OpenSettings,
    OpenProject,
    OpenFile,
    SaveAll,
    NextTab,
    PreviousTab,
    ReopenClosedTab,
    CloseAllTabs,
    ToggleZenMode,
    FocusExplorer,
    FocusSourceControl,
    FocusProblems,
    FocusOutput,
    FocusDebugger,
    FocusDebugConsole,
    FocusBreakpoints,
    BuildProject,
    RunProject,
    ToggleBreakpoint,
    RemoveAllBreakpoints,
    AddWatch,
    QuickOpenFile,
    CloseWindow,
}

impl Action {
    pub const ALL: [Self; 36] = [
        Self::Save,
        Self::CloseTab,
        Self::ToggleSidePanel,
        Self::ToggleOutputPanel,
        Self::FindInFiles,
        Self::GoToDeclaration,
        Self::FormatDocument,
        Self::DebugContinue,
        Self::DebugStop,
        Self::DebugRestart,
        Self::DebugStepOver,
        Self::DebugStepInto,
        Self::DebugStepOut,
        Self::OpenSettings,
        Self::OpenProject,
        Self::OpenFile,
        Self::SaveAll,
        Self::NextTab,
        Self::PreviousTab,
        Self::ReopenClosedTab,
        Self::CloseAllTabs,
        Self::ToggleZenMode,
        Self::FocusExplorer,
        Self::FocusSourceControl,
        Self::FocusProblems,
        Self::FocusOutput,
        Self::FocusDebugger,
        Self::FocusDebugConsole,
        Self::FocusBreakpoints,
        Self::BuildProject,
        Self::RunProject,
        Self::ToggleBreakpoint,
        Self::RemoveAllBreakpoints,
        Self::AddWatch,
        Self::QuickOpenFile,
        Self::CloseWindow,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Save => "Save file",
            Self::CloseTab => "Close tab",
            Self::ToggleSidePanel => "Toggle side panel",
            Self::ToggleOutputPanel => "Toggle output panel",
            Self::FindInFiles => "Find in files",
            Self::GoToDeclaration => "Go to indexed declaration",
            Self::FormatDocument => "Format document",
            Self::DebugContinue => "Start / continue debugging",
            Self::DebugStop => "Stop debugging",
            Self::DebugRestart => "Restart debugging / run without debugging",
            Self::DebugStepOver => "Debug: step over",
            Self::DebugStepInto => "Debug: step into",
            Self::DebugStepOut => "Debug: step out",
            Self::OpenSettings => "Open Settings",
            Self::OpenProject => "Open Project Folder",
            Self::OpenFile => "Open File in Project",
            Self::SaveAll => "Save All Files",
            Self::NextTab => "Next Editor Tab",
            Self::PreviousTab => "Previous Editor Tab",
            Self::ReopenClosedTab => "Reopen Closed Tab",
            Self::CloseAllTabs => "Close All Editor Tabs",
            Self::ToggleZenMode => "Toggle Zen Mode",
            Self::FocusExplorer => "Focus Explorer",
            Self::FocusSourceControl => "Focus Source Control",
            Self::FocusProblems => "Open Problems",
            Self::FocusOutput => "Open Build Output",
            Self::FocusDebugger => "Open Debug View",
            Self::FocusDebugConsole => "Open Debug Console",
            Self::FocusBreakpoints => "Open Breakpoints",
            Self::BuildProject => "Build Project",
            Self::RunProject => "Run Without Debugging",
            Self::ToggleBreakpoint => "Toggle Breakpoint",
            Self::RemoveAllBreakpoints => "Remove All Breakpoints",
            Self::AddWatch => "Open Watch Expressions",
            Self::QuickOpenFile => "Quick Open File",
            Self::CloseWindow => "Close IDE Window / Quit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub action: Action,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Name understood by `egui::Key::from_name`, e.g. "S" or "F5".
    pub key: String,
}

impl KeyBinding {
    fn new(action: Action, ctrl: bool, shift: bool, alt: bool, key: &str) -> Self {
        Self {
            action,
            ctrl,
            shift,
            alt,
            key: key.to_owned(),
        }
    }

    pub fn egui_key(&self) -> Option<egui::Key> {
        egui::Key::from_name(&self.key)
    }

    pub fn modifiers(&self) -> egui::Modifiers {
        egui::Modifiers {
            ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
            ..egui::Modifiers::NONE
        }
    }

    /// Human-readable form such as `Ctrl+Shift+S`.
    pub fn display(&self) -> String {
        let mut text = String::new();
        if self.ctrl {
            text.push_str("Ctrl+");
        }
        if self.alt {
            text.push_str("Alt+");
        }
        if self.shift {
            text.push_str("Shift+");
        }
        text.push_str(&self.key);
        text
    }

    fn same_chord(&self, other: &Self) -> bool {
        self.ctrl == other.ctrl
            && self.shift == other.shift
            && self.alt == other.alt
            && self.key.eq_ignore_ascii_case(&other.key)
    }
}

fn is_function_key(key: &str) -> bool {
    matches!(
        key.to_ascii_uppercase().as_str(),
        "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keymap {
    pub bindings: Vec<KeyBinding>,
    #[serde(default)]
    pub aliases: Vec<KeyBinding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: Action::ALL.map(default_binding).to_vec(),
            aliases: vec![
                KeyBinding::new(Action::FocusExplorer, false, false, true, "1"),
                KeyBinding::new(Action::FindInFiles, false, false, true, "2"),
                KeyBinding::new(Action::FocusSourceControl, false, false, true, "3"),
                KeyBinding::new(Action::FocusProblems, false, false, true, "4"),
                KeyBinding::new(Action::FocusOutput, false, false, true, "5"),
                KeyBinding::new(Action::NextTab, true, false, false, "Tab"),
                KeyBinding::new(Action::PreviousTab, true, true, false, "Tab"),
                KeyBinding::new(Action::BuildProject, true, false, false, "B"),
                KeyBinding::new(Action::RunProject, true, true, false, "R"),
                KeyBinding::new(Action::CloseWindow, true, false, false, "Q"),
            ],
        }
    }
}

fn default_binding(action: Action) -> KeyBinding {
    match action {
        Action::Save => KeyBinding::new(action, true, false, false, "S"),
        Action::CloseTab => KeyBinding::new(action, true, false, false, "W"),
        Action::ToggleSidePanel => KeyBinding::new(action, true, true, false, "J"),
        Action::ToggleOutputPanel => KeyBinding::new(action, true, false, false, "J"),
        Action::FindInFiles => KeyBinding::new(action, true, true, false, "F"),
        Action::GoToDeclaration => KeyBinding::new(action, false, false, false, "F12"),
        Action::FormatDocument => KeyBinding::new(action, true, true, true, "F"),
        Action::DebugContinue => KeyBinding::new(action, false, false, false, "F5"),
        Action::DebugStop => KeyBinding::new(action, false, true, false, "F5"),
        Action::DebugRestart => KeyBinding::new(action, true, true, false, "F5"),
        Action::DebugStepOver => KeyBinding::new(action, false, false, false, "F10"),
        Action::DebugStepInto => KeyBinding::new(action, false, false, false, "F11"),
        Action::DebugStepOut => KeyBinding::new(action, false, true, false, "F11"),
        Action::OpenSettings => KeyBinding::new(action, true, false, false, ","),
        Action::OpenProject => KeyBinding::new(action, true, true, false, "O"),
        Action::OpenFile => KeyBinding::new(action, true, false, false, "O"),
        Action::SaveAll => KeyBinding::new(action, true, false, true, "S"),
        Action::NextTab => KeyBinding::new(action, true, false, false, "PageDown"),
        Action::PreviousTab => KeyBinding::new(action, true, false, false, "PageUp"),
        Action::ReopenClosedTab => KeyBinding::new(action, true, true, false, "T"),
        Action::CloseAllTabs => KeyBinding::new(action, true, true, true, "W"),
        Action::ToggleZenMode => KeyBinding::new(action, true, true, false, "F11"),
        Action::FocusExplorer => KeyBinding::new(action, true, true, false, "E"),
        Action::FocusSourceControl => KeyBinding::new(action, true, true, false, "G"),
        Action::FocusProblems => KeyBinding::new(action, true, true, false, "M"),
        Action::FocusOutput => KeyBinding::new(action, true, true, false, "U"),
        Action::FocusDebugger => KeyBinding::new(action, true, true, false, "D"),
        Action::FocusDebugConsole => KeyBinding::new(action, true, true, false, "Y"),
        Action::FocusBreakpoints => KeyBinding::new(action, true, true, true, "B"),
        Action::BuildProject => KeyBinding::new(action, true, true, false, "B"),
        Action::RunProject => KeyBinding::new(action, true, false, false, "F5"),
        Action::ToggleBreakpoint => KeyBinding::new(action, false, false, false, "F9"),
        Action::RemoveAllBreakpoints => KeyBinding::new(action, true, true, false, "F9"),
        Action::AddWatch => KeyBinding::new(action, true, false, true, "W"),
        Action::QuickOpenFile => KeyBinding::new(action, true, false, false, "P"),
        Action::CloseWindow => KeyBinding::new(action, true, true, false, "W"),
    }
}

impl Keymap {
    #[cfg(test)]
    pub fn binding(&self, action: Action) -> Option<&KeyBinding> {
        self.bindings
            .iter()
            .find(|binding| binding.action == action)
    }

    pub fn display_bindings(&self, action: Action) -> String {
        self.bindings_for(action)
            .map(KeyBinding::display)
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub fn bindings_for(&self, action: Action) -> impl Iterator<Item = &KeyBinding> {
        self.bindings
            .iter()
            .chain(self.aliases.iter())
            .filter(move |binding| binding.action == action)
    }

    /// The action (other than `action`) that already uses this chord, if any.
    pub fn conflict_for(&self, candidate: &KeyBinding) -> Option<Action> {
        self.bindings
            .iter()
            .chain(self.aliases.iter())
            .find(|binding| binding.action != candidate.action && binding.same_chord(candidate))
            .map(|binding| binding.action)
    }

    /// Rebinds an action. Refuses unknown keys, bare keys that would break
    /// typing, and chords already used by another action, and says why.
    pub fn rebind(&mut self, candidate: KeyBinding) -> Result<(), String> {
        if candidate.egui_key().is_none() {
            return Err(format!("'{}' is not a key Fugh can bind.", candidate.key));
        }
        if !candidate.ctrl && !candidate.alt && !is_function_key(&candidate.key) {
            return Err(
                "Shortcuts need Ctrl or Alt so they do not interfere with typing code.".to_owned(),
            );
        }
        if let Some(other) = self.conflict_for(&candidate) {
            return Err(format!(
                "{} is already used by \"{}\". Change that shortcut first.",
                candidate.display(),
                other.label()
            ));
        }
        match self
            .bindings
            .iter_mut()
            .find(|binding| binding.action == candidate.action)
        {
            Some(slot) => *slot = candidate,
            None => self.bindings.push(candidate),
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Repairs a keymap loaded from disk: one valid, non-conflicting binding
    /// per action; anything else falls back to the default for that action.
    fn sanitize(&mut self) {
        let loaded = std::mem::take(&mut self.bindings);
        let loaded_aliases = std::mem::take(&mut self.aliases);
        let mut repaired: Vec<KeyBinding> = Vec::new();
        for action in Action::ALL {
            let wanted = loaded
                .iter()
                .find(|binding| binding.action == action)
                .filter(|binding| {
                    binding.egui_key().is_some()
                        && (binding.ctrl || binding.alt || is_function_key(&binding.key))
                        && !repaired.iter().any(|done| done.same_chord(binding))
                })
                .cloned();
            let mut chosen = wanted.unwrap_or_else(|| default_binding(action));
            // A default can itself collide with a user binding made earlier.
            if repaired.iter().any(|done| done.same_chord(&chosen)) {
                chosen = default_binding(action);
            }
            repaired.push(chosen);
        }
        self.bindings = repaired;
        let mut aliases = Vec::new();
        for binding in loaded_aliases {
            if Action::ALL.contains(&binding.action)
                && binding.egui_key().is_some()
                && (binding.ctrl || binding.alt || is_function_key(&binding.key))
                && !self
                    .bindings
                    .iter()
                    .any(|primary| primary.same_chord(&binding))
                && !aliases
                    .iter()
                    .any(|done: &KeyBinding| done.same_chord(&binding))
            {
                aliases.push(binding);
            }
        }
        self.aliases = aliases;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    #[serde(default = "legacy_settings_schema")]
    pub schema_version: u32,
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub editor: EditorSettings,
    pub formatting: FormattingSettings,
    pub keymap: Keymap,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            general: GeneralSettings::default(),
            appearance: AppearanceSettings::default(),
            editor: EditorSettings::default(),
            formatting: FormattingSettings::default(),
            keymap: Keymap::default(),
        }
    }
}

impl UserSettings {
    /// Upgrades older serialized settings before validation. Missing fields in
    /// legacy settings receive the defaults from each settings section.
    pub fn migrate(&mut self) {
        if self.schema_version < SETTINGS_SCHEMA_VERSION {
            // Make custom formatting settings visible in normal use without
            // requiring users to discover a separate Format Document action.
            self.formatting.format_on_save = true;
            self.schema_version = SETTINGS_SCHEMA_VERSION;
        }
    }

    /// Clamps and repairs every value so nothing downstream sees garbage.
    pub fn sanitize(&mut self) {
        self.migrate();
        let appearance = &mut self.appearance;
        appearance.ui_scale = clamp_finite(appearance.ui_scale, MIN_UI_SCALE, MAX_UI_SCALE, 1.0);
        appearance.ui_font_size =
            clamp_finite(appearance.ui_font_size, MIN_FONT_SIZE, MAX_FONT_SIZE, 14.0);
        self.editor.font_size =
            clamp_finite(self.editor.font_size, MIN_FONT_SIZE, MAX_FONT_SIZE, 14.5);
        self.editor.line_height = clamp_finite(self.editor.line_height, 0.0, 12.0, 1.0);
        self.editor.tab_width = self.editor.tab_width.clamp(1, 8);
        self.formatting.indent_width = self.formatting.indent_width.clamp(1, 8);
        self.formatting.line_length = self.formatting.line_length.clamp(40, 240);
        self.editor.tab_width = self.formatting.indent_width;
        self.editor.use_tabs = self.formatting.use_tabs;
        self.general.autosave_interval_minutes =
            self.general.autosave_interval_minutes.clamp(1, 60);
        self.general.recent_projects_count = self.general.recent_projects_count.clamp(1, 30);
        if let Some(colors) = &mut self.appearance.custom_colors {
            for color in [
                &mut colors.accent,
                &mut colors.background,
                &mut colors.panel,
                &mut colors.editor_background,
                &mut colors.card,
                &mut colors.hover,
                &mut colors.border,
                &mut colors.foreground,
                &mut colors.muted,
                &mut colors.selection,
                &mut colors.cursor,
            ] {
                *color &= 0x00ff_ffff;
            }
        }
        for command in [
            &mut self.general.terminal,
            &mut self.general.shell,
            &mut self.general.external_browser,
            &mut self.general.file_manager,
        ] {
            *command = command.trim().chars().take(512).collect();
        }

        let location = self.general.default_project_location.trim().to_owned();
        self.general.default_project_location =
            if location.is_empty() || std::path::Path::new(&location).is_absolute() {
                location
            } else {
                String::new()
            };
        self.keymap.sanitize();
    }

    pub fn restore_defaults(&mut self) {
        *self = Self::default();
    }
}

const CONTEXT_KEY: &str = "cairn.user-settings.live";

/// Makes the current settings readable from any widget code that only has an
/// `egui::Context`, without threading a parameter through every function.
pub fn publish(ctx: &egui::Context, settings: &UserSettings) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(CONTEXT_KEY), settings.clone()));
}

/// The settings last passed to [`publish`], or defaults before the first frame.
pub fn current(ctx: &egui::Context) -> UserSettings {
    ctx.data(|data| data.get_temp::<UserSettings>(egui::Id::unique(CONTEXT_KEY)))
        .unwrap_or_default()
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_already_valid() {
        let mut settings = UserSettings::default();
        let before = settings.clone();
        settings.sanitize();
        assert_eq!(settings, before);
    }

    #[test]
    fn sanitize_clamps_numbers_and_rejects_nan() {
        let mut settings = UserSettings::default();
        settings.appearance.ui_scale = 9.0;
        settings.appearance.ui_font_size = f32::NAN;
        settings.editor.font_size = 1.0;
        settings.formatting.indent_width = 0;
        settings.general.autosave_interval_minutes = 120;
        settings.sanitize();
        assert_eq!(settings.appearance.ui_scale, MAX_UI_SCALE);
        assert_eq!(settings.appearance.ui_font_size, 14.0);
        assert_eq!(settings.editor.font_size, MIN_FONT_SIZE);
        assert_eq!(settings.editor.tab_width, 1);
        assert_eq!(settings.general.autosave_interval_minutes, 60);
    }

    #[test]
    fn formatter_preferences_are_clamped_and_drive_editor_indentation() {
        let mut settings = UserSettings::default();
        settings.formatting.indent_width = 20;
        settings.formatting.use_tabs = true;
        settings.formatting.line_length = 900;
        settings.sanitize();
        assert_eq!(settings.formatting.indent_width, 8);
        assert!(settings.formatting.use_tabs);
        assert_eq!(settings.formatting.line_length, 240);
        assert_eq!(settings.editor.tab_width, 8);
        assert!(settings.editor.use_tabs);
    }

    #[test]
    fn sanitize_drops_relative_project_locations() {
        let mut settings = UserSettings::default();
        settings.general.default_project_location = "relative/path".to_owned();
        settings.sanitize();
        assert!(settings.general.default_project_location.is_empty());
        let absolute = std::env::current_dir()
            .expect("current directory is absolute")
            .join("cairn-settings-test");
        settings.general.default_project_location = format!(" {} ", absolute.display());
        settings.sanitize();
        assert_eq!(
            settings.general.default_project_location,
            absolute.display().to_string()
        );
    }

    #[test]
    fn default_keymap_has_one_binding_per_action_without_conflicts() {
        let keymap = Keymap::default();
        for action in Action::ALL {
            let binding = keymap.binding(action).expect("binding exists");
            assert!(binding.egui_key().is_some(), "{action:?} key is valid");
            assert_eq!(keymap.conflict_for(binding), None);
        }
    }

    #[test]
    fn rebind_explains_conflicts_and_keeps_old_binding() {
        let mut keymap = Keymap::default();
        let clash = KeyBinding::new(Action::Save, true, false, false, "W");
        let error = keymap.rebind(clash).unwrap_err();
        assert!(error.contains("Ctrl+W"));
        assert!(error.contains("Close tab"));
        assert_eq!(keymap.binding(Action::Save).unwrap().key, "S");
    }

    #[test]
    fn rebind_rejects_bare_keys_and_unknown_keys() {
        let mut keymap = Keymap::default();
        assert!(
            keymap
                .rebind(KeyBinding::new(Action::Save, false, false, false, "S"))
                .is_err()
        );
        assert!(
            keymap
                .rebind(KeyBinding::new(Action::Save, true, false, false, "NotAKey"))
                .is_err()
        );
    }

    #[test]
    fn rebind_accepts_free_chord_and_reset_restores_defaults() {
        let mut keymap = Keymap::default();
        keymap
            .rebind(KeyBinding::new(Action::Save, true, true, false, "S"))
            .unwrap();
        assert_eq!(
            keymap.binding(Action::Save).unwrap().display(),
            "Ctrl+Shift+S"
        );
        keymap.reset();
        assert_eq!(keymap, Keymap::default());
    }

    #[test]
    fn function_key_binding_is_allowed_without_modifiers() {
        let mut keymap = Keymap::default();
        assert_eq!(
            keymap
                .binding(Action::GoToDeclaration)
                .expect("go to declaration shortcut exists")
                .display(),
            "F12"
        );
        keymap
            .rebind(KeyBinding::new(
                Action::GoToDeclaration,
                false,
                true,
                false,
                "F12",
            ))
            .expect("function keys can be bound without Ctrl or Alt");
    }

    #[test]
    fn sanitize_repairs_duplicate_and_missing_bindings() {
        let mut settings = UserSettings::default();
        settings.keymap.bindings = vec![
            KeyBinding::new(Action::Save, true, false, false, "K"),
            KeyBinding::new(Action::CloseTab, true, false, false, "K"),
            KeyBinding::new(Action::ToggleSidePanel, false, false, false, "B"),
        ];
        settings.sanitize();
        assert_eq!(settings.keymap.bindings.len(), Action::ALL.len());
        assert_eq!(settings.keymap.binding(Action::Save).unwrap().key, "K");
        assert_eq!(settings.keymap.binding(Action::CloseTab).unwrap().key, "W");
        assert_eq!(
            settings
                .keymap
                .binding(Action::ToggleSidePanel)
                .unwrap()
                .key,
            "J"
        );
        let panel_toggle = settings
            .keymap
            .binding(Action::ToggleSidePanel)
            .expect("panel toggle has a default binding");
        assert!(panel_toggle.ctrl && panel_toggle.shift);
        for binding in &settings.keymap.bindings {
            assert_eq!(settings.keymap.conflict_for(binding), None);
        }
    }

    #[test]
    fn restore_defaults_resets_everything() {
        let mut settings = UserSettings::default();
        settings.editor.word_wrap = false;
        settings.appearance.accent = AccentChoice::Teal;
        settings.restore_defaults();
        assert_eq!(settings, UserSettings::default());
    }
}
