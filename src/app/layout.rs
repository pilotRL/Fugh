use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLayout {
    pub name: String,
    pub sidebar_side: DockSide,
    pub output_side: DockSide,
    pub sidebar_tab: SidebarTab,
    pub designer_mode: bool,
    pub show_problems: bool,
    #[serde(default)]
    pub show_debugger: bool,
    #[serde(default)]
    pub show_variables: bool,
    pub sidebar_visible: bool,
    pub output_visible: bool,
    pub output_height: f32,
    #[serde(default)]
    pub zen_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidebarTab {
    Project,
    Search,
    Symbols,
    Outline,
    Breadcrumbs,
    SourceControl,
    Breakpoints,
    Helper,
    Process,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockItem {
    Sidebar,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorLayout {
    pub sidebar_side: DockSide,
    pub output_side: DockSide,
    pub sidebar_tab: SidebarTab,
    pub designer_mode: bool,
    #[serde(default)]
    pub show_problems: bool,
    #[serde(default)]
    pub show_debugger: bool,
    #[serde(default)]
    pub show_variables: bool,
    #[serde(default = "default_true")]
    pub sidebar_visible: bool,
    #[serde(default = "default_true")]
    pub output_visible: bool,
    #[serde(default = "default_output_height")]
    pub output_height: f32,
    #[serde(default)]
    pub zen_mode: bool,
    #[serde(default)]
    pub layout_name: String,
    #[serde(default)]
    pub saved_layouts: Vec<SavedLayout>,
}

fn default_true() -> bool {
    true
}

fn default_output_height() -> f32 {
    165.0
}

impl Default for EditorLayout {
    fn default() -> Self {
        Self {
            sidebar_side: DockSide::Right,
            output_side: DockSide::Bottom,
            sidebar_tab: SidebarTab::Project,
            designer_mode: false,
            show_problems: false,
            show_debugger: false,
            show_variables: false,
            sidebar_visible: true,
            output_visible: true,
            output_height: default_output_height(),
            zen_mode: false,
            layout_name: String::new(),
            saved_layouts: Vec::new(),
        }
    }
}

impl EditorLayout {
    pub fn save_named_layout(&mut self) -> Result<(), String> {
        let name = self.layout_name.trim();
        if name.is_empty() {
            return Err("Enter a name for this layout first.".to_owned());
        }
        let snapshot = SavedLayout {
            name: name.to_owned(),
            sidebar_side: self.sidebar_side,
            output_side: self.output_side,
            sidebar_tab: self.sidebar_tab,
            designer_mode: self.designer_mode,
            show_problems: self.show_problems,
            show_debugger: self.show_debugger,
            show_variables: self.show_variables,
            sidebar_visible: self.sidebar_visible,
            output_visible: self.output_visible,
            output_height: self.output_height,
            zen_mode: self.zen_mode,
        };
        if let Some(existing) = self
            .saved_layouts
            .iter_mut()
            .find(|layout| layout.name.eq_ignore_ascii_case(name))
        {
            *existing = snapshot;
        } else {
            self.saved_layouts.push(snapshot);
        }
        Ok(())
    }

    pub fn load_named_layout(&mut self, index: usize) -> Result<(), String> {
        let saved = self
            .saved_layouts
            .get(index)
            .ok_or_else(|| "That saved layout no longer exists.".to_owned())?;
        self.sidebar_side = saved.sidebar_side;
        self.output_side = saved.output_side;
        self.sidebar_tab = saved.sidebar_tab;
        self.designer_mode = saved.designer_mode;
        self.show_problems = saved.show_problems;
        self.show_debugger = saved.show_debugger;
        self.show_variables = saved.show_variables;
        self.sidebar_visible = saved.sidebar_visible;
        self.output_visible = saved.output_visible;
        self.output_height = saved.output_height;
        self.zen_mode = saved.zen_mode;
        self.layout_name = saved.name.clone();
        Ok(())
    }

    pub fn reset_to_default(&mut self) {
        let saved_layouts = std::mem::take(&mut self.saved_layouts);
        *self = Self::default();
        self.saved_layouts = saved_layouts;
    }

    pub fn dock(&mut self, item: DockItem, side: DockSide) {
        let (current, other) = match item {
            DockItem::Sidebar => (&mut self.sidebar_side, &mut self.output_side),
            DockItem::Output => (&mut self.output_side, &mut self.sidebar_side),
        };
        let previous_side = *current;
        *current = side;
        *other = previous_side;
    }
}

#[cfg(test)]
mod tests {
    use super::{DockItem, DockSide, EditorLayout, SidebarTab};

    #[test]
    fn layout_defaults_to_a_project_sidebar_and_bottom_output() {
        let layout = EditorLayout::default();
        assert_eq!(layout.sidebar_side, DockSide::Right);
        assert_eq!(layout.output_side, DockSide::Bottom);
        assert_eq!(layout.sidebar_tab, SidebarTab::Project);
    }

    #[test]
    fn docking_one_panel_swaps_the_panel_already_on_that_side() {
        let mut layout = EditorLayout::default();
        layout.dock(DockItem::Sidebar, DockSide::Bottom);

        assert_eq!(layout.sidebar_side, DockSide::Bottom);
        assert_eq!(layout.output_side, DockSide::Right);

        layout.dock(DockItem::Output, DockSide::Left);
        assert_eq!(layout.output_side, DockSide::Left);
    }

    #[test]
    fn panels_can_dock_at_the_top_without_losing_the_other_panel() {
        let mut layout = EditorLayout::default();
        layout.dock(DockItem::Sidebar, DockSide::Top);
        assert_eq!(layout.sidebar_side, DockSide::Top);
        assert_eq!(layout.output_side, DockSide::Right);
        layout.dock(DockItem::Output, DockSide::Top);
        assert_eq!(layout.output_side, DockSide::Top);
        assert_eq!(layout.sidebar_side, DockSide::Right);
    }

    #[test]
    fn named_layouts_save_load_rename_and_survive_reset() {
        let mut layout = EditorLayout::default();
        layout.layout_name = "Focus".to_owned();
        layout.sidebar_visible = false;
        layout.dock(DockItem::Output, DockSide::Top);
        layout.save_named_layout().expect("named layout saved");
        layout.layout_name = "Focus renamed".to_owned();
        layout.saved_layouts[0].name = layout.layout_name.clone();
        layout.sidebar_visible = true;
        layout.reset_to_default();
        assert_eq!(layout.saved_layouts[0].name, "Focus renamed");
        layout.load_named_layout(0).expect("saved layout loaded");
        assert!(!layout.sidebar_visible);
        assert_eq!(layout.output_side, DockSide::Top);
    }
}
