use serde::{Deserialize, Serialize};

use crate::{app::layout::EditorLayout, project::Workspace, settings::UserSettings};

pub const SETTINGS_KEY: &str = "cairn.settings";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspacePage {
    #[default]
    Welcome,
    Workspace,
    Toolchains,
    Settings,
}

impl WorkspacePage {
    #[cfg(test)]
    pub const ALL: [Self; 4] = [
        Self::Welcome,
        Self::Workspace,
        Self::Toolchains,
        Self::Settings,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::Workspace => "Workspace",
            Self::Toolchains => "Toolchains",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BottomTab {
    #[default]
    Output,
    Problems,
    Terminal,
    DebugConsole,
}

impl BottomTab {
    #[cfg(test)]
    pub const ALL: [Self; 4] = [
        Self::Output,
        Self::Problems,
        Self::Terminal,
        Self::DebugConsole,
    ];

    #[cfg(test)]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Output => "Build Output",
            Self::Problems => "Problems",
            Self::Terminal => "Terminal",
            Self::DebugConsole => "Debug Console",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub page: WorkspacePage,
    pub bottom_tab: BottomTab,
    pub explorer_visible: bool,
    pub properties_visible: bool,
    pub output_visible: bool,
    pub workspace: Workspace,
    pub editor_layout: EditorLayout,
    pub user: UserSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            page: WorkspacePage::Welcome,
            bottom_tab: BottomTab::Output,
            explorer_visible: true,
            properties_visible: true,
            output_visible: true,
            workspace: Workspace::default(),
            editor_layout: EditorLayout::default(),
            user: UserSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppSettings, BottomTab, WorkspacePage};

    #[test]
    fn defaults_to_a_useful_ide_layout() {
        let settings = AppSettings::default();

        assert_eq!(settings.page, WorkspacePage::Welcome);
        assert_eq!(settings.bottom_tab, BottomTab::Output);
        assert!(settings.explorer_visible);
        assert!(settings.properties_visible);
        assert!(settings.output_visible);
        assert!(settings.workspace.documents[0].text.contains("Fugh"));
    }

    #[test]
    fn page_and_bottom_tab_labels_are_available_for_navigation() {
        assert_eq!(
            WorkspacePage::ALL.map(WorkspacePage::label),
            ["Welcome", "Workspace", "Toolchains", "Settings",]
        );
        assert_eq!(
            BottomTab::ALL.map(BottomTab::label),
            ["Build Output", "Problems", "Terminal", "Debug Console",]
        );
    }
}
