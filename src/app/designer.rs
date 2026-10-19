use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use crate::{
    project::{ProjectInfo, ProjectTemplate},
    theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ControlKind {
    Button,
    Label,
    TextBox,
    CheckBox,
    ToggleSwitch,
    Slider,
    ProgressBar,
    RadioButton,
    Separator,
    ComboBox,
    Calendar,
    CalendarDatePicker,
    DatePicker,
    NumericUpDown,
    Image,
    Frame,
    Extra(u8),
}

const EXTRA_CONTROLS: &[(&str, &str, &str, f32, f32)] = &[
    ("Toggle button", "ToggleButton", "Actions", 140.0, 38.0),
    ("Split button", "SplitButton", "Actions", 150.0, 38.0),
    ("Drop-down button", "DropDownButton", "Actions", 150.0, 38.0),
    ("Password box", "TextBox", "Text", 200.0, 36.0),
    ("Auto-complete box", "AutoCompleteBox", "Text", 220.0, 36.0),
    ("List box", "ListBox", "Data Display", 220.0, 140.0),
    ("Tree view", "TreeView", "Data Display", 240.0, 180.0),
    ("Time picker", "TimePicker", "Date and time", 200.0, 40.0),
    ("Grid", "Grid", "Layout", 260.0, 180.0),
    ("Stack panel", "StackPanel", "Layout", 240.0, 160.0),
    ("Wrap panel", "WrapPanel", "Layout", 240.0, 160.0),
    ("Dock panel", "DockPanel", "Layout", 240.0, 160.0),
    ("Scroll viewer", "ScrollViewer", "Layout", 240.0, 160.0),
    ("Expander", "Expander", "Layout", 220.0, 100.0),
    ("Tab control", "TabControl", "Layout", 280.0, 180.0),
    ("Content control", "ContentControl", "Layout", 200.0, 80.0),
    (
        "Items control",
        "ItemsControl",
        "Data Display",
        220.0,
        120.0,
    ),
    ("Menu bar", "Menu", "Containers and Navigation", 420.0, 40.0),
    (
        "Menu item",
        "MenuItem",
        "Containers and Navigation",
        140.0,
        36.0,
    ),
    ("Rectangle", "Rectangle", "Images and Media", 120.0, 80.0),
    ("Ellipse", "Ellipse", "Images and Media", 100.0, 100.0),
    ("Polygon", "Polygon", "Images and Media", 120.0, 100.0),
    ("Tab item", "TabItem", "Collections", 120.0, 36.0),
];

fn extra_control(index: u8) -> Option<(&'static str, &'static str, &'static str, f32, f32)> {
    EXTRA_CONTROLS.get(index as usize).copied()
}

impl ControlKind {
    fn tag(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::Label => "TextBlock",
            Self::TextBox => "TextBox",
            Self::CheckBox => "CheckBox",
            Self::ToggleSwitch => "ToggleSwitch",
            Self::Slider => "Slider",
            Self::ProgressBar => "ProgressBar",
            Self::RadioButton => "RadioButton",
            Self::Separator => "Separator",
            Self::ComboBox => "ComboBox",
            Self::Calendar => "Calendar",
            Self::CalendarDatePicker => "CalendarDatePicker",
            Self::DatePicker => "DatePicker",
            Self::NumericUpDown => "NumericUpDown",
            Self::Image => "Image",
            Self::Frame => "Border",
            Self::Extra(index) => extra_control(index).map_or("Border", |item| item.1),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::Label => "Label",
            Self::TextBox => "Text box",
            Self::CheckBox => "Check box",
            Self::ToggleSwitch => "Toggle switch",
            Self::Slider => "Slider",
            Self::ProgressBar => "Progress bar",
            Self::RadioButton => "Radio button",
            Self::Separator => "Separator",
            Self::ComboBox => "Combo box",
            Self::Calendar => "Calendar",
            Self::CalendarDatePicker => "Calendar date picker",
            Self::DatePicker => "Date picker",
            Self::NumericUpDown => "Numeric up/down",
            Self::Image => "Image",
            Self::Frame => "Frame",
            Self::Extra(index) => extra_control(index).map_or("Control", |item| item.0),
        }
    }

    fn category(self) -> &'static str {
        match self {
            Self::Button | Self::CheckBox | Self::ToggleSwitch | Self::RadioButton => "Actions",
            Self::Label | Self::TextBox => "Text",
            Self::Slider | Self::ProgressBar => "Input and status",
            Self::ComboBox | Self::NumericUpDown => "Input and status",
            Self::Calendar | Self::CalendarDatePicker | Self::DatePicker => "Date and time",
            Self::Separator | Self::Image | Self::Frame => "Layout",
            Self::Extra(index) => extra_control(index).map_or("Other", |item| item.2),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Button => "A clickable action button.",
            Self::Label => "Displays a line of text.",
            Self::TextBox => "Accepts single-line text input.",
            Self::CheckBox => "A boolean selection control.",
            Self::ToggleSwitch => "An on/off switch.",
            Self::Slider => "Selects a numeric value.",
            Self::ProgressBar => "Displays operation progress.",
            Self::RadioButton => "Selects one option in a group.",
            Self::Separator => "A horizontal visual divider.",
            Self::ComboBox => "Select an item from a dropdown list.",
            Self::Calendar => "Displays a calendar for date selection.",
            Self::CalendarDatePicker => "Select a date from a dropdown calendar.",
            Self::DatePicker => "Select a date using a compact date field.",
            Self::NumericUpDown => "Edit a number with spinner buttons.",
            Self::Image => "Displays a bitmap from a project file or URI.",
            Self::Frame => "A bordered surface for grouping interface content.",
            Self::Extra(_) => "A built-in Avalonia control or visual primitive.",
        }
    }

    fn search_terms(self) -> &'static str {
        match self {
            Self::Button => "click command action",
            Self::Label => "textblock text display",
            Self::TextBox => "textbox input text entry",
            Self::CheckBox => "checkbox boolean check",
            Self::ToggleSwitch => "toggle switch boolean on off",
            Self::Slider => "slider number range input",
            Self::ProgressBar => "progress status loading",
            Self::RadioButton => "radio option selection",
            Self::Separator => "separator line divider",
            Self::ComboBox => "dropdown select list selection",
            Self::Calendar => "calendar date day month selection",
            Self::CalendarDatePicker => "calendar date picker select",
            Self::DatePicker => "date picker calendar select",
            Self::NumericUpDown => "numeric number up down spinner input",
            Self::Image => "picture bitmap png jpg image source media",
            Self::Frame => "frame border group panel container",
            Self::Extra(_) => "avalonia built-in layout input text navigation shape",
        }
    }

    fn size(self) -> egui::Vec2 {
        match self {
            Self::Button => egui::vec2(120.0, 38.0),
            Self::Label => egui::vec2(420.0, 36.0),
            Self::TextBox => egui::vec2(200.0, 36.0),
            Self::CheckBox | Self::RadioButton => egui::vec2(170.0, 36.0),
            // The native Fluent template needs more vertical room than our
            // compact preview to avoid clipping its track and thumb.
            Self::ToggleSwitch => egui::vec2(220.0, 64.0),
            Self::Slider => egui::vec2(220.0, 56.0),
            Self::ProgressBar => egui::vec2(220.0, 24.0),
            Self::Separator => egui::vec2(180.0, 3.0),
            Self::ComboBox => egui::vec2(200.0, 36.0),
            Self::Calendar => egui::vec2(340.0, 400.0),
            Self::CalendarDatePicker => egui::vec2(240.0, 44.0),
            Self::DatePicker => egui::vec2(360.0, 48.0),
            Self::NumericUpDown => egui::vec2(160.0, 38.0),
            Self::Image => egui::vec2(240.0, 160.0),
            Self::Frame => egui::vec2(240.0, 160.0),
            Self::Extra(index) => extra_control(index)
                .map(|item| egui::vec2(item.3, item.4))
                .unwrap_or(egui::vec2(160.0, 40.0)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum InspectorTab {
    #[default]
    Properties,
    Events,
}

#[derive(Debug, Clone)]
struct DesignerControl {
    kind: ControlKind,
    name: String,
    text: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font_size: f32,
    margin: f32,
    color: String,
    is_visible: bool,
    is_enabled: bool,
    opacity: f32,
    tooltip: String,
    event_name: Option<String>,
    font_family: String,
    font_weight: String,
    padding: f32,
    /// Parent control name for controls nested inside a designer Frame.
    parent: Option<String>,
    /// XAML attributes without an editor are carried through saves unchanged.
    source_attributes: Vec<(String, String)>,
}

const TITLE_H: f32 = 36.0;

#[derive(Default)]
pub(super) struct WindowDesigner {
    project_path: Option<PathBuf>,
    controls: Vec<DesignerControl>,
    selected: Option<usize>,
    window_selected: bool,
    window_title: String,
    window_width: f32,
    window_height: f32,
    window_icon: String,
    auto_resize_content: bool,
    error: Option<String>,
    canvas_collapsed: bool,
    open_code: Option<PathBuf>,
    open_code_handler: Option<String>,
    source_xaml: Option<String>,
    pending_xaml: Option<String>,
    unsupported_xaml: bool,
    undo_stack: Vec<Vec<DesignerControl>>,
    redo_stack: Vec<Vec<DesignerControl>>,
    clipboard: Option<DesignerControl>,
    zoom_factor: Option<f32>,
    pan: egui::Vec2,
    toolbox_search: String,
    property_search: String,
    event_search: String,
    outline_search: String,
    inspector_tab: InspectorTab,
    recent_controls: Vec<ControlKind>,
    favorite_controls: Vec<ControlKind>,
    left_panel_width: f32,
    right_panel_width: f32,
    outline_list_height: f32,
    preview_mode: bool,
}

pub(super) fn merge_generated_handler(buffer: &mut String, source: &str) -> Result<bool, String> {
    let signature = source.lines().next().unwrap_or_default().trim();
    if signature.is_empty() || buffer.contains(signature) {
        return Ok(false);
    }
    let class_end = buffer
        .rfind('}')
        .ok_or_else(|| "Could not find the class closing brace in the open code tab.".to_owned())?;
    buffer.insert_str(class_end, source);
    Ok(true)
}

impl WindowDesigner {
    pub(super) fn show(&mut self, ui: &mut egui::Ui, project: &ProjectInfo) -> Option<PathBuf> {
        self.open_code = None;
        self.open_code_handler = None;
        self.load_project(project);

        // Designer editing shortcuts are scoped to the designer so they do not
        // intercept typing in a property field.
        if !self.preview_mode && !ui.ctx().egui_wants_keyboard_input() {
            let (copy, cut, paste, duplicate, undo, redo, delete) = ui.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::C),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::X),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::V),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::D),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::Z),
                    input.consume_key(egui::Modifiers::CTRL, egui::Key::Y),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                        || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
                )
            });
            let (escape, fit, move_delta) = ui.input_mut(|input| {
                let ctrl = egui::Modifiers::CTRL;
                let mut movement = egui::Vec2::ZERO;
                let step = if input.modifiers.shift { 10.0 } else { 1.0 };
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                    movement.x -= step;
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                    movement.x += step;
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    movement.y -= step;
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    movement.y += step;
                }
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                    input.consume_key(ctrl, egui::Key::Num0),
                    movement,
                )
            });
            if escape {
                self.selected = None;
                self.window_selected = false;
            }
            if fit {
                self.zoom_factor = Some(1.0);
                self.pan = egui::Vec2::ZERO;
            }
            if move_delta != egui::Vec2::ZERO
                && let Some(index) = self.selected
                && index < self.controls.len()
                && !self.unsupported_xaml
            {
                let bounds = control_parent_bounds(
                    &self.controls,
                    index,
                    self.window_width,
                    self.window_height,
                );
                self.record_snapshot(self.controls.clone());
                if let Some(control) = self.controls.get_mut(index) {
                    control.x =
                        (control.x + move_delta.x).clamp(0.0, (bounds.x - control.width).max(0.0));
                    control.y =
                        (control.y + move_delta.y).clamp(0.0, (bounds.y - control.height).max(0.0));
                }
                self.save_project(project);
            }
            if copy || cut {
                self.copy_selected();
            }
            if cut {
                self.delete_selected(project);
            }
            if paste {
                self.paste_control(project);
            }
            if duplicate {
                self.duplicate_selected(project);
            }
            if undo {
                self.undo(project);
            }
            if redo {
                self.redo(project);
            }
            if delete {
                self.delete_selected(project);
            }
        }

        // Split the whole available area into outline | canvas | properties.
        let full = ui.available_rect_before_wrap();
        ui.allocate_rect(full, egui::Sense::hover());
        // Keep a usable canvas even in narrow windows, then let the side panes
        // scale down instead of retaining widths that push them off-screen.
        let max_side_width = (full.width() - 160.0).max(0.0) * 0.48;
        let left_w = if self.left_panel_width > 0.0 {
            self.left_panel_width
        } else {
            230.0
        }
        .min(max_side_width);
        let right_w = if self.right_panel_width > 0.0 {
            self.right_panel_width
        } else {
            260.0
        }
        .min(max_side_width);
        let left = egui::Rect::from_min_max(full.min, egui::pos2(full.min.x + left_w, full.max.y));
        let right =
            egui::Rect::from_min_max(egui::pos2(full.max.x - right_w, full.min.y), full.max);
        let center = egui::Rect::from_min_max(
            egui::pos2(left.max.x, full.min.y),
            egui::pos2(right.min.x, full.max.y),
        );

        let left_divider = egui::Rect::from_min_max(
            egui::pos2(left.max.x - 4.0, full.min.y),
            egui::pos2(left.max.x + 4.0, full.max.y),
        );
        let left_resize = ui
            .interact(
                left_divider,
                ui.make_persistent_id("designer-left-panel-resize"),
                egui::Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if left_resize.dragged() {
            self.left_panel_width = (left_w + left_resize.drag_motion().x)
                .clamp(170.0_f32.min(max_side_width), 360.0_f32.min(max_side_width));
        }
        let right_divider = egui::Rect::from_min_max(
            egui::pos2(right.min.x - 4.0, full.min.y),
            egui::pos2(right.min.x + 4.0, full.max.y),
        );
        let right_resize = ui
            .interact(
                right_divider,
                ui.make_persistent_id("designer-right-panel-resize"),
                egui::Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if right_resize.dragged() {
            self.right_panel_width = (right_w - right_resize.drag_motion().x)
                .clamp(170.0_f32.min(max_side_width), 360.0_f32.min(max_side_width));
        }

        for panel in [left, right] {
            ui.painter()
                .rect_filled(panel, egui::CornerRadius::ZERO, theme::panel());
        }
        ui.painter().line_segment(
            [left.right_top(), left.right_bottom()],
            egui::Stroke::new(1.0, theme::line()),
        );
        ui.painter().line_segment(
            [right.left_top(), right.left_bottom()],
            egui::Stroke::new(1.0, theme::line()),
        );

        // These panes are positioned manually in `full`. Use child UIs rather
        // than `scope_builder`, which also advances the parent's layout cursor;
        // advancing it after reserving the full rect can leave the left pane
        // clipped/empty after a layout refresh.
        let mut left_ui =
            ui.new_child(egui::UiBuilder::new().max_rect(left.shrink2(egui::vec2(10.0, 8.0))));
        left_ui.set_clip_rect(left.shrink2(egui::vec2(4.0, 4.0)));
        self.draw_outline(&mut left_ui, project);

        let mut right_ui =
            ui.new_child(egui::UiBuilder::new().max_rect(right.shrink2(egui::vec2(12.0, 8.0))));
        right_ui.set_clip_rect(right.shrink2(egui::vec2(4.0, 4.0)));
        right_ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.inspector_tab,
                InspectorTab::Properties,
                "Properties",
            );
            ui.selectable_value(&mut self.inspector_tab, InspectorTab::Events, "Events");
        });
        match self.inspector_tab {
            InspectorTab::Properties => self.draw_properties(&mut right_ui, project),
            InspectorTab::Events => self.draw_events(&mut right_ui, project),
        }

        // Register the drop target on the canvas rect, which is independent of
        // the parent's layout cursor. Draw the canvas after registering it so
        // control selection and resizing remain the topmost interactions.
        let mut canvas_ui = ui.new_child(egui::UiBuilder::new().max_rect(center));
        canvas_ui.set_clip_rect(center);
        self.draw_canvas(&mut canvas_ui, project);
        // Register the drop target after canvas controls so the target remains
        // reachable even when the pointer is over an existing Frame or widget.
        let drop_target = ui.interact(
            center,
            ui.make_persistent_id("designer-canvas-drop-target"),
            egui::Sense::hover(),
        );
        let hovering_control = (!self.preview_mode)
            .then(|| drop_target.dnd_hover_payload::<ControlKind>())
            .flatten();
        let dropped_control = (!self.preview_mode)
            .then(|| drop_target.dnd_release_payload::<ControlKind>())
            .flatten();
        let factor = self.zoom_factor.unwrap_or(1.0);
        let design_window_size = egui::vec2(self.window_width, self.window_height);
        let zoom = (((center.width() - 56.0) / design_window_size.x)
            .min((center.height() - 56.0) / design_window_size.y)
            .clamp(0.3, 1.0)
            * factor)
            .clamp(0.2, 2.0);
        let window_min = center.center() + self.pan - design_window_size * zoom * 0.5;
        if hovering_control.is_some() {
            let world_positions = control_world_positions(&self.controls);
            for (index, control) in self.controls.iter().enumerate() {
                if can_contain_controls(control.kind) {
                    let position = world_positions[index];
                    let rect = egui::Rect::from_min_size(
                        window_min + egui::vec2(position.x, position.y + TITLE_H) * zoom,
                        egui::vec2(control.width, control.height) * zoom,
                    );
                    ui.painter().rect_stroke(
                        rect,
                        egui::CornerRadius::same(4),
                        egui::Stroke::new(2.0, theme::accent_hover()),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        if let Some(kind) = dropped_control {
            let kind = *kind;
            let position = ui.ctx().pointer_interact_pos().map(|pointer| {
                egui::pos2(
                    (pointer.x - window_min.x) / zoom,
                    (pointer.y - window_min.y) / zoom - TITLE_H,
                )
            });
            if let Some(position) = position {
                let world_positions = control_world_positions(&self.controls);
                let parent = self
                    .controls
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(index, control)| {
                        if !can_contain_controls(control.kind) {
                            return None;
                        }
                        let origin = world_positions[index];
                        (position.x >= origin.x
                            && position.y >= origin.y
                            && position.x <= origin.x + control.width
                            && position.y <= origin.y + control.height)
                            .then_some((index, origin))
                    });
                let half_size = kind.size() * 0.5;
                if let Some((parent_index, parent_origin)) = parent {
                    let parent_name = self.controls[parent_index].name.clone();
                    let parent_size = egui::vec2(
                        self.controls[parent_index].width,
                        self.controls[parent_index].height,
                    );
                    let local = egui::pos2(
                        (position.x - parent_origin.x - half_size.x)
                            .clamp(0.0, (parent_size.x - kind.size().x).max(0.0)),
                        (position.y - parent_origin.y - half_size.y)
                            .clamp(0.0, (parent_size.y - kind.size().y).max(0.0)),
                    );
                    self.add_control_at(kind, project, Some(local));
                    let child_name = self.controls.last().map(|child| child.name.clone());
                    if let Some(child) = self.controls.last_mut() {
                        child.parent = Some(parent_name);
                    }
                    normalize_control_order(&mut self.controls);
                    self.selected = child_name.and_then(|name| {
                        self.controls
                            .iter()
                            .position(|control| control.name == name)
                    });
                    self.window_selected = false;
                    self.save_project(project);
                } else {
                    self.add_control_at(
                        kind,
                        project,
                        Some(egui::pos2(
                            position.x - half_size.x,
                            position.y - half_size.y,
                        )),
                    );
                }
                self.note_recent_control(kind);
            }
        }
        paint_control_drag_preview(ui.ctx(), &self.controls);
        self.open_code.take()
    }

    fn draw_outline(&mut self, ui: &mut egui::Ui, project: &ProjectInfo) {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new("DOCUMENT OUTLINE")
                .small()
                .strong()
                .color(theme::muted()),
        );
        let search_button_width = 22.0;
        let search_row_width = (ui.available_width() - 12.0).max(0.0);
        let search_width =
            (search_row_width - search_button_width - ui.spacing().item_spacing.x).max(1.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [search_width, 26.0],
                egui::TextEdit::singleline(&mut self.outline_search).hint_text("Search outline…"),
            );
            if ui
                .add_sized([search_button_width, 26.0], egui::Button::new("×"))
                .clicked()
            {
                self.outline_search.clear();
            }
        });
        let outline_query = self.outline_search.trim().to_ascii_lowercase();
        let max_outline_height = (ui.available_height() - 160.0).max(72.0);
        let outline_scroll_height = if self.outline_list_height > 0.0 {
            self.outline_list_height
        } else {
            ui.available_height() * 0.30
        }
        .clamp(72.0, max_outline_height);
        egui::ScrollArea::vertical()
            .id_salt("designer-outline-list")
            .max_height(outline_scroll_height)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                if outline_row(
                    ui,
                    0,
                    Some(true),
                    "Window",
                    &project.name,
                    self.window_selected,
                )
                .clicked()
                {
                    self.selected = None;
                    self.window_selected = true;
                }
                let collapsed = self.canvas_collapsed;
                if outline_query.is_empty()
                    && outline_row(ui, 1, Some(!collapsed), "Canvas", "", false).clicked()
                {
                    self.canvas_collapsed = !collapsed;
                }
                if !self.canvas_collapsed || !outline_query.is_empty() {
                    let mut reparent = None;
                    for (index, depth) in outline_order(&self.controls) {
                        let (tag, name) = (
                            self.controls[index].kind.tag(),
                            self.controls[index].name.clone(),
                        );
                        if !outline_query.is_empty()
                            && !tag.to_ascii_lowercase().contains(&outline_query)
                            && !name.to_ascii_lowercase().contains(&outline_query)
                        {
                            continue;
                        }
                        let can_contain = can_contain_controls(self.controls[index].kind);
                        let has_children = self
                            .controls
                            .iter()
                            .any(|control| control.parent.as_deref() == Some(name.as_str()));
                        let response = outline_row(
                            ui,
                            2 + depth,
                            can_contain.then_some(has_children),
                            tag,
                            &name,
                            self.selected == Some(index),
                        )
                        .on_hover_text(if can_contain {
                            "Drop a control here to make it a child of this container."
                        } else {
                            "Drag this control onto a container to nest it."
                        });
                        response.dnd_set_drag_payload(index);
                        let hovering_control = can_contain
                            && (response.dnd_hover_payload::<usize>().is_some()
                                || response.dnd_hover_payload::<ControlKind>().is_some());
                        if hovering_control {
                            ui.painter().rect_stroke(
                                response.rect,
                                egui::CornerRadius::same(3),
                                egui::Stroke::new(2.0, theme::accent_hover()),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if can_contain && response.dnd_hover_payload::<usize>().is_some() {
                            if let Some(child) = response.dnd_release_payload::<usize>()
                                && *child != index
                                && !would_create_parent_cycle(&self.controls, *child, index)
                            {
                                reparent = Some((*child, index));
                            }
                        } else if can_contain
                            && response.dnd_hover_payload::<ControlKind>().is_some()
                            && let Some(kind) = response.dnd_release_payload::<ControlKind>()
                        {
                            let kind = *kind;
                            let parent_name = self.controls[index].name.clone();
                            let parent_size =
                                egui::vec2(self.controls[index].width, self.controls[index].height);
                            let position = egui::pos2(
                                ((parent_size.x - kind.size().x) * 0.5).max(0.0),
                                ((parent_size.y - kind.size().y) * 0.5).max(0.0),
                            );
                            self.add_control_at(kind, project, Some(position));
                            let child_name = self.controls.last().map(|child| child.name.clone());
                            if let Some(child) = self.controls.last_mut() {
                                child.parent = Some(parent_name);
                            }
                            normalize_control_order(&mut self.controls);
                            self.selected = child_name.and_then(|name| {
                                self.controls
                                    .iter()
                                    .position(|control| control.name == name)
                            });
                            self.save_project(project);
                        }
                        if response.double_clicked() {
                            self.open_control_code(index, project);
                        } else if response.clicked() {
                            self.selected = Some(index);
                            self.window_selected = false;
                        }
                    }
                    if let Some((child, parent)) = reparent {
                        let before = self.controls.clone();
                        let world_positions = control_world_positions(&self.controls);
                        let parent_position = world_positions[parent];
                        let child_position = world_positions[child];
                        let parent_name = self.controls[parent].name.clone();
                        let child_name = self.controls[child].name.clone();
                        let parent_size =
                            egui::vec2(self.controls[parent].width, self.controls[parent].height);
                        let child_control = &mut self.controls[child];
                        child_control.x = (child_position.x - parent_position.x)
                            .clamp(0.0, (parent_size.x - child_control.width).max(0.0));
                        child_control.y = (child_position.y - parent_position.y)
                            .clamp(0.0, (parent_size.y - child_control.height).max(0.0));
                        child_control.parent = Some(parent_name);
                        normalize_control_order(&mut self.controls);
                        self.record_snapshot(before);
                        self.selected = self
                            .controls
                            .iter()
                            .position(|control| control.name == child_name);
                        self.window_selected = false;
                        self.save_project(project);
                    }
                }
                if !self.unsupported_xaml {
                    ui.label(
                        RichText::new("Drag a control row onto a container row to nest it.")
                            .small()
                            .color(theme::muted()),
                    );
                }
            });
        ui.add_space(3.0);
        let (divider_rect, divider) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::drag());
        let divider = divider.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        ui.painter().line_segment(
            [divider_rect.left_center(), divider_rect.right_center()],
            egui::Stroke::new(1.0, theme::line()),
        );
        if divider.dragged() {
            self.outline_list_height =
                (outline_scroll_height + divider.drag_motion().y).clamp(72.0, max_outline_height);
        }
        ui.add_space(3.0);
        ui.label(
            RichText::new("TOOLBOX")
                .small()
                .strong()
                .color(theme::muted()),
        );
        let search_button_width = 22.0;
        let search_row_width = (ui.available_width() - 12.0).max(0.0);
        let search_width =
            (search_row_width - search_button_width - ui.spacing().item_spacing.x).max(1.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [search_width, 26.0],
                egui::TextEdit::singleline(&mut self.toolbox_search).hint_text("Search controls…"),
            );
            if ui
                .add_sized([search_button_width, 26.0], egui::Button::new("×"))
                .clicked()
            {
                self.toolbox_search.clear();
            }
        });
        let query = self.toolbox_search.trim().to_ascii_lowercase();
        let kinds: Vec<ControlKind> = if project.template == ProjectTemplate::QtWidgetsApp {
            vec![
                ControlKind::Button,
                ControlKind::Label,
                ControlKind::TextBox,
            ]
        } else {
            let mut kinds = vec![
                ControlKind::Button,
                ControlKind::Label,
                ControlKind::TextBox,
                ControlKind::CheckBox,
                ControlKind::ToggleSwitch,
                ControlKind::Slider,
                ControlKind::ProgressBar,
                ControlKind::RadioButton,
                ControlKind::Separator,
                ControlKind::ComboBox,
                ControlKind::Calendar,
                ControlKind::CalendarDatePicker,
                ControlKind::DatePicker,
                ControlKind::NumericUpDown,
                ControlKind::Image,
                ControlKind::Frame,
            ];
            kinds.extend((0..EXTRA_CONTROLS.len()).map(|index| ControlKind::Extra(index as u8)));
            kinds
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 34.0).max(0.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                if !self.recent_controls.is_empty() && query.is_empty() {
                    egui::CollapsingHeader::new("Recently Used")
                        .default_open(true)
                        .show(ui, |ui| {
                            for kind in self.recent_controls.clone() {
                                self.toolbox_item(ui, kind, project);
                            }
                        });
                }
                if !self.favorite_controls.is_empty() && query.is_empty() {
                    egui::CollapsingHeader::new("Favorites")
                        .default_open(true)
                        .show(ui, |ui| {
                            for kind in self.favorite_controls.clone() {
                                self.toolbox_item(ui, kind, project);
                            }
                        });
                }
                for category in [
                    "Actions",
                    "Text",
                    "Input and status",
                    "Date and time",
                    "Layout",
                    "Containers and Navigation",
                    "Images and Media",
                    "Collections",
                    "Data Display",
                ] {
                    let matching = kinds
                        .iter()
                        .copied()
                        .filter(|kind| {
                            kind.category() == category
                                && (query.is_empty()
                                    || kind.title().to_ascii_lowercase().contains(&query)
                                    || kind.category().to_ascii_lowercase().contains(&query)
                                    || kind.description().to_ascii_lowercase().contains(&query)
                                    || kind.search_terms().contains(&query.as_str()))
                        })
                        .collect::<Vec<_>>();
                    if matching.is_empty() {
                        continue;
                    }
                    egui::CollapsingHeader::new(category)
                        .id_salt(("designer-toolbox-category", category))
                        .default_open(!query.is_empty() || category == "Actions")
                        .show(ui, |ui| {
                            for kind in matching {
                                self.toolbox_item(ui, kind, project);
                            }
                        });
                }
                if !query.is_empty()
                    && !kinds.iter().copied().any(|kind| {
                        kind.title().to_ascii_lowercase().contains(&query)
                            || kind.category().to_ascii_lowercase().contains(&query)
                            || kind.description().to_ascii_lowercase().contains(&query)
                            || kind.search_terms().contains(&query.as_str())
                    })
                {
                    ui.label(RichText::new("No matching controls.").color(theme::muted()));
                }
            });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Drag controls to the design surface, or double-click to add. Ctrl+C/X/V copies, cuts, and pastes; Ctrl+D duplicates; Ctrl+Z/Y undo and redo.",
            )
            .small()
            .color(theme::muted()),
        );
    }

    fn toolbox_item(&mut self, ui: &mut egui::Ui, kind: ControlKind, project: &ProjectInfo) {
        let favorite_width = 22.0;
        ui.horizontal(|ui| {
            let response = ui
                .add(
                    egui::Button::new(format!("＋  {}", kind.title()))
                        .truncate()
                        .sense(egui::Sense::click_and_drag()),
                )
                .on_hover_text(format!("{} — {}", kind.title(), kind.description()));
            response.dnd_set_drag_payload(kind);
            let insert = response.clicked() || response.double_clicked();
            if insert {
                self.add_control(kind, project);
                self.note_recent_control(kind);
            }
            let favorite = self.favorite_controls.contains(&kind);
            if ui
                .add_sized(
                    [favorite_width, 28.0],
                    egui::Button::new(if favorite { "★" } else { "☆" }),
                )
                .clicked()
            {
                if favorite {
                    self.favorite_controls.retain(|item| *item != kind);
                } else {
                    self.favorite_controls.push(kind);
                }
            }
        });
    }

    fn note_recent_control(&mut self, kind: ControlKind) {
        self.recent_controls.retain(|recent| *recent != kind);
        self.recent_controls.insert(0, kind);
        self.recent_controls.truncate(5);
    }

    fn draw_events(&mut self, ui: &mut egui::Ui, project: &ProjectInfo) {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new("EVENTS")
                .small()
                .strong()
                .color(theme::muted()),
        );
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 25.0, 26.0],
                egui::TextEdit::singleline(&mut self.event_search).hint_text("Search events…"),
            );
            if ui.small_button("×").clicked() {
                self.event_search.clear();
            }
        });
        if self.unsupported_xaml {
            ui.colored_label(
                Color32::LIGHT_RED,
                "Events are read-only while the XAML contains unsupported controls or properties.",
            );
            return;
        }
        let Some(index) = self.selected.filter(|index| *index < self.controls.len()) else {
            ui.label(
                RichText::new("Select a control to inspect its events.").color(theme::muted()),
            );
            return;
        };
        let control = &self.controls[index];
        ui.label(format!("{} · {}", control.kind.title(), control.name));
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Event").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("Handler").strong());
            });
        });
        let query = self.event_search.trim().to_ascii_lowercase();
        let events = designer_events(control.kind);
        let assigned = control.event_name.clone();
        let control_name = control.name.clone();
        let mut assign = None;
        let mut open_handler = None;
        let before_events_edit = self.controls.clone();
        egui::ScrollArea::vertical()
            .id_salt("designer-events-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for &(event, description) in events {
                    if !query.is_empty()
                        && !event.to_ascii_lowercase().contains(&query)
                        && !description.to_ascii_lowercase().contains(&query)
                    {
                        continue;
                    }
                    let handler = format!("{control_name}_{event}");
                    ui.horizontal(|ui| {
                        let response = ui
                            .selectable_label(assigned.as_deref() == Some(event), event)
                            .on_hover_text(description);
                        if response.clicked() {
                            assign = Some(event.to_owned());
                        }
                        ui.label(
                            RichText::new(if assigned.as_deref() == Some(event) {
                                handler.clone()
                            } else {
                                "—".to_owned()
                            })
                            .small(),
                        );
                        if ui
                            .small_button(if assigned.as_deref() == Some(event) {
                                "Open"
                            } else {
                                "Create"
                            })
                            .clicked()
                        {
                            open_handler = Some(event.to_owned());
                        }
                    });
                }
            });
        if let Some(event) = assign
            && let Some(control) = self.controls.get_mut(index)
        {
            control.event_name = Some(event);
            self.record_snapshot(before_events_edit.clone());
            self.open_control_code(index, project);
        }
        if let Some(event) = open_handler {
            if let Some(control) = self.controls.get_mut(index) {
                control.event_name = Some(event.clone());
            }
            self.record_snapshot(before_events_edit);
            self.open_control_code(index, project);
        }
        ui.label(
            RichText::new(
                "Select an event to assign it; Create/Open generates or navigates to its handler.",
            )
            .small()
            .color(theme::muted()),
        );
    }

    fn draw_properties(&mut self, ui: &mut egui::Ui, project: &ProjectInfo) {
        if self.window_selected {
            self.draw_window_properties(ui, project);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("designer-properties-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let controls_before_edit = self.controls.clone();
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("PROPERTIES")
                        .small()
                        .strong()
                        .color(theme::muted()),
                );
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [ui.available_width() - 25.0, 26.0],
                        egui::TextEdit::singleline(&mut self.property_search)
                            .hint_text("Search properties…"),
                    );
                    if ui.small_button("×").clicked() {
                        self.property_search.clear();
                    }
                });
                ui.add_space(6.0);
                if self.unsupported_xaml {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        self.error
                            .as_deref()
                            .unwrap_or("This XAML is read-only in the visual designer."),
                    );
                    return;
                }
                let mut properties_changed = false;
                let mut renamed_button = None;
                let mut renamed_container = None;
                let query = self.property_search.trim().to_ascii_lowercase();
                let mut selected_property = self
                    .selected
                    .and_then(|index| self.controls.get(index))
                    .and_then(|control| {
                        let property_id = ui.make_persistent_id((
                            "designer-properties-leaf",
                            control.name.as_str(),
                        ));
                        ui.ctx().data(|data| data.get_temp::<String>(property_id))
                    });
                let mut selected_category = "All".to_owned();
                if let Some(control) = self.selected.and_then(|index| self.controls.get(index)) {
                    let category_id = ui.make_persistent_id((
                        "designer-properties-category",
                        control.name.as_str(),
                    ));
                    let property_id = ui.make_persistent_id((
                        "designer-properties-leaf",
                        control.name.as_str(),
                    ));
                    selected_category = ui
                        .ctx()
                        .data(|data| data.get_temp::<String>(category_id))
                        .unwrap_or_else(|| "All".to_owned());
                    egui::CollapsingHeader::new(control.name.clone())
                        .id_salt(category_id)
                        .default_open(true)
                        .show(ui, |ui| {
                            let groups: [(&str, &[(&str, &str)]); 3] = [
                                (
                                    "Common",
                                    &[
                                        ("Name", "name"),
                                        ("Content / items", "text"),
                                        ("State", "visible"),
                                        ("Tooltip", "tooltip"),
                                    ],
                                ),
                                (
                                    "Layout",
                                    &[
                                        ("Position", "position"),
                                        ("Size", "size"),
                                        ("Margin", "margin"),
                                        ("Padding", "padding"),
                                        ("Alignment", "alignment"),
                                    ],
                                ),
                                (
                                    "Appearance",
                                    &[("Font", "font"), ("Color", "color")],
                                ),
                            ];
                            ui.indent("designer-property-tree", |ui| {
                                if ui
                                    .selectable_label(
                                        selected_category == "All" && selected_property.is_none(),
                                        "All properties",
                                    )
                                    .clicked()
                                {
                                    selected_category = "All".to_owned();
                                    selected_property = None;
                                }
                                for (category, properties) in groups {
                                    let category_response = ui.horizontal(|ui| {
                                        let opened = egui::CollapsingHeader::new(category)
                                            .id_salt((category_id, category))
                                            .default_open(category == "Common")
                                            .show(ui, |ui| {
                                                ui.indent(("designer-property-leaves", category), |ui| {
                                                    for (label, key) in properties {
                                                        if ui
                                                            .selectable_label(
                                                                selected_property.as_deref() == Some(*key),
                                                                *label,
                                                            )
                                                            .clicked()
                                                        {
                                                            selected_category = category.to_owned();
                                                            selected_property = Some((*key).to_owned());
                                                        }
                                                    }
                                                });
                                            });
                                        opened.header_response
                                    });
                                    if category_response.inner.clicked() {
                                        selected_category = category.to_owned();
                                        selected_property = None;
                                    }
                                }
                            });
                        });
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(category_id, selected_category.clone());
                        if let Some(property) = &selected_property {
                            data.insert_temp(property_id, property.clone());
                        } else {
                            data.remove::<String>(property_id);
                        }
                    });
                }
                let property_matches = |terms: &[&str]| {
                    (query.is_empty()
                        || terms
                            .iter()
                            .any(|term| term.to_ascii_lowercase().contains(&query)))
                        && selected_property
                            .as_deref()
                            .is_none_or(|selected| terms.contains(&selected))
                };
                let category_matches = |category: &str| {
                    selected_category == "All" || selected_category == category
                };
                let property_bounds = self
                    .selected
                    .map(|index| {
                        control_parent_bounds(
                            &self.controls,
                            index,
                            self.window_width,
                            self.window_height,
                        )
                    })
                    .unwrap_or(egui::vec2(
                        self.window_width,
                        self.window_height - TITLE_H,
                    ));
                let existing_names = self
                    .selected
                    .map(|index| {
                        self.controls
                            .iter()
                            .enumerate()
                            .filter(|(other_index, _)| *other_index != index)
                            .map(|(_, other)| other.name.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if let Some(index) = self.selected
                    && let Some(control) = self.controls.get_mut(index)
                {
                    ui.label(
                        RichText::new(control.kind.title())
                            .size(16.0)
                            .color(theme::accent_hover()),
                    );
                    ui.add_space(8.0);
                    if category_matches("Common")
                        && property_matches(&["name", "identifier", "common"])
                    {
                        ui.label(RichText::new("Name").small().color(theme::muted()));
                        let original_name = control.name.clone();
                        if ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                egui::TextEdit::singleline(&mut control.name),
                            )
                            .changed()
                        {
                            control.name = control_name(&control.name, index, &existing_names);
                            if can_contain_controls(control.kind)
                                && control.name != original_name
                            {
                                renamed_container =
                                    Some((original_name.clone(), control.name.clone()));
                            }
                            if control.kind == ControlKind::Button
                                && control.event_name.is_some()
                                && control.name != original_name
                            {
                                renamed_button = Some((original_name, control.name.clone()));
                            }
                            properties_changed = true;
                        }
                    }
                    if property_matches(&[
                        "text",
                        "content",
                        "watermark",
                        "items",
                        "value",
                        "common",
                    ]) && category_matches("Common") && matches!(
                        control.kind,
                        ControlKind::Button
                            | ControlKind::Label
                            | ControlKind::TextBox
                            | ControlKind::CheckBox
                            | ControlKind::ToggleSwitch
                            | ControlKind::RadioButton
                            | ControlKind::ComboBox
                            | ControlKind::CalendarDatePicker
                            | ControlKind::NumericUpDown
                            | ControlKind::Image
                            | ControlKind::Extra(_)
                    ) {
                        let collection_editor = matches!(control.kind, ControlKind::Extra(index)
                            if extra_control(index).is_some_and(|item| matches!(item.0, "Menu bar" | "Tree view" | "List box" | "Tab control" | "Split button" | "Items control")));
                        let hierarchical_editor = matches!(control.kind, ControlKind::Extra(index)
                            if extra_control(index).is_some_and(|item| matches!(item.0, "Menu bar" | "Tree view")));
                        ui.add_space(6.0);
                        let edit_label = match control.kind {
                            ControlKind::ComboBox => "Items (one per line)",
                            ControlKind::Extra(index)
                                if extra_control(index).is_some_and(|item| item.0 == "Menu bar") =>
                            {
                                "Menu structure"
                            }
                            ControlKind::Extra(index)
                                if extra_control(index).is_some_and(|item| item.0 == "Tree view") =>
                            {
                                "Tree items"
                            }
                            ControlKind::Extra(index)
                                if extra_control(index).is_some_and(|item| item.0 == "List box") =>
                            {
                                "List items (one per line)"
                            }
                            ControlKind::Extra(index)
                                if extra_control(index).is_some_and(|item| item.0 == "Tab control") =>
                            {
                                "Tab titles (one per line)"
                            }
                            ControlKind::Extra(index)
                                if extra_control(index).is_some_and(|item| item.0 == "Split button") =>
                            {
                                "Main action, then dropdown items"
                            }
                            ControlKind::Extra(index)
                                if extra_control(index).is_some_and(|item| item.0 == "Items control") =>
                            {
                                "Items"
                            }
                            ControlKind::CalendarDatePicker => "Placeholder",
                            ControlKind::NumericUpDown => "Value",
                            ControlKind::Image => "Image source",
                            _ => "Text / Content",
                        };
                        ui.label(RichText::new(edit_label).small().color(theme::muted()));
                        let text_changed = if hierarchical_editor {
                            hierarchy_tree_editor(ui, &mut control.text, &control.name)
                        } else if control.kind == ControlKind::ComboBox {
                            ui.add_sized(
                                [ui.available_width(), 72.0],
                                egui::TextEdit::multiline(&mut control.text)
                                    .desired_rows(3)
                                    .hint_text("Option 1\nOption 2"),
                            )
                            .changed()
                        } else if collection_editor {
                            let item_label = match control.kind {
                                ControlKind::Extra(index)
                                    if extra_control(index).is_some_and(|item| item.0 == "Tab control") => "Tab",
                                ControlKind::Extra(index)
                                    if extra_control(index).is_some_and(|item| item.0 == "Split button") => "Option",
                                _ => "Item",
                            };
                            flat_collection_editor(ui, &mut control.text, &control.name, item_label)
                        } else if control.kind == ControlKind::NumericUpDown {
                            let mut value = control.text.parse::<f64>().unwrap_or(5.0);
                            let changed = ui
                                .add(egui::DragValue::new(&mut value).range(0.0..=100.0))
                                .changed();
                            if changed {
                                control.text = value.to_string();
                            }
                            changed
                        } else if control.kind == ControlKind::Image {
                            ui.add_sized(
                                [ui.available_width(), 28.0],
                                egui::TextEdit::singleline(&mut control.text)
                                    .hint_text("/Assets/image.png or avares://…"),
                            )
                            .changed()
                        } else {
                            ui.add_sized(
                                [ui.available_width(), 28.0],
                                egui::TextEdit::singleline(&mut control.text),
                            )
                            .changed()
                        };
                        if text_changed {
                            // Keep labels and buttons wide enough to show their text on one line
                            // in both the visual preview and the generated Avalonia window.
                            if matches!(
                                control.kind,
                                ControlKind::Button
                                    | ControlKind::Label
                                    | ControlKind::CheckBox
                                    | ControlKind::ToggleSwitch
                                    | ControlKind::RadioButton
                            ) {
                                let text_width = ui.fonts_mut(|fonts| {
                                    fonts
                                        .layout_no_wrap(
                                            control.text.clone(),
                                            egui::FontId::proportional(control.font_size),
                                            theme::text(),
                                        )
                                        .size()
                                        .x
                                });
                                let leading = match control.kind {
                                    ControlKind::CheckBox | ControlKind::RadioButton => 36.0,
                                    ControlKind::ToggleSwitch => 58.0,
                                    _ => 24.0,
                                };
                                control.width = control.width.max(
                                    (text_width + leading + control.padding * 2.0)
                                        .min((property_bounds.x - control.x).max(0.0)),
                                );
                            }
                            properties_changed = true;
                        }
                    }
                    if category_matches("Layout")
                        && property_matches(&["position", "x", "y", "layout"])
                    {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Position").small().color(theme::muted()));
                        ui.horizontal(|ui| {
                            ui.label("X");
                            properties_changed |= ui
                                .add(egui::DragValue::new(&mut control.x).range(
                                    0.0..=(property_bounds.x - control.width).max(0.0),
                                ))
                                .changed();
                            ui.label("Y");
                            properties_changed |= ui
                                .add(egui::DragValue::new(&mut control.y).range(
                                    0.0..=(property_bounds.y - control.height).max(0.0),
                                ))
                                .changed();
                        });
                    }
                    if category_matches("Layout")
                        && property_matches(&["size", "width", "height", "layout"])
                    {
                        ui.label(RichText::new("Size").small().color(theme::muted()));
                        ui.horizontal(|ui| {
                            ui.label("W");
                            properties_changed |= ui
                                .add(egui::DragValue::new(&mut control.width).range(
                                    24.0..=(property_bounds.x - control.x).max(24.0),
                                ))
                                .changed();
                            ui.label("H");
                            properties_changed |= ui
                                .add(egui::DragValue::new(&mut control.height).range(
                                    20.0..=(property_bounds.y - control.y).max(20.0),
                                ))
                                .changed();
                        });
                    }
                    if category_matches("Layout")
                        && property_matches(&["margin", "spacing", "layout"])
                    {
                        ui.horizontal(|ui| {
                            ui.label("Margin");
                            properties_changed |= ui
                                .add(egui::DragValue::new(&mut control.margin).range(0.0..=64.0))
                                .changed();
                        });
                    }
                    if category_matches("Appearance")
                        && supports_font_properties(control.kind)
                        && property_matches(&["font", "font size", "appearance"])
                    {
                        ui.horizontal(|ui| {
                            ui.label("Font size");
                            properties_changed |= ui
                                .add(egui::DragValue::new(&mut control.font_size).range(8.0..=72.0))
                                .changed();
                        });
                        ui.label(RichText::new("Font family").small().color(theme::muted()));
                        properties_changed |= ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                egui::TextEdit::singleline(&mut control.font_family)
                                    .hint_text("Theme default"),
                            )
                            .changed();
                        ui.horizontal(|ui| {
                            ui.label("Weight");
                            egui::ComboBox::from_id_salt(("designer-font-weight", index))
                                .selected_text(if control.font_weight.is_empty() {
                                    "Default"
                                } else {
                                    &control.font_weight
                                })
                                .show_ui(ui, |ui| {
                                    for weight in ["Default", "Light", "Normal", "Bold"] {
                                        let value = if weight == "Default" { "" } else { weight };
                                        properties_changed |= ui
                                            .selectable_value(
                                                &mut control.font_weight,
                                                value.to_owned(),
                                                weight,
                                            )
                                            .changed();
                                    }
                                });
                        });
                    }
                    if category_matches("Layout")
                        && property_matches(&["padding", "appearance", "layout"])
                        && matches!(
                            control.kind,
                            ControlKind::Button
                                | ControlKind::TextBox
                                | ControlKind::CheckBox
                                | ControlKind::ToggleSwitch
                                | ControlKind::RadioButton
                        )
                    {
                        ui.horizontal(|ui| {
                            ui.label("Padding");
                            properties_changed |= ui
                                .add(egui::DragValue::new(&mut control.padding).range(0.0..=48.0))
                                .changed();
                        });
                    }
                    if category_matches("Appearance")
                        && supports_color_property(control.kind)
                        && property_matches(&[
                        "color",
                        "brush",
                        "background",
                        "foreground",
                        "appearance",
                    ])
                    {
                        ui.horizontal(|ui| {
                            ui.label("Color");
                            let mut color = parse_color(&control.color).unwrap_or(Color32::WHITE);
                            if ui.color_edit_button_srgba(&mut color).changed() {
                                control.color = format!(
                                    "#{:02X}{:02X}{:02X}{:02X}",
                                    color.a(),
                                    color.r(),
                                    color.g(),
                                    color.b()
                                );
                                properties_changed = true;
                            }
                        });
                    }
                    if (category_matches("Common") || category_matches("Appearance"))
                        && property_matches(&["visible", "enabled", "opacity", "common", "appearance"])
                    {
                        properties_changed |=
                            ui.checkbox(&mut control.is_visible, "Visible").changed();
                        properties_changed |=
                            ui.checkbox(&mut control.is_enabled, "Enabled").changed();
                        ui.horizontal(|ui| {
                            ui.label("Opacity");
                            properties_changed |= ui
                                .add(
                                    egui::Slider::new(&mut control.opacity, 0.0..=1.0)
                                        .show_value(true),
                                )
                                .changed();
                        });
                    }
                    if category_matches("Common")
                        && property_matches(&["tooltip", "hover", "common"])
                    {
                        ui.label(RichText::new("Tooltip").small().color(theme::muted()));
                        properties_changed |= ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                egui::TextEdit::singleline(&mut control.tooltip)
                                    .hint_text("Optional help text"),
                            )
                            .changed();
                    }
                    if category_matches("Layout")
                        && property_matches(&["align", "alignment", "layout"])
                    {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(if control.parent.is_some() {
                                "Align to parent"
                            } else {
                                "Align to window"
                            })
                                .small()
                                .color(theme::muted()),
                        );
                        ui.horizontal(|ui| {
                            if ui.small_button("Left").clicked() {
                                control.x = 0.0;
                                properties_changed = true;
                            }
                            if ui.small_button("Center X").clicked() {
                                control.x = ((property_bounds.x - control.width) * 0.5).max(0.0);
                                properties_changed = true;
                            }
                            if ui.small_button("Right").clicked() {
                                control.x = (property_bounds.x - control.width).max(0.0);
                                properties_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.small_button("Top").clicked() {
                                control.y = 0.0;
                                properties_changed = true;
                            }
                            if ui.small_button("Center Y").clicked() {
                                control.y = ((property_bounds.y - control.height) * 0.5).max(0.0);
                                properties_changed = true;
                            }
                            if ui.small_button("Bottom").clicked() {
                                control.y = (property_bounds.y - control.height).max(0.0);
                                properties_changed = true;
                            }
                        });
                    }
                } else {
                    ui.label(
                        RichText::new("Select a control to edit its properties.")
                            .color(theme::muted()),
                    );
                }
                let handler_error = renamed_button.and_then(|(old_name, new_name)| {
                    ensure_control_handler(project, &new_name)
                        .err()
                        .map(|error| (old_name, error))
                });
                if let Some((old_name, _)) = &handler_error
                    && let Some(index) = self.selected
                    && let Some(control) = self.controls.get_mut(index)
                {
                    control.name.clone_from(old_name);
                }
                if let Some((old_name, new_name)) = renamed_container {
                    for control in &mut self.controls {
                        if control.parent.as_deref() == Some(old_name.as_str()) {
                            control.parent = Some(new_name.clone());
                        }
                    }
                }
                if properties_changed {
                    self.record_snapshot(controls_before_edit);
                    self.save_project(project);
                }
                if let Some((_, error)) = handler_error {
                    self.error = Some(error);
                }
                ui.add_space(12.0);
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                } else {
                    ui.label(
                        RichText::new(if self.pending_xaml.is_some() {
                            "Unsaved changes — use Save to write MainWindow.axaml."
                        } else {
                            "All changes saved to MainWindow.axaml."
                        })
                            .small()
                            .color(theme::muted()),
                    );
                }
            });
    }

    fn draw_window_properties(&mut self, ui: &mut egui::Ui, project: &ProjectInfo) {
        let mut changed = false;
        egui::ScrollArea::vertical()
            .id_salt("designer-window-properties")
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("WINDOW PROPERTIES")
                        .small()
                        .strong()
                        .color(theme::muted()),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("Title").small().color(theme::muted()));
                changed |= ui
                    .add_sized(
                        [ui.available_width(), 28.0],
                        egui::TextEdit::singleline(&mut self.window_title),
                    )
                    .changed();
                ui.add_space(8.0);
                ui.label(RichText::new("Size").small().color(theme::muted()));
                ui.horizontal(|ui| {
                    ui.label("Width");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.window_width)
                                .range(320.0..=4000.0)
                                .suffix(" px"),
                        )
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Height");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.window_height)
                                .range(240.0..=3000.0)
                                .suffix(" px"),
                        )
                        .changed();
                });
                changed |= ui
                    .checkbox(
                        &mut self.auto_resize_content,
                        "Auto-resize content with window",
                    )
                    .changed();
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Window icon (.ico)")
                        .small()
                        .color(theme::muted()),
                );
                ui.horizontal(|ui| {
                    if ui.button("Choose icon…").clicked()
                        && let Some(source) = rfd::FileDialog::new()
                            .add_filter("Icon", &["ico"])
                            .pick_file()
                    {
                        match copy_window_icon(project, &source) {
                            Ok(icon) => {
                                self.window_icon = icon;
                                changed = true;
                                self.error = None;
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                    if !self.window_icon.is_empty() && ui.small_button("Clear").clicked() {
                        self.window_icon.clear();
                        changed = true;
                    }
                });
                if self.window_icon.is_empty() {
                    ui.label(
                        RichText::new("No icon selected")
                            .small()
                            .color(theme::muted()),
                    );
                } else {
                    ui.label(
                        RichText::new(&self.window_icon)
                            .small()
                            .color(theme::text()),
                    );
                }
                if changed {
                    self.window_width = self.window_width.clamp(320.0, 4000.0);
                    self.window_height = self.window_height.clamp(240.0, 3000.0);
                    for control in &mut self.controls {
                        if matches!(control.kind, ControlKind::Extra(index)
                            if extra_control(index).is_some_and(|item| item.0 == "Menu bar"))
                        {
                            control.x = 0.0;
                            control.y = 0.0;
                            control.width = self.window_width;
                        }
                    }
                }
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            });
        if changed {
            self.save_project(project);
        }
    }

    fn load_project(&mut self, project: &ProjectInfo) {
        let project_path = &project.path;
        if self.project_path.as_deref() == Some(project_path) {
            if project.template == ProjectTemplate::AvaloniaApp {
                let xaml_path = project_path.join("MainWindow.axaml");
                if let Ok(contents) = std::fs::read_to_string(&xaml_path)
                    && self.source_xaml.as_deref() != Some(contents.as_str())
                {
                    let controls = parse_controls(&contents);
                    self.load_window_settings(&contents, project);
                    self.unsupported_xaml = !is_supported_xaml(&project.name, &contents, &controls);
                    self.controls = controls;
                    self.undo_stack.clear();
                    self.redo_stack.clear();
                    self.source_xaml = Some(contents);
                    self.pending_xaml = None;
                    self.selected = None;
                    self.upgrade_clipped_control_heights(project);
                    self.error = self.unsupported_xaml.then(|| {
                        format!(
                            "{} The file is read-only in the designer until it is fixed or edited in Code view.",
                            self.source_xaml
                                .as_deref()
                                .map(describe_unsupported_xaml)
                                .unwrap_or_default()
                        )
                    });
                }
            }
            return;
        }
        self.window_selected = false;
        if project.template == ProjectTemplate::QtWidgetsApp {
            let layout_path = project_path.join("MainWindow.cairn-ui");
            match std::fs::read_to_string(&layout_path) {
                Ok(contents) => {
                    self.controls = parse_qt_controls(&contents);
                    self.undo_stack.clear();
                    self.redo_stack.clear();
                    self.project_path = Some(project_path.to_path_buf());
                    self.selected = None;
                    self.error = None;
                    self.source_xaml = None;
                    self.pending_xaml = None;
                    self.unsupported_xaml = false;
                }
                Err(error) => {
                    self.project_path = Some(project_path.to_path_buf());
                    self.controls.clear();
                    self.error = Some(format!(
                        "Could not load '{}': {error}",
                        layout_path.display()
                    ));
                }
            }
            return;
        }
        use_generated_initialize_component(project);
        let xaml_path = project_path.join("MainWindow.axaml");
        match std::fs::read_to_string(&xaml_path) {
            Ok(contents) => {
                self.controls = parse_controls(&contents);
                self.load_window_settings(&contents, project);
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.unsupported_xaml =
                    !is_supported_xaml(&project.name, &contents, &self.controls);
                self.source_xaml = Some(contents.clone());
                self.pending_xaml = None;
                self.project_path = Some(project_path.to_path_buf());
                self.selected = None;
                self.upgrade_clipped_control_heights(project);
                self.error = self.unsupported_xaml.then(|| {
                    format!(
                        "{} The file is read-only in the designer until it is fixed or edited in Code view.",
                        describe_unsupported_xaml(&contents)
                    )
                });
            }
            Err(error) => {
                self.project_path = Some(project_path.to_path_buf());
                self.controls.clear();
                self.source_xaml = None;
                self.pending_xaml = None;
                self.unsupported_xaml = true;
                self.error = Some(format!("Could not load '{}': {error}", xaml_path.display()));
            }
        }
    }

    fn load_window_settings(&mut self, xaml: &str, project: &ProjectInfo) {
        if let Ok(document) = roxmltree::Document::parse(xaml) {
            let root = document.root_element();
            self.window_title = root.attribute("Title").unwrap_or(&project.name).to_owned();
            self.window_width = root
                .attribute("Width")
                .and_then(|value| value.parse().ok())
                .unwrap_or(1100.0);
            self.window_height = root
                .attribute("Height")
                .and_then(|value| value.parse().ok())
                .unwrap_or(760.0);
            self.window_icon = root.attribute("Icon").unwrap_or_default().to_owned();
            self.auto_resize_content = root
                .children()
                .find(roxmltree::Node::is_element)
                .filter(|node| node.tag_name().name() == "Viewbox")
                .and_then(|node| node.attribute("Stretch"))
                .is_none_or(|stretch| stretch == "Uniform");
        }
    }

    fn upgrade_clipped_control_heights(&mut self, project: &ProjectInfo) {
        if self.unsupported_xaml {
            return;
        }
        let mut changed = false;
        for control in &mut self.controls {
            let minimum_size: f32 = match control.kind {
                ControlKind::ToggleSwitch => 64.0,
                ControlKind::Slider => 56.0,
                ControlKind::Calendar => 400.0,
                ControlKind::CalendarDatePicker => 44.0,
                ControlKind::DatePicker => 48.0,
                _ => 20.0,
            };
            if let ControlKind::Extra(index) = control.kind {
                match extra_control(index).map(|item| item.0) {
                    Some("Menu bar") if control.text.trim().is_empty() => {
                        control.text =
                            "File\n  New\n  Open\nEdit\n  Undo\n  Redo\nView\nHelp".to_owned();
                        changed = true;
                    }
                    Some("Tree view") if control.text.trim().is_empty() => {
                        control.text = "Item 1\n  Child 1\n  Child 2\nItem 2\n  Child 3".to_owned();
                        changed = true;
                    }
                    Some("List box") if control.text.trim().is_empty() => {
                        control.text = "Item 1\nItem 2\nItem 3".to_owned();
                        changed = true;
                    }
                    Some("Tab control") if control.text.trim().is_empty() => {
                        control.text = "Home\nSettings\nReports".to_owned();
                        changed = true;
                    }
                    Some("Split button") if control.text.lines().count() < 2 => {
                        let label = control.text.lines().next().unwrap_or("Main action");
                        control.text = format!("{label}\nOption 1\nOption 2");
                        changed = true;
                    }
                    _ => {}
                }
            }
            let old = (control.x, control.y, control.width, control.height);
            let minimum_width = match control.kind {
                ControlKind::Calendar => 340.0,
                ControlKind::CalendarDatePicker => 240.0,
                ControlKind::DatePicker => 360.0,
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Menu bar") =>
                {
                    self.window_width
                }
                _ => 24.0,
            };
            control.width = control.width.max(minimum_width);
            control.height = control.height.max(minimum_size.max(20.0));
            control.x = control.x.max(0.0);
            control.y = control.y.max(0.0);
            if matches!(control.kind, ControlKind::Extra(index)
                if extra_control(index).is_some_and(|item| item.0 == "Menu bar"))
            {
                control.x = 0.0;
                control.y = 0.0;
            }
            if old != (control.x, control.y, control.width, control.height) {
                changed = true;
            }
        }
        for _ in 0..self.controls.len() {
            for frame_index in (0..self.controls.len()).rev() {
                if !can_contain_controls(self.controls[frame_index].kind) {
                    continue;
                }
                let frame_name = self.controls[frame_index].name.clone();
                let required = self
                    .controls
                    .iter()
                    .filter(|child| child.parent.as_deref() == Some(frame_name.as_str()))
                    .fold(
                        egui::vec2(
                            self.controls[frame_index].width,
                            self.controls[frame_index].height,
                        ),
                        |size, child| {
                            egui::vec2(
                                size.x.max(child.x + child.width + 16.0),
                                size.y.max(child.y + child.height + 16.0),
                            )
                        },
                    );
                if required.x > self.controls[frame_index].width
                    || required.y > self.controls[frame_index].height
                {
                    self.controls[frame_index].width = required.x;
                    self.controls[frame_index].height = required.y;
                    changed = true;
                }
            }
        }
        if changed {
            self.save_project(project);
        }
    }

    fn add_control(&mut self, kind: ControlKind, project: &ProjectInfo) {
        self.add_control_at(kind, project, None);
    }

    fn add_control_at(
        &mut self,
        kind: ControlKind,
        project: &ProjectInfo,
        position: Option<egui::Pos2>,
    ) {
        if self.unsupported_xaml {
            self.error = Some("This XAML is read-only in the visual designer because it contains unsupported markup.".to_owned());
            return;
        }
        let index = self.controls.len() + 1;
        let is_menu_bar = matches!(kind, ControlKind::Extra(index)
            if extra_control(index).is_some_and(|item| item.0 == "Menu bar"));
        let is_tree_view = matches!(kind, ControlKind::Extra(index)
            if extra_control(index).is_some_and(|item| item.0 == "Tree view"));
        let name = format!("{}{}", kind.tag(), index);
        self.record_snapshot(self.controls.clone());
        self.controls.push(DesignerControl {
            kind,
            name: name.clone(),
            text: match kind {
                ControlKind::Button => "Button".to_owned(),
                ControlKind::Label => "Label".to_owned(),
                ControlKind::TextBox => String::new(),
                ControlKind::CheckBox => "Check me".to_owned(),
                ControlKind::ToggleSwitch => "Enabled".to_owned(),
                ControlKind::Slider => "Value".to_owned(),
                ControlKind::ProgressBar => String::new(),
                ControlKind::RadioButton => "Option".to_owned(),
                ControlKind::Separator => String::new(),
                ControlKind::ComboBox => "Option 1\nOption 2".to_owned(),
                ControlKind::Calendar => String::new(),
                ControlKind::CalendarDatePicker | ControlKind::DatePicker => String::new(),
                ControlKind::NumericUpDown => "5".to_owned(),
                ControlKind::Image => "".to_owned(),
                ControlKind::Frame => String::new(),
                ControlKind::Extra(_) if is_menu_bar => {
                    "File\n  New\n  Open\nEdit\n  Undo\n  Redo\nView\nHelp".to_owned()
                }
                ControlKind::Extra(_) if is_tree_view => {
                    "Item 1\n  Child 1\n  Child 2\nItem 2\n  Child 3".to_owned()
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "List box") =>
                {
                    "Item 1\nItem 2\nItem 3".to_owned()
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Items control") =>
                {
                    "Item 1\nItem 2\nItem 3".to_owned()
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Tab control") =>
                {
                    "Home\nSettings\nReports".to_owned()
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Split button") =>
                {
                    "Main action\nOption 1\nOption 2".to_owned()
                }
                ControlKind::Extra(_) => kind.title().to_owned(),
            },
            x: if is_menu_bar {
                0.0
            } else {
                position
                    .map(|position| {
                        position
                            .x
                            .clamp(0.0, (self.window_width - kind.size().x).max(0.0))
                    })
                    .unwrap_or(
                        (36.0 + ((index - 1) % 4) as f32 * 32.0)
                            .clamp(0.0, (self.window_width - kind.size().x).max(0.0)),
                    )
            },
            y: if is_menu_bar {
                0.0
            } else {
                position
                    .map(|position| {
                        position
                            .y
                            .clamp(0.0, (self.window_height - TITLE_H - kind.size().y).max(0.0))
                    })
                    .unwrap_or(
                        (92.0 + ((index - 1) / 4) as f32 * 48.0)
                            .clamp(0.0, (self.window_height - TITLE_H - kind.size().y).max(0.0)),
                    )
            },
            width: if is_menu_bar {
                self.window_width
            } else {
                kind.size().x
            },
            height: kind.size().y,
            font_size: 14.0,
            margin: 0.0,
            color: default_control_color(kind).to_owned(),
            is_visible: true,
            is_enabled: true,
            opacity: 1.0,
            tooltip: String::new(),
            font_family: String::new(),
            font_weight: String::new(),
            padding: 0.0,
            parent: None,
            event_name: (kind == ControlKind::Button
                && project.template == ProjectTemplate::QtWidgetsApp)
                .then(|| "Click".to_owned()),
            source_attributes: Vec::new(),
        });
        self.selected = Some(index - 1);
        self.window_selected = false;
        if kind == ControlKind::Button
            && project.template == ProjectTemplate::QtWidgetsApp
            && let Err(error) = ensure_control_handler(project, &name)
        {
            self.error = Some(error);
            return;
        }
        self.save_project(project);
    }

    fn record_snapshot(&mut self, snapshot: Vec<DesignerControl>) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn copy_selected(&mut self) {
        if let Some(index) = self.selected
            && let Some(control) = self.controls.get(index)
        {
            self.clipboard = Some(control.clone());
        }
    }

    fn delete_selected(&mut self, project: &ProjectInfo) {
        if self.unsupported_xaml {
            return;
        }
        if let Some(index) = self.selected.take()
            && index < self.controls.len()
        {
            let world_positions = control_world_positions(&self.controls);
            let deleted_name = self.controls[index].name.clone();
            self.record_snapshot(self.controls.clone());
            for (child_index, control) in self.controls.iter_mut().enumerate() {
                if control.parent.as_deref() == Some(deleted_name.as_str()) {
                    control.parent = None;
                    control.x = world_positions[child_index].x;
                    control.y = world_positions[child_index].y;
                }
            }
            self.controls.remove(index);
            normalize_control_order(&mut self.controls);
            self.save_project(project);
        }
    }

    fn paste_control(&mut self, project: &ProjectInfo) {
        if self.unsupported_xaml {
            return;
        }
        let Some(mut control) = self.clipboard.clone() else {
            return;
        };
        self.record_snapshot(self.controls.clone());
        let existing = self
            .controls
            .iter()
            .map(|item| item.name.clone())
            .collect::<Vec<_>>();
        control.name = control_name(
            &format!("{}Copy", control.kind.tag()),
            self.controls.len(),
            &existing,
        );
        let bounds = control
            .parent
            .as_deref()
            .and_then(|name| {
                self.controls
                    .iter()
                    .find(|candidate| candidate.name == name)
            })
            .filter(|parent| can_contain_controls(parent.kind))
            .map(|parent| egui::vec2(parent.width, parent.height))
            .unwrap_or(egui::vec2(self.window_width, self.window_height - TITLE_H));
        control.x = (control.x + 24.0).clamp(0.0, (bounds.x - control.width).max(0.0));
        control.y = (control.y + 24.0).clamp(0.0, (bounds.y - control.height).max(0.0));
        // A copy must not silently share the original control's event handler.
        control.event_name = None;
        self.controls.push(control);
        self.selected = Some(self.controls.len() - 1);
        self.window_selected = false;
        self.save_project(project);
    }

    fn duplicate_selected(&mut self, project: &ProjectInfo) {
        self.copy_selected();
        self.paste_control(project);
    }

    fn undo(&mut self, project: &ProjectInfo) {
        if let Some(previous) = self.undo_stack.pop() {
            self.redo_stack
                .push(std::mem::replace(&mut self.controls, previous));
            self.selected = self.selected.filter(|index| *index < self.controls.len());
            self.save_project(project);
        }
    }

    fn redo(&mut self, project: &ProjectInfo) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack
                .push(std::mem::replace(&mut self.controls, next));
            self.selected = self.selected.filter(|index| *index < self.controls.len());
            self.save_project(project);
        }
    }

    fn draw_canvas(&mut self, ui: &mut egui::Ui, project: &ProjectInfo) {
        let controls_before_drag = self.controls.clone();
        let mut drag_started = false;
        let area = ui.max_rect();
        // Paint the canvas first so the zoom controls remain above it.
        ui.painter()
            .rect_filled(area, egui::CornerRadius::ZERO, theme::bg_dark());
        let mut factor = self.zoom_factor.unwrap_or(1.0);
        let canvas_hovered = ui.input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|position| area.contains(position))
        });
        let (wheel_zoom, zoom_in, zoom_out) = ui.input_mut(|input| {
            // egui converts Ctrl+wheel into zoom_delta instead of a scroll
            // delta. Read it directly before another ScrollArea consumes it.
            let wheel_zoom = if canvas_hovered {
                input.zoom_delta()
            } else {
                1.0
            };
            let zoom_in = canvas_hovered
                && (input.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Plus)
                    || input.consume_key(
                        egui::Modifiers {
                            ctrl: true,
                            shift: true,
                            ..egui::Modifiers::NONE
                        },
                        egui::Key::Equals,
                    ));
            let zoom_out = canvas_hovered
                && (input.consume_key(egui::Modifiers::CTRL, egui::Key::Minus)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Minus));
            (wheel_zoom, zoom_in, zoom_out)
        });
        if (wheel_zoom - 1.0).abs() > f32::EPSILON {
            factor = (factor * wheel_zoom).clamp(0.5, 2.0);
        }
        if zoom_in || zoom_out {
            factor = (factor + if zoom_in { 0.1 } else { -0.1 }).clamp(0.5, 2.0);
        }
        self.zoom_factor = Some(factor);
        if !self.preview_mode
            && ui.input(|input| {
                input.pointer.button_down(egui::PointerButton::Middle)
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|position| area.contains(position))
            })
        {
            self.pan += ui.input(|input| input.pointer.delta());
            self.pan.x = self.pan.x.clamp(-area.width() * 0.75, area.width() * 0.75);
            self.pan.y = self
                .pan
                .y
                .clamp(-area.height() * 0.75, area.height() * 0.75);
        }
        // Workspace dotted grid.
        let margin = 28.0;
        let design_window_size = egui::vec2(
            self.window_width.clamp(320.0, 4000.0),
            self.window_height.clamp(240.0, 3000.0),
        );
        let zoom = (((area.width() - margin * 2.0) / design_window_size.x)
            .min((area.height() - margin * 2.0) / design_window_size.y)
            .clamp(0.3, 1.0)
            * factor)
            .clamp(0.2, 2.0);
        let size = design_window_size * zoom;
        let window = egui::Rect::from_center_size(area.center() + self.pan, size);
        let painter = ui.painter().with_clip_rect(area);

        // Window chrome: shadow, title bar, caption buttons, body.
        painter.rect_filled(
            window.translate(egui::vec2(0.0, 6.0)).expand(2.0),
            egui::CornerRadius::same(10),
            Color32::from_black_alpha(90),
        );
        painter.rect_filled(window, egui::CornerRadius::same(8), theme::editor());
        let title_rect =
            egui::Rect::from_min_size(window.min, egui::vec2(window.width(), TITLE_H * zoom));
        painter.rect_filled(
            title_rect,
            egui::CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            },
            theme::panel(),
        );
        painter.text(
            title_rect.left_center() + egui::vec2(14.0, 0.0),
            egui::Align2::LEFT_CENTER,
            &self.window_title,
            egui::FontId::proportional((13.0 * zoom).max(11.0)),
            theme::text(),
        );
        for (offset, glyph) in [(1.0, "✕"), (2.0, "□"), (3.0, "–")] {
            painter.text(
                title_rect.right_center() - egui::vec2(offset * 34.0 * zoom.max(0.6) - 10.0, 0.0),
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional(13.0),
                theme::muted(),
            );
        }
        painter.rect_stroke(
            window,
            egui::CornerRadius::same(8),
            egui::Stroke::new(
                if self.window_selected && !self.preview_mode {
                    2.0
                } else {
                    1.0
                },
                if self.window_selected && !self.preview_mode {
                    theme::accent_hover()
                } else {
                    theme::line()
                },
            ),
            egui::StrokeKind::Inside,
        );
        let mut window_changed = false;
        if self.window_selected && !self.unsupported_xaml && !self.preview_mode {
            let handle = egui::Rect::from_min_size(
                window.right_bottom() - egui::vec2(9.0, 9.0),
                egui::vec2(18.0, 18.0),
            );
            let response = ui
                .interact(
                    handle,
                    ui.make_persistent_id("designer-window-resize"),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            painter.rect_filled(handle, egui::CornerRadius::same(3), theme::accent_hover());
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_motion() / zoom;
                self.window_width = (self.window_width + delta.x).clamp(320.0, 4000.0);
                self.window_height = (self.window_height + delta.y).clamp(240.0, 3000.0);
                window_changed = true;
            }
        }
        let body = egui::Rect::from_min_max(egui::pos2(window.min.x, title_rect.max.y), window.max);
        if !self.preview_mode {
            let step = 20.0 * zoom;
            let mut gx = body.min.x + step;
            while gx < body.max.x {
                let mut gy = body.min.y + step;
                while gy < body.max.y {
                    painter.circle_filled(egui::pos2(gx, gy), 0.8, theme::line());
                    gy += step;
                }
                gx += step;
            }
        }

        // Do not register a full-canvas click widget here: it overlaps every
        // control and can win egui's pointer interaction, preventing their
        // click-and-drag responses from starting. Detect empty-space clicks
        // from the pointer directly instead.
        let world_positions = control_world_positions(&self.controls);
        let parent_bounds = self
            .controls
            .iter()
            .enumerate()
            .map(|(index, _control)| {
                control_parent_bounds(&self.controls, index, self.window_width, self.window_height)
            })
            .collect::<Vec<_>>();
        let empty_canvas_clicked = ui.input(|input| {
            !self.preview_mode
                && input.pointer.button_clicked(egui::PointerButton::Primary)
                && input.pointer.interact_pos().is_some_and(|position| {
                    body.contains(position)
                        && !self.controls.iter().enumerate().any(|(index, control)| {
                            let rect = egui::Rect::from_min_size(
                                body.min
                                    + egui::vec2(
                                        world_positions[index].x,
                                        world_positions[index].y,
                                    ) * zoom,
                                egui::vec2(control.width, control.height) * zoom,
                            );
                            rect.contains(position)
                        })
                })
        });
        if empty_canvas_clicked {
            self.selected = None;
            self.window_selected = true;
        }

        let mut controls_moved = false;
        let mut open_control = None;
        for (index, control) in self.controls.iter_mut().enumerate() {
            if !control.is_visible {
                continue;
            }
            if !control.text.is_empty()
                && matches!(
                    control.kind,
                    ControlKind::Button
                        | ControlKind::Label
                        | ControlKind::CheckBox
                        | ControlKind::ToggleSwitch
                        | ControlKind::RadioButton
                )
            {
                let font_size = (control.font_size * zoom).max(8.0);
                let text_width = ui.fonts_mut(|fonts| {
                    fonts
                        .layout_no_wrap(
                            control.text.clone(),
                            egui::FontId::proportional(font_size),
                            theme::text(),
                        )
                        .size()
                        .x
                }) / zoom;
                let leading = match control.kind {
                    ControlKind::CheckBox | ControlKind::RadioButton => 36.0,
                    ControlKind::ToggleSwitch => 58.0,
                    _ => 24.0,
                };
                let min_width = (text_width + leading + control.padding * 2.0)
                    .min((parent_bounds[index].x - control.x).max(0.0));
                if control.width < min_width {
                    control.width = min_width;
                    controls_moved = true;
                }
            }
            let control_size = egui::vec2(control.width, control.height);
            let rect = egui::Rect::from_min_size(
                body.min + egui::vec2(world_positions[index].x, world_positions[index].y) * zoom,
                control_size * zoom,
            );
            let response = ui
                .interact(
                    rect,
                    ui.make_persistent_id(("designer-control", index)),
                    if self.unsupported_xaml || self.preview_mode {
                        egui::Sense::click()
                    } else {
                        egui::Sense::click_and_drag()
                    },
                )
                .on_hover_cursor(egui::CursorIcon::Grab);
            if response.double_clicked_by(egui::PointerButton::Primary) {
                open_control = Some(index);
                self.selected = Some(index);
                self.window_selected = false;
            } else if response.clicked_by(egui::PointerButton::Primary)
                || response.drag_started_by(egui::PointerButton::Primary)
            {
                self.selected = Some(index);
                self.window_selected = false;
            }
            drag_started |= response.drag_started_by(egui::PointerButton::Primary);
            if response.dragged_by(egui::PointerButton::Primary)
                && response.drag_motion() != egui::Vec2::ZERO
            {
                let delta = response.drag_motion() / zoom;
                let bounds = parent_bounds[index];
                control.x = (control.x + delta.x).clamp(0.0, (bounds.x - control_size.x).max(0.0));
                control.y = (control.y + delta.y).clamp(0.0, (bounds.y - control_size.y).max(0.0));
                controls_moved = true;
            }
            let selected = !self.preview_mode && self.selected == Some(index);
            let radius = egui::CornerRadius::same(4);
            let control_color = with_opacity(
                parse_color(&control.color).unwrap_or(theme::accent()),
                control.opacity,
            );
            match control.kind {
                ControlKind::Button => {
                    painter.rect_filled(rect, radius, control_color);
                }
                ControlKind::TextBox => {
                    painter.rect_filled(rect, radius, control_color);
                    painter.rect_stroke(
                        rect,
                        radius,
                        egui::Stroke::new(1.0, theme::line()),
                        egui::StrokeKind::Inside,
                    );
                }
                ControlKind::Label | ControlKind::Image | ControlKind::Frame => {}
                ControlKind::CheckBox | ControlKind::RadioButton => {}
                ControlKind::ToggleSwitch | ControlKind::Slider | ControlKind::ProgressBar => {
                    painter.rect_filled(rect, radius, theme::panel());
                }
                ControlKind::ComboBox
                | ControlKind::Calendar
                | ControlKind::CalendarDatePicker
                | ControlKind::DatePicker
                | ControlKind::NumericUpDown => {
                    painter.rect_filled(rect, radius, theme::panel());
                    painter.rect_stroke(
                        rect,
                        radius,
                        egui::Stroke::new(1.0, theme::line()),
                        egui::StrokeKind::Inside,
                    );
                }
                ControlKind::Separator => {
                    painter.line_segment(
                        [rect.left_center(), rect.right_center()],
                        egui::Stroke::new(1.0, theme::line()),
                    );
                }
                ControlKind::Extra(_) => {
                    paint_extra_preview(
                        &painter,
                        control,
                        rect,
                        control_color,
                        zoom,
                        !self.preview_mode,
                    );
                }
            }
            let accent = control_color;
            match control.kind {
                ControlKind::CheckBox => {
                    let box_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(4.0, (rect.height() - 18.0) * 0.5),
                        egui::vec2(18.0, 18.0),
                    );
                    painter.rect_stroke(
                        box_rect,
                        egui::CornerRadius::same(3),
                        egui::Stroke::new(1.5, accent),
                        egui::StrokeKind::Inside,
                    );
                }
                ControlKind::RadioButton => {
                    let center = egui::pos2(rect.left() + 13.0, rect.center().y);
                    painter.circle_stroke(center, 9.0, egui::Stroke::new(1.5, accent));
                }
                ControlKind::ToggleSwitch => {
                    let track = egui::Rect::from_min_size(
                        rect.min + egui::vec2(5.0, (rect.height() - 20.0) * 0.5),
                        egui::vec2(38.0, 20.0),
                    );
                    painter.rect_filled(track, egui::CornerRadius::same(10), accent);
                    painter.circle_filled(
                        egui::pos2(track.right() - 10.0, track.center().y),
                        7.0,
                        Color32::WHITE,
                    );
                }
                ControlKind::Slider => {
                    let track = egui::Rect::from_center_size(
                        rect.center() + egui::vec2(0.0, 5.0),
                        egui::vec2((rect.width() - 20.0).max(24.0), 4.0),
                    );
                    painter.rect_filled(track, egui::CornerRadius::same(2), theme::line());
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            track.min,
                            egui::pos2(track.center().x, track.max.y),
                        ),
                        egui::CornerRadius::same(2),
                        accent,
                    );
                    painter.circle_filled(
                        egui::pos2(track.center().x, track.center().y),
                        7.0,
                        accent,
                    );
                }
                ControlKind::ProgressBar => {
                    let fill = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.left() + rect.width() * 0.62, rect.bottom()),
                    );
                    painter.rect_filled(fill, radius, accent);
                }
                ControlKind::ComboBox => {
                    painter.text(
                        egui::pos2(rect.right() - 14.0, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        "▾",
                        egui::FontId::proportional(14.0),
                        theme::muted(),
                    );
                }
                ControlKind::CalendarDatePicker | ControlKind::DatePicker => {
                    painter.text(
                        egui::pos2(rect.right() - 18.0, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        "▦",
                        egui::FontId::proportional(15.0),
                        theme::muted(),
                    );
                }
                ControlKind::NumericUpDown => {
                    let button_area = egui::Rect::from_min_max(
                        egui::pos2(rect.right() - 26.0, rect.top()),
                        rect.right_bottom(),
                    );
                    painter.line_segment(
                        [button_area.left_top(), button_area.left_bottom()],
                        egui::Stroke::new(1.0, theme::line()),
                    );
                    painter.text(
                        egui::pos2(button_area.center().x, button_area.center().y - 6.0),
                        egui::Align2::CENTER_CENTER,
                        "▴",
                        egui::FontId::proportional(10.0),
                        theme::muted(),
                    );
                    painter.text(
                        egui::pos2(button_area.center().x, button_area.center().y + 6.0),
                        egui::Align2::CENTER_CENTER,
                        "▾",
                        egui::FontId::proportional(10.0),
                        theme::muted(),
                    );
                }
                ControlKind::Image => {
                    painter.rect_stroke(
                        rect,
                        4.0,
                        egui::Stroke::new(1.0, theme::line()),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if control.text.is_empty() {
                            "▧  Image"
                        } else {
                            "▧  Preview"
                        },
                        egui::FontId::proportional(14.0),
                        theme::muted(),
                    );
                }
                ControlKind::Frame => {
                    painter.rect_stroke(
                        rect,
                        radius,
                        egui::Stroke::new(2.0, control_color),
                        egui::StrokeKind::Inside,
                    );
                }
                ControlKind::Calendar => {
                    let header = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.right(), rect.top() + 28.0 * zoom),
                    );
                    painter.rect_filled(header, radius, theme::accent());
                    painter.text(
                        header.center(),
                        egui::Align2::CENTER_CENTER,
                        "Calendar",
                        egui::FontId::proportional((13.0 * zoom).max(9.0)),
                        Color32::WHITE,
                    );
                    let grid_top = header.bottom() + 10.0 * zoom;
                    let cell_w = rect.width() / 7.0;
                    let cell_h = ((rect.bottom() - grid_top) / 5.0).max(1.0);
                    for day in 1..=31 {
                        let column = (day - 1) % 7;
                        let row = (day - 1) / 7;
                        let center = egui::pos2(
                            rect.left() + (column as f32 + 0.5) * cell_w,
                            grid_top + (row as f32 + 0.5) * cell_h,
                        );
                        painter.text(
                            center,
                            egui::Align2::CENTER_CENTER,
                            day.to_string(),
                            egui::FontId::proportional((11.0 * zoom).max(8.0)),
                            theme::text(),
                        );
                    }
                }
                _ => {}
            }
            let (text, color, align_x) = match control.kind {
                ControlKind::TextBox if control.text.is_empty() => {
                    ("TextBox", theme::muted(), true)
                }
                ControlKind::TextBox => (control.text.as_str(), theme::text(), true),
                ControlKind::Label => (
                    control.text.as_str(),
                    with_opacity(
                        parse_color(&control.color).unwrap_or(theme::text()),
                        control.opacity,
                    ),
                    true,
                ),
                ControlKind::Button => (control.text.as_str(), Color32::WHITE, false),
                ControlKind::CheckBox | ControlKind::ToggleSwitch | ControlKind::RadioButton => (
                    control.text.as_str(),
                    with_opacity(
                        parse_color(&control.color).unwrap_or(theme::text()),
                        control.opacity,
                    ),
                    true,
                ),
                ControlKind::Slider => (control.text.as_str(), theme::muted(), true),
                ControlKind::ProgressBar | ControlKind::Separator => ("", theme::text(), true),
                ControlKind::ComboBox => (
                    control.text.lines().next().unwrap_or("Choose…"),
                    theme::text(),
                    true,
                ),
                ControlKind::Calendar => ("", theme::text(), true),
                ControlKind::Image => ("", theme::text(), true),
                ControlKind::Frame => ("", theme::text(), true),
                ControlKind::CalendarDatePicker | ControlKind::DatePicker => (
                    if control.text.is_empty() {
                        "Select date…"
                    } else {
                        &control.text
                    },
                    theme::text(),
                    true,
                ),
                ControlKind::NumericUpDown => (
                    if control.text.is_empty() {
                        "0"
                    } else {
                        &control.text
                    },
                    theme::text(),
                    true,
                ),
                ControlKind::Extra(index) => {
                    extra_preview_caption(index, control.text.as_str(), control_color)
                }
            };
            let text_offset = match control.kind {
                ControlKind::CheckBox | ControlKind::RadioButton => 29.0,
                ControlKind::ToggleSwitch => 50.0,
                ControlKind::Slider => 0.0,
                _ => 8.0,
            };
            let font = egui::FontId::proportional((control.font_size * zoom).max(8.0));
            if align_x {
                painter.text(
                    rect.left_center() + egui::vec2(text_offset * zoom, 0.0),
                    egui::Align2::LEFT_CENTER,
                    text,
                    font,
                    color,
                );
            } else {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    font,
                    color,
                );
            }
            if selected {
                painter.rect_stroke(
                    rect.expand(2.0),
                    radius,
                    egui::Stroke::new(1.5, theme::accent_hover()),
                    egui::StrokeKind::Outside,
                );
                let handle =
                    egui::Rect::from_center_size(rect.right_bottom(), egui::vec2(11.0, 11.0));
                let resize = ui
                    .interact(
                        handle,
                        ui.make_persistent_id(("designer-resize", index)),
                        if self.unsupported_xaml {
                            egui::Sense::hover()
                        } else {
                            egui::Sense::drag()
                        },
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
                drag_started |= resize.drag_started_by(egui::PointerButton::Primary);
                painter.rect_filled(handle, egui::CornerRadius::same(2), Color32::WHITE);
                if resize.dragged_by(egui::PointerButton::Primary)
                    && resize.drag_motion() != egui::Vec2::ZERO
                {
                    let delta = resize.drag_motion() / zoom;
                    let bounds = parent_bounds[index];
                    control.width =
                        (control.width + delta.x).clamp(24.0, (bounds.x - control.x).max(24.0));
                    control.height =
                        (control.height + delta.y).clamp(20.0, (bounds.y - control.y).max(20.0));
                    controls_moved = true;
                }
            }
        }
        if let Some(index) = open_control {
            self.open_control_code(index, project);
        }
        if drag_started {
            self.record_snapshot(controls_before_drag);
        }
        if !self.preview_mode {
            painter.text(
                area.right_bottom() - egui::vec2(12.0, 10.0),
                egui::Align2::RIGHT_BOTTOM,
                format!(
                    "{:.0} × {:.0}   {:.0}%",
                    self.window_width,
                    self.window_height,
                    zoom * 100.0
                ),
                egui::FontId::proportional(12.0),
                theme::muted(),
            );
        }
        // Draw toolbar widgets after every canvas shape so the window surface
        // cannot paint over the controls.
        if ui
            .put(
                egui::Rect::from_min_size(
                    area.left_top() + egui::vec2(158.0, 6.0),
                    egui::vec2(76.0, 24.0),
                ),
                egui::Button::new(if self.preview_mode {
                    "Design"
                } else {
                    "Preview"
                }),
            )
            .clicked()
        {
            self.preview_mode = !self.preview_mode;
        }
        if !self.preview_mode {
            if ui
                .put(
                    egui::Rect::from_min_size(
                        area.left_top() + egui::vec2(8.0, 6.0),
                        egui::vec2(32.0, 24.0),
                    ),
                    egui::Button::new("−"),
                )
                .clicked()
            {
                factor = (factor - 0.1).clamp(0.5, 2.0);
            }
            if ui
                .put(
                    egui::Rect::from_min_size(
                        area.left_top() + egui::vec2(44.0, 6.0),
                        egui::vec2(32.0, 24.0),
                    ),
                    egui::Button::new("+"),
                )
                .clicked()
            {
                factor = (factor + 0.1).clamp(0.5, 2.0);
            }
            if ui
                .put(
                    egui::Rect::from_min_size(
                        area.left_top() + egui::vec2(80.0, 6.0),
                        egui::vec2(72.0, 24.0),
                    ),
                    egui::Button::new("Reset").truncate(),
                )
                .clicked()
            {
                factor = 1.0;
                self.pan = egui::Vec2::ZERO;
            }
        }
        self.zoom_factor = Some(factor);
        if controls_moved || window_changed {
            self.save_project(project);
        }
    }

    fn save_project(&mut self, project: &ProjectInfo) {
        if project.template == ProjectTemplate::QtWidgetsApp {
            if let Err(error) = save_qt_project(project, &self.controls) {
                self.error = Some(error);
            } else {
                self.error = None;
            }
            return;
        }
        for control in &mut self.controls {
            if matches!(control.kind, ControlKind::Extra(index)
                if extra_control(index).is_some_and(|item| item.0 == "Menu bar"))
            {
                control.x = 0.0;
                control.y = 0.0;
                control.width = self.window_width;
            }
        }
        let path = project.path.join("MainWindow.axaml");
        let existing = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) => {
                self.error = Some(format!(
                    "Could not read designer file '{}': {error}",
                    path.display()
                ));
                return;
            }
        };
        if self.unsupported_xaml {
            self.error = Some("This XAML is read-only in the designer because it contains unsupported controls or properties. No changes were written.".to_owned());
            return;
        }
        if self.source_xaml.as_deref() != Some(existing.as_str()) {
            self.error = Some("MainWindow.axaml changed outside the designer. Reload it in Code view, then reopen the designer; the designer did not overwrite the newer file.".to_owned());
            return;
        }
        let contents = preserve_root_attributes(
            &existing,
            &apply_window_settings(
                &render_xaml(&project.name, &self.controls),
                &self.window_title,
                self.window_width,
                self.window_height,
                &self.window_icon,
                self.auto_resize_content,
            ),
        );
        self.pending_xaml =
            (self.source_xaml.as_deref() != Some(contents.as_str())).then_some(contents);
        self.error = None;
    }

    pub(super) fn current_xaml(&self, project: &ProjectInfo) -> String {
        self.pending_xaml
            .clone()
            .or_else(|| self.source_xaml.clone())
            .unwrap_or_else(|| render_xaml(&project.name, &self.controls))
    }

    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.pending_xaml.is_some()
    }

    pub(super) fn take_open_code_handler(&mut self) -> Option<String> {
        self.open_code_handler.take()
    }

    fn open_control_code(&mut self, index: usize, project: &ProjectInfo) {
        let Some(control) = self.controls.get(index) else {
            return;
        };
        if project.template == ProjectTemplate::QtWidgetsApp {
            if control.kind == ControlKind::Button {
                if let Err(error) = ensure_control_handler(project, &control.name) {
                    self.error = Some(error);
                }
            }
            self.open_code = Some(PathBuf::from("src/MainWindow.cpp"));
            return;
        }
        let event = event_for_control(control.kind);
        let event_name = control
            .event_name
            .clone()
            .unwrap_or_else(|| event.to_owned());
        let handler_name = format!("{}_{}", control.name, event_name);
        // The user may have assigned a non-default event in the Events panel
        // (for example DatePicker.Loaded). Generate the delegate signature for
        // that selected event, not for the control's default event.
        match ensure_avalonia_event_handler(project, &handler_name, &event_name) {
            Ok(source) => {
                if let Some(control) = self.controls.get_mut(index) {
                    control.event_name = Some(event_name);
                }
                self.open_code_handler = Some(source);
                self.save_project(project);
                self.open_code = Some(PathBuf::from("MainWindow.axaml.cs"));
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn paint_control_drag_preview(ctx: &egui::Context, controls: &[DesignerControl]) {
    let label = if let Some(kind) = egui::DragAndDrop::payload::<ControlKind>(ctx) {
        format!("＋  {}", kind.title())
    } else if let Some(index) = egui::DragAndDrop::payload::<usize>(ctx) {
        let Some(control) = controls.get(*index) else {
            return;
        };
        control.name.clone()
    } else {
        return;
    };
    let Some(pointer) = ctx.pointer_interact_pos() else {
        return;
    };
    ctx.request_repaint();
    let galley = ctx.fonts_mut(|fonts| {
        fonts.layout_no_wrap(label, egui::FontId::proportional(14.0), Color32::WHITE)
    });
    let size = galley.size() + egui::vec2(24.0, 14.0);
    let rect = egui::Rect::from_center_size(pointer + egui::vec2(22.0, 14.0), size);
    let angle = (0.035 + ctx.input(|input| input.pointer.delta().x * 0.002)).clamp(-0.07, 0.07);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::unique("toolbox-drag-preview"),
    ));
    painter.add(egui::Shape::Rect(
        egui::epaint::RectShape::filled(
            rect.translate(egui::vec2(1.0, 3.0)),
            egui::CornerRadius::same(5),
            Color32::from_black_alpha(100),
        )
        .with_angle(angle),
    ));
    painter.add(egui::Shape::Rect(
        egui::epaint::RectShape::new(
            rect,
            egui::CornerRadius::same(5),
            theme::panel(),
            egui::Stroke::new(1.5, theme::accent_hover()),
            egui::StrokeKind::Inside,
        )
        .with_angle(angle),
    ));
    let mut text =
        egui::epaint::TextShape::new(rect.center() - galley.size() * 0.5, galley, Color32::WHITE);
    text.angle = angle;
    painter.add(text);
}

fn outline_row(
    ui: &mut egui::Ui,
    depth: usize,
    open: Option<bool>,
    tag: &str,
    name: &str,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.0), 24.0),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    if selected {
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(3),
            Color32::from_rgb(58, 52, 120),
        );
    } else if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(3), theme::hover());
    }
    for level in 0..depth {
        let x = rect.left() + 8.0 + level as f32 * 14.0;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, theme::line()),
        );
    }
    let mut x = rect.left() + 4.0 + depth as f32 * 14.0;
    if let Some(is_open) = open {
        painter.text(
            egui::pos2(x + 5.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            if is_open { "▾" } else { "▸" },
            egui::FontId::proportional(12.0),
            theme::muted(),
        );
    }
    x += 16.0;
    // Keep the familiar six-dot grip visible while making the whole row
    // draggable for dropping controls onto Frames.
    for row in 0..3 {
        for column in 0..2 {
            painter.circle_filled(
                egui::pos2(
                    x + column as f32 * 4.0,
                    rect.center().y - 4.0 + row as f32 * 4.0,
                ),
                1.1,
                if response.dragged() {
                    theme::accent_hover()
                } else {
                    theme::muted()
                },
            );
        }
    }
    x += 14.0;
    let _ = tag;
    let name_x = x + 4.0;
    let available_width = (rect.right() - name_x - 4.0).max(1.0);
    let name_font = egui::FontId::proportional(13.0);
    let mut shown_name = name.to_owned();
    loop {
        let galley = painter.layout_no_wrap(shown_name.clone(), name_font.clone(), theme::muted());
        if galley.size().x <= available_width || shown_name.is_empty() {
            painter.galley(
                egui::pos2(name_x, rect.center().y - galley.size().y / 2.0),
                galley,
                theme::muted(),
            );
            break;
        }
        if shown_name.ends_with('…') {
            shown_name.pop();
        } else if !shown_name.is_empty() {
            shown_name.pop();
            shown_name.push('…');
        }
    }
    let is_dragged = response.dragged();
    let response = response.on_hover_cursor(if is_dragged {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    });
    if is_dragged {
        paint_outline_drag_ghost(ui, name);
    }
    response
}

fn paint_outline_drag_ghost(ui: &egui::Ui, name: &str) {
    let Some(pointer) = ui.ctx().pointer_interact_pos() else {
        return;
    };
    let time = ui.input(|input| input.time) as f32;
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(16));
    let angle = (time * 8.0).sin() * 0.025 - 0.025;
    let rect = egui::Rect::from_min_size(pointer + egui::vec2(14.0, 12.0), egui::vec2(178.0, 30.0));
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::unique("designer-outline-drag-ghost"),
    ));
    painter.add(egui::Shape::Rect(
        egui::epaint::RectShape::new(
            rect,
            egui::CornerRadius::same(5),
            Color32::from_rgba_unmultiplied(49, 44, 92, 225),
            egui::Stroke::new(1.0, theme::accent_hover()),
            egui::StrokeKind::Inside,
        )
        .with_angle(angle),
    ));
    painter.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("⠿  {name}"),
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
}

fn outline_order(controls: &[DesignerControl]) -> Vec<(usize, usize)> {
    fn visit(
        controls: &[DesignerControl],
        parent: Option<&str>,
        depth: usize,
        visited: &mut Vec<bool>,
        output: &mut Vec<(usize, usize)>,
    ) {
        for (index, control) in controls.iter().enumerate() {
            if !visited[index] && control.parent.as_deref() == parent {
                visited[index] = true;
                output.push((index, depth));
                if can_contain_controls(control.kind) {
                    visit(controls, Some(&control.name), depth + 1, visited, output);
                }
            }
        }
    }

    let mut output = Vec::with_capacity(controls.len());
    let mut visited = vec![false; controls.len()];
    visit(controls, None, 0, &mut visited, &mut output);
    // Recover gracefully from stale parent names in older or hand-edited XAML.
    for index in 0..controls.len() {
        if !visited[index] {
            output.push((index, 0));
        }
    }
    output
}

fn normalize_control_order(controls: &mut Vec<DesignerControl>) {
    let order = outline_order(controls)
        .into_iter()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let old = std::mem::take(controls);
    *controls = order
        .into_iter()
        .filter_map(|index| old.get(index).cloned())
        .collect();
}

fn would_create_parent_cycle(controls: &[DesignerControl], child: usize, parent: usize) -> bool {
    let Some(child_name) = controls.get(child).map(|control| control.name.as_str()) else {
        return true;
    };
    let Some(parent_control) = controls.get(parent) else {
        return true;
    };
    if !can_contain_controls(parent_control.kind) {
        return true;
    }
    let mut ancestor = Some(parent_control);
    while let Some(control) = ancestor {
        if control.name == child_name {
            return true;
        }
        ancestor = control
            .parent
            .as_deref()
            .and_then(|name| controls.iter().find(|candidate| candidate.name == name));
    }
    false
}

fn can_contain_controls(kind: ControlKind) -> bool {
    matches!(
        kind.tag(),
        "Border"
            | "Grid"
            | "StackPanel"
            | "WrapPanel"
            | "DockPanel"
            | "ScrollViewer"
            | "Expander"
            | "ContentControl"
    )
}

fn control_world_positions(controls: &[DesignerControl]) -> Vec<egui::Pos2> {
    let mut positions = Vec::with_capacity(controls.len());
    for (index, control) in controls.iter().enumerate() {
        let mut position = egui::pos2(control.x, control.y);
        let mut parent = control.parent.as_deref();
        let mut remaining = controls.len();
        while let Some(parent_name) = parent.filter(|_| remaining > 0) {
            remaining -= 1;
            let Some(parent_control) = controls.iter().find(|item| item.name == parent_name) else {
                break;
            };
            position += egui::vec2(parent_control.x, parent_control.y);
            parent = parent_control.parent.as_deref();
        }
        if index >= positions.len() {
            positions.push(position);
        }
    }
    positions
}

fn control_parent_bounds(
    controls: &[DesignerControl],
    index: usize,
    window_width: f32,
    window_height: f32,
) -> egui::Vec2 {
    controls
        .get(index)
        .and_then(|control| control.parent.as_deref())
        .and_then(|name| controls.iter().find(|candidate| candidate.name == name))
        .filter(|parent| can_contain_controls(parent.kind))
        .map(|parent| egui::vec2(parent.width, parent.height))
        .unwrap_or(egui::vec2(window_width, window_height - TITLE_H))
}

fn parse_hierarchical_items(text: &str) -> Vec<(String, Vec<String>)> {
    let mut items: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let trimmed = line.trim();
        if line.chars().next().is_some_and(char::is_whitespace) {
            if let Some((_, children)) = items.last_mut() {
                children.push(trimmed.to_owned());
            }
        } else {
            items.push((trimmed.to_owned(), Vec::new()));
        }
    }
    items
}

fn hierarchy_tree_editor(ui: &mut egui::Ui, text: &mut String, identity: &str) -> bool {
    type DragItem = (usize, usize);
    const ROOT: usize = usize::MAX;
    enum Action {
        AddRoot,
        AddChild(usize),
        RemoveRoot(usize),
        RemoveChild(usize, usize),
        Move(DragItem, DragItem),
    }

    let mut items = parse_hierarchical_items(text);
    let mut action = None;
    let mut edited = false;
    let selection_id = egui::Id::unique(("designer-hierarchy-selection", identity));
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<DragItem>(selection_id))
        .unwrap_or((ROOT, 0));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Menu tree").strong());
        if ui.small_button("+ Category").clicked() {
            action = Some(Action::AddRoot);
        }
        if ui.small_button("+ Item").clicked() {
            action = Some(Action::AddChild(items.len().saturating_sub(1)));
        }
    });
    if items.is_empty() {
        ui.label(
            RichText::new("Add a category, then add items beneath it.")
                .small()
                .color(theme::muted()),
        );
    }

    for (root_index, (category, children)) in items.iter().enumerate() {
        let root_row = ui
            .horizontal(|ui| {
                let drag = ui
                    .add(
                        egui::Label::new(RichText::new("⠿").color(theme::accent()))
                            .sense(egui::Sense::drag()),
                    )
                    .on_hover_cursor(egui::CursorIcon::Grab)
                    .on_hover_text("Drag this category to reorder it");
                drag.dnd_set_drag_payload((ROOT, root_index));
                if ui
                    .selectable_label(selected == (ROOT, root_index), format!("▾ {category}"))
                    .clicked()
                {
                    selected = (ROOT, root_index);
                }
                if ui.small_button("+ Item").clicked() {
                    action = Some(Action::AddChild(root_index));
                }
                if ui
                    .small_button("×")
                    .on_hover_text("Remove category")
                    .clicked()
                {
                    action = Some(Action::RemoveRoot(root_index));
                }
            })
            .response;
        let root_hovering = root_row.dnd_hover_payload::<DragItem>().is_some();
        if root_hovering {
            ui.painter().rect_stroke(
                root_row.rect,
                egui::CornerRadius::same(3),
                egui::Stroke::new(1.5, theme::accent_hover()),
                egui::StrokeKind::Outside,
            );
        }
        if root_hovering && let Some(source) = root_row.dnd_release_payload::<DragItem>() {
            action = Some(Action::Move(*source, (ROOT, root_index)));
        }
        for (child_index, child) in children.iter().enumerate() {
            let child_row = ui
                .horizontal(|ui| {
                    ui.add_space(22.0);
                    let drag = ui
                        .add(
                            egui::Label::new(RichText::new("⠿").color(theme::muted()))
                                .sense(egui::Sense::drag()),
                        )
                        .on_hover_cursor(egui::CursorIcon::Grab)
                        .on_hover_text("Drag this item to another category or position");
                    drag.dnd_set_drag_payload((root_index, child_index));
                    if ui
                        .selectable_label(
                            selected == (root_index, child_index),
                            format!("↳ {child}"),
                        )
                        .clicked()
                    {
                        selected = (root_index, child_index);
                    }
                    if ui.small_button("×").on_hover_text("Remove item").clicked() {
                        action = Some(Action::RemoveChild(root_index, child_index));
                    }
                })
                .response;
            let child_hovering = child_row.dnd_hover_payload::<DragItem>().is_some();
            if child_hovering {
                ui.painter().rect_stroke(
                    child_row.rect,
                    egui::CornerRadius::same(3),
                    egui::Stroke::new(1.5, theme::accent_hover()),
                    egui::StrokeKind::Outside,
                );
            }
            if child_hovering && let Some(source) = child_row.dnd_release_payload::<DragItem>() {
                action = Some(Action::Move(*source, (root_index, child_index)));
            }
        }
        ui.painter().line_segment(
            [root_row.rect.left_bottom(), root_row.rect.right_bottom()],
            egui::Stroke::new(1.0, theme::line()),
        );
    }

    if selected.0 == ROOT {
        if let Some((category, _)) = items.get_mut(selected.1) {
            ui.horizontal(|ui| {
                ui.label("Selected category");
                edited |= ui
                    .add_sized(
                        [ui.available_width(), 26.0],
                        egui::TextEdit::singleline(category),
                    )
                    .changed();
            });
        }
    } else if let Some((_, children)) = items.get_mut(selected.0)
        && let Some(item) = children.get_mut(selected.1)
    {
        ui.horizontal(|ui| {
            ui.label("Selected item");
            edited |= ui
                .add_sized(
                    [ui.available_width(), 26.0],
                    egui::TextEdit::singleline(item),
                )
                .changed();
        });
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selected));

    let did_change = action.is_some();
    match action {
        Some(Action::AddRoot) => {
            items.push((format!("Menu {}", items.len() + 1), Vec::new()));
            selected = (ROOT, items.len() - 1);
        }
        Some(Action::AddChild(index)) => {
            if items.is_empty() {
                items.push(("File".to_owned(), vec!["New item".to_owned()]));
                selected = (0, 0);
            } else if let Some((_, children)) = items.get_mut(index) {
                children.push("New item".to_owned());
                selected = (index, children.len() - 1);
            }
        }
        Some(Action::RemoveRoot(index)) => {
            if index < items.len() {
                items.remove(index);
                selected = (ROOT, index.min(items.len().saturating_sub(1)));
            }
        }
        Some(Action::RemoveChild(parent, child)) => {
            if let Some((_, children)) = items.get_mut(parent)
                && child < children.len()
            {
                children.remove(child);
                selected = (parent, child.min(children.len().saturating_sub(1)));
            }
        }
        Some(Action::Move((source_parent, source_item), (target_parent, target_item))) => {
            if source_parent == ROOT && target_parent == ROOT {
                if source_item < items.len()
                    && target_item < items.len()
                    && source_item != target_item
                {
                    let item = items.remove(source_item);
                    items.insert(target_item.min(items.len()), item);
                }
            } else if source_parent != ROOT && target_parent == ROOT {
                if let Some((_, children)) = items.get_mut(source_parent)
                    && source_item < children.len()
                {
                    let child = children.remove(source_item);
                    if let Some((_, target_children)) = items.get_mut(target_item) {
                        target_children.push(child);
                    }
                }
            } else if source_parent != ROOT && target_parent != ROOT {
                let moved = items.get_mut(source_parent).and_then(|(_, children)| {
                    (source_item < children.len()).then(|| children.remove(source_item))
                });
                if let Some(child) = moved
                    && let Some((_, target_children)) = items.get_mut(target_parent)
                {
                    let insertion = target_item.min(target_children.len());
                    target_children.insert(insertion, child);
                }
            }
        }
        None => {}
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selected));
    if did_change || edited {
        *text = items
            .iter()
            .flat_map(|(category, children)| {
                std::iter::once(category.clone())
                    .chain(children.iter().map(|child| format!("  {child}")))
            })
            .collect::<Vec<_>>()
            .join("\n");
        true
    } else {
        false
    }
}

fn flat_collection_editor(
    ui: &mut egui::Ui,
    text: &mut String,
    identity: &str,
    item_label: &str,
) -> bool {
    let mut items = text
        .lines()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let selection_id = egui::Id::unique(("designer-collection-selection", identity));
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(selection_id))
        .unwrap_or(0);
    let mut changed = false;
    let mut action = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.strong(format!("{item_label}s"));
            if ui.small_button("+ Add").clicked() {
                action = Some(true);
            }
            if ui.small_button("Remove").clicked() && !items.is_empty() {
                action = Some(false);
            }
        });
        egui::ScrollArea::vertical()
            .max_height(112.0)
            .show(ui, |ui| {
                for (index, item) in items.iter().enumerate() {
                    if ui.selectable_label(selected == index, item).clicked() {
                        selected = index;
                    }
                }
            });
        if let Some(item) = items.get_mut(selected) {
            ui.horizontal(|ui| {
                ui.label("Name");
                changed |= ui
                    .add_sized(
                        [ui.available_width(), 26.0],
                        egui::TextEdit::singleline(item),
                    )
                    .changed();
            });
        }
    });
    match action {
        Some(true) => {
            items.push(format!("{item_label} {}", items.len() + 1));
            selected = items.len() - 1;
            changed = true;
        }
        Some(false) => {
            if !items.is_empty() {
                items.remove(selected.min(items.len() - 1));
                selected = selected.min(items.len().saturating_sub(1));
                changed = true;
            }
        }
        None => {}
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selected));
    if changed {
        *text = items.join("\n");
    }
    changed
}

fn render_xaml(project_name: &str, controls: &[DesignerControl]) -> String {
    let (surface_width, surface_height) = design_surface_size(controls);
    let mut xaml = format!(
        "<Window xmlns=\"https://github.com/avaloniaui\"\n        xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"\n        x:Class=\"CairnApp.MainWindow\"\n        Title=\"{}\" Width=\"1100\" Height=\"760\" MinWidth=\"480\" MinHeight=\"360\">\n  <Viewbox Stretch=\"Uniform\">\n    <Canvas Width=\"{surface_width:.0}\" Height=\"{surface_height:.0}\" Background=\"#FF1E1E1E\">\n",
        xml_escape(project_name)
    );
    let mut control_blocks = Vec::with_capacity(controls.len());
    for control in controls {
        let name = xml_escape(&control.name);
        let text = xml_escape(&control.text);
        let color = xml_escape(&control.color);
        let font_family = xml_escape(&control.font_family);
        // Avalonia's Fluent ToggleSwitch template places its label and switch
        // track on separate rows. Existing controls may still carry the old
        // explicit 48px height, which clips the track even with MinHeight set.
        let height = if control.kind == ControlKind::ToggleSwitch {
            control.height.max(64.0)
        } else if control.kind == ControlKind::Slider {
            control.height.max(56.0)
        } else if control.kind == ControlKind::Calendar {
            control.height.max(400.0)
        } else if control.kind == ControlKind::DatePicker {
            control.height.max(48.0)
        } else if control.kind == ControlKind::CalendarDatePicker {
            control.height.max(44.0)
        } else {
            control.height
        };
        let element = match control.kind {
            ControlKind::Button => format!(
                "    <Button x:Name=\"{name}\" Content=\"{text}\" Width=\"{:.0}\" Height=\"{:.0}\" FontSize=\"{:.0}\" Margin=\"{:.0}\" Background=\"{color}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.font_size, control.margin, control.x, control.y
            ),
            ControlKind::Label => format!(
                "    <TextBlock x:Name=\"{name}\" Text=\"{text}\" Width=\"{:.0}\" Height=\"{:.0}\" FontSize=\"{:.0}\" Margin=\"{:.0}\" Foreground=\"{color}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.font_size, control.margin, control.x, control.y
            ),
            ControlKind::TextBox => format!(
                "    <TextBox x:Name=\"{name}\" Watermark=\"{text}\" Width=\"{:.0}\" Height=\"{:.0}\" FontSize=\"{:.0}\" Margin=\"{:.0}\" Background=\"{color}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.font_size, control.margin, control.x, control.y
            ),
            ControlKind::CheckBox => format!(
                "    <CheckBox x:Name=\"{name}\" Content=\"{text}\" Foreground=\"{color}\" Width=\"{:.0}\" Height=\"{:.0}\" FontSize=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.font_size, control.margin, control.x, control.y
            ),
            ControlKind::ToggleSwitch => format!(
                "    <ToggleSwitch x:Name=\"{name}\" Content=\"{text}\" Foreground=\"{color}\" IsChecked=\"True\" Width=\"{:.0}\" Height=\"{:.0}\" MinHeight=\"56\" FontSize=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.font_size, control.margin, control.x, control.y
            ),
            ControlKind::Slider => format!(
                "    <Slider x:Name=\"{name}\" Minimum=\"0\" Maximum=\"100\" Value=\"50\" Foreground=\"{color}\" Width=\"{:.0}\" Height=\"{:.0}\" MinHeight=\"48\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.margin, control.x, control.y
            ),
            ControlKind::ProgressBar => format!(
                "    <ProgressBar x:Name=\"{name}\" Minimum=\"0\" Maximum=\"100\" Value=\"62\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.margin, control.x, control.y
            ),
            ControlKind::RadioButton => format!(
                "    <RadioButton x:Name=\"{name}\" Content=\"{text}\" Foreground=\"{color}\" Width=\"{:.0}\" Height=\"{:.0}\" FontSize=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.font_size, control.margin, control.x, control.y
            ),
            ControlKind::Separator => format!(
                "    <Border x:Name=\"{name}\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Background=\"{color}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.margin, control.x, control.y
            ),
            ControlKind::ComboBox => format!(
                "    <ComboBox x:Name=\"{name}\" SelectedIndex=\"0\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.margin, control.x, control.y
            ),
            ControlKind::Calendar => format!(
                "    <Calendar x:Name=\"{name}\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width.max(340.0),
                height,
                control.margin,
                control.x,
                control.y
            ),
            ControlKind::CalendarDatePicker => format!(
                "    <CalendarDatePicker x:Name=\"{name}\" Watermark=\"{text}\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width.max(240.0),
                height,
                control.margin,
                control.x,
                control.y
            ),
            ControlKind::DatePicker => format!(
                "    <DatePicker x:Name=\"{name}\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width.max(360.0),
                height,
                control.margin,
                control.x,
                control.y
            ),
            ControlKind::NumericUpDown => {
                let value = control
                    .text
                    .parse::<f64>()
                    .ok()
                    .filter(|value| (0.0..=100.0).contains(value))
                    .unwrap_or(5.0);
                format!(
                    "    <NumericUpDown x:Name=\"{name}\" Value=\"{value}\" Minimum=\"0\" Maximum=\"100\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                    control.width, height, control.margin, control.x, control.y
                )
            }
            ControlKind::Image => format!(
                "    <Image x:Name=\"{name}\" Source=\"{text}\" Stretch=\"Uniform\" Width=\"{:.0}\" Height=\"{:.0}\" Margin=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, height, control.margin, control.x, control.y
            ),
            ControlKind::Frame => format!(
                "    <Border x:Name=\"{name}\" Background=\"Transparent\" BorderBrush=\"{color}\" BorderThickness=\"2\" CornerRadius=\"4\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                frame_content_size(control, controls).x,
                frame_content_size(control, controls).y,
                control.x,
                control.y
            ),
            ControlKind::Extra(index) => {
                let tag = extra_control(index).map_or("Border", |item| item.1);
                let text_attribute = match tag {
                    "ToggleButton" | "DropDownButton" | "ContentControl" => {
                        format!(" Content=\"{text}\"")
                    }
                    "Expander" => format!(" Header=\"{text}\""),
                    "SplitButton" => format!(
                        " Content=\"{}\"",
                        xml_escape(text.lines().next().unwrap_or("Main action"))
                    ),
                    "MenuItem" | "TabItem" => format!(" Header=\"{text}\""),
                    "ListBox" => " SelectedIndex=\"0\"".to_owned(),
                    "TabControl" => " SelectedIndex=\"0\"".to_owned(),
                    "TextBox" | "AutoCompleteBox" => format!(" Watermark=\"{text}\""),
                    _ => String::new(),
                };
                let color_attribute = extra_color_property_name(control.kind)
                    .filter(|property| {
                        !(*property == "Background"
                            && uses_default_theme_surface(control.kind, &control.color))
                    })
                    .map(|property| format!(" {property}=\"{}\"", xml_escape(&control.color)))
                    .unwrap_or_default();
                let polygon_points = if tag == "Polygon" {
                    " Points=\"50,0 100,100 0,100\""
                } else {
                    ""
                };
                let password_char =
                    if extra_control(index).is_some_and(|item| item.0 == "Password box") {
                        " PasswordChar=\"●\""
                    } else {
                        ""
                    };
                format!(
                    "    <{tag} x:Name=\"{name}\"{text_attribute}{color_attribute}{polygon_points}{password_char} Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                    control.width, control.height, control.x, control.y
                )
            }
        };
        let tooltip = xml_escape(&control.tooltip);
        let event_binding = control
            .event_name
            .as_ref()
            .map_or_else(String::new, |event| {
                format!(" {event}=\"{}_{}\"", name, event)
            });
        let supports_font_properties = supports_font_properties(control.kind);
        let font_family_attr = if control.font_family.is_empty() || !supports_font_properties {
            String::new()
        } else {
            format!(" FontFamily=\"{font_family}\"")
        };
        let font_weight_attr = if control.font_weight.is_empty() || !supports_font_properties {
            String::new()
        } else {
            format!(" FontWeight=\"{}\"", xml_escape(&control.font_weight))
        };
        let padding_attr = if matches!(
            control.kind,
            ControlKind::Button
                | ControlKind::TextBox
                | ControlKind::CheckBox
                | ControlKind::ToggleSwitch
                | ControlKind::RadioButton
        ) && control.padding > 0.0
        {
            format!(" Padding=\"{:.0}\"", control.padding)
        } else {
            String::new()
        };
        let common = format!(
            "{event_binding}{font_family_attr}{font_weight_attr}{padding_attr} IsVisible=\"{}\" IsEnabled=\"{}\" Opacity=\"{:.2}\" ToolTip.Tip=\"{tooltip}\"",
            control.is_visible, control.is_enabled, control.opacity,
        );
        let (element, common) = merge_preserved_control_attributes(control, element, common);
        let mut block = String::new();
        let extra_title = match control.kind {
            ControlKind::Extra(index) => extra_control(index).map(|item| item.0),
            _ => None,
        };
        if extra_title == Some("Menu bar") {
            let opening = element.strip_suffix(" />\n").unwrap_or(element.as_str());
            block.push_str(&format!("{opening}{common}>\n"));
            for (category, children) in parse_hierarchical_items(&control.text) {
                let category = xml_escape(&category);
                if children.is_empty() {
                    block.push_str(&format!("      <MenuItem Header=\"{category}\" />\n"));
                } else {
                    block.push_str(&format!("      <MenuItem Header=\"{category}\">\n"));
                    for child in children {
                        block.push_str(&format!(
                            "        <MenuItem Header=\"{}\" />\n",
                            xml_escape(&child)
                        ));
                    }
                    block.push_str("      </MenuItem>\n");
                }
            }
            block.push_str("    </Menu>\n");
        } else if extra_title == Some("Tree view") {
            let opening = element.strip_suffix(" />\n").unwrap_or(element.as_str());
            block.push_str(&format!("{opening}{common}>\n"));
            for (item, children) in parse_hierarchical_items(&control.text) {
                let item = xml_escape(&item);
                if children.is_empty() {
                    block.push_str(&format!("      <TreeViewItem Header=\"{item}\" />\n"));
                } else {
                    block.push_str(&format!("      <TreeViewItem Header=\"{item}\">\n"));
                    for child in children {
                        block.push_str(&format!(
                            "        <TreeViewItem Header=\"{}\" />\n",
                            xml_escape(&child)
                        ));
                    }
                    block.push_str("      </TreeViewItem>\n");
                }
            }
            block.push_str("    </TreeView>\n");
        } else if extra_title == Some("Split button") {
            let opening = element.strip_suffix(" />\n").unwrap_or(element.as_str());
            block.push_str(&format!("{opening}{common}>\n"));
            block.push_str(
                "      <SplitButton.Flyout>\n        <MenuFlyout Placement=\"Bottom\">\n",
            );
            let mut actions = control
                .text
                .lines()
                .skip(1)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if actions.is_empty() {
                actions.extend(["Option 1", "Option 2"]);
            }
            for action in actions {
                block.push_str(&format!(
                    "          <MenuItem Header=\"{}\" />\n",
                    xml_escape(action)
                ));
            }
            block.push_str(
                "        </MenuFlyout>\n      </SplitButton.Flyout>\n    </SplitButton>\n",
            );
        } else if extra_title == Some("List box") {
            let opening = element.strip_suffix(" />\n").unwrap_or(element.as_str());
            block.push_str(&format!("{opening}{common}>\n"));
            for item in control
                .text
                .lines()
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                block.push_str(&format!(
                    "      <ListBoxItem Content=\"{}\" />\n",
                    xml_escape(item)
                ));
            }
            block.push_str("    </ListBox>\n");
        } else if extra_title == Some("Items control") {
            let opening = element.strip_suffix(" />\n").unwrap_or(element.as_str());
            block.push_str(&format!("{opening}{common}>\n"));
            for item in control
                .text
                .lines()
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                block.push_str(&format!(
                    "      <TextBlock Text=\"{}\" Margin=\"4\" />\n",
                    xml_escape(item)
                ));
            }
            block.push_str("    </ItemsControl>\n");
        } else if extra_title == Some("Tab control") {
            let opening = element.strip_suffix(" />\n").unwrap_or(element.as_str());
            block.push_str(&format!("{opening}{common}>\n"));
            for tab in control
                .text
                .lines()
                .map(str::trim)
                .filter(|tab| !tab.is_empty())
            {
                block.push_str(&format!(
                    "      <TabItem Header=\"{}\"><TextBlock Text=\"{} content\" Margin=\"8\" /></TabItem>\n",
                    xml_escape(tab), xml_escape(tab)
                ));
            }
            block.push_str("    </TabControl>\n");
        } else if control.kind == ControlKind::ComboBox {
            let opening = element.strip_suffix(" />\n").unwrap_or(element.as_str());
            block.push_str(&format!("{opening}{common}>\n"));
            let items = control
                .text
                .lines()
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            for item in items {
                block.push_str(&format!(
                    "      <ComboBoxItem Content=\"{}\" />\n",
                    xml_escape(item)
                ));
            }
            block.push_str("    </ComboBox>\n");
        } else {
            block.push_str(&element.replace(" />", &format!("{common} />")));
        }
        control_blocks.push(block);
    }
    for (index, control) in controls.iter().enumerate() {
        if control.parent.is_none() {
            append_xaml_control(index, controls, &control_blocks, 1, &mut xaml);
        }
    }
    xaml.push_str("    </Canvas>\n  </Viewbox>\n</Window>\n");
    xaml
}

fn append_xaml_control(
    index: usize,
    controls: &[DesignerControl],
    blocks: &[String],
    depth: usize,
    output: &mut String,
) {
    let Some(control) = controls.get(index) else {
        return;
    };
    let child_indices = controls
        .iter()
        .enumerate()
        .filter_map(|(child_index, child)| {
            (child.parent.as_deref() == Some(control.name.as_str())).then_some(child_index)
        })
        .collect::<Vec<_>>();
    let Some(block) = blocks.get(index) else {
        return;
    };
    if !can_contain_controls(control.kind) || child_indices.is_empty() {
        output.push_str(block);
        return;
    }
    let Some(opening) = block.strip_suffix(" />\n") else {
        output.push_str(block);
        return;
    };
    let indent = "  ".repeat(depth);
    output.push_str(opening);
    output.push_str(">\n");
    let frame_size = frame_content_size(control, controls);
    output.push_str(&format!(
        "{indent}  <Canvas Width=\"{:.0}\" Height=\"{:.0}\">\n",
        frame_size.x, frame_size.y
    ));
    for child_index in child_indices {
        append_xaml_control(child_index, controls, blocks, depth + 2, output);
    }
    output.push_str(&format!(
        "{indent}  </Canvas>\n{indent}</{}>\n",
        control.kind.tag()
    ));
}

fn frame_content_size(frame: &DesignerControl, controls: &[DesignerControl]) -> egui::Vec2 {
    let mut size = egui::vec2(frame.width.max(1.0), frame.height.max(1.0));
    for child in controls
        .iter()
        .filter(|child| child.parent.as_deref() == Some(frame.name.as_str()))
    {
        let child_size = if can_contain_controls(child.kind) {
            frame_content_size(child, controls)
        } else {
            egui::vec2(child.width, child.height)
        };
        size.x = size.x.max(child.x + child_size.x + 8.0);
        size.y = size.y.max(child.y + child_size.y + 8.0);
    }
    size
}

fn design_surface_size(controls: &[DesignerControl]) -> (f32, f32) {
    let mut width: f32 = 900.0;
    let mut height: f32 = 564.0;
    for control in controls.iter().filter(|control| control.parent.is_none()) {
        let size = if can_contain_controls(control.kind) {
            frame_content_size(control, controls)
        } else {
            egui::vec2(control.width, control.height)
        };
        let right_padding = if matches!(control.kind, ControlKind::Extra(index)
            if extra_control(index).is_some_and(|item| item.0 == "Menu bar"))
        {
            0.0
        } else {
            24.0
        };
        width = width.max(control.x + size.x + right_padding);
        height = height.max(control.y + size.y + 24.0);
    }
    (width, height)
}

fn render_legacy_xaml(project_name: &str, controls: &[DesignerControl]) -> String {
    let mut xaml = format!(
        "<Window xmlns=\"https://github.com/avaloniaui\"\n        xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"\n        x:Class=\"CairnApp.MainWindow\"\n        Title=\"{}\" Width=\"900\" Height=\"600\">\n  <Canvas Background=\"#FF1E1E1E\">\n",
        xml_escape(project_name)
    );
    for control in controls {
        let name = xml_escape(&control.name);
        let text = xml_escape(&control.text);
        let font = if (control.font_size - 14.0).abs() < f32::EPSILON {
            String::new()
        } else {
            format!(" FontSize=\"{:.0}\"", control.font_size)
        };
        let element = match control.kind {
            ControlKind::Button => format!(
                "    <Button x:Name=\"{name}\" Content=\"{text}\" Click=\"{name}_Click\"{font} Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.x, control.y
            ),
            ControlKind::Label => format!(
                "    <TextBlock x:Name=\"{name}\" Text=\"{text}\"{font} Foreground=\"White\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.x, control.y
            ),
            ControlKind::TextBox => format!(
                "    <TextBox x:Name=\"{name}\" Watermark=\"{text}\" Width=\"200\"{font} Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.x, control.y
            ),
            ControlKind::CheckBox | ControlKind::RadioButton => format!(
                "    <{} x:Name=\"{name}\" Content=\"{text}\"{font} Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.kind.tag(),
                control.x,
                control.y
            ),
            ControlKind::Frame => format!(
                "    <Border x:Name=\"{name}\" Background=\"Transparent\" BorderBrush=\"{}\" BorderThickness=\"2\" CornerRadius=\"4\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                xml_escape(&control.color),
                control.width,
                control.height,
                control.x,
                control.y
            ),
            ControlKind::ToggleSwitch => format!(
                "    <ToggleSwitch x:Name=\"{name}\" Content=\"{text}\"{font} Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.x, control.y
            ),
            ControlKind::Slider => format!(
                "    <Slider x:Name=\"{name}\" Minimum=\"0\" Maximum=\"100\" Value=\"50\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.x, control.y
            ),
            ControlKind::ProgressBar => format!(
                "    <ProgressBar x:Name=\"{name}\" Minimum=\"0\" Maximum=\"100\" Value=\"62\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.x, control.y
            ),
            ControlKind::Separator => format!(
                "    <Separator x:Name=\"{name}\" Width=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, control.x, control.y
            ),
            ControlKind::ComboBox => format!(
                "    <ComboBox x:Name=\"{name}\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, control.height, control.x, control.y
            ),
            ControlKind::Calendar => format!(
                "    <Calendar x:Name=\"{name}\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, control.height, control.x, control.y
            ),
            ControlKind::CalendarDatePicker => format!(
                "    <CalendarDatePicker x:Name=\"{name}\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, control.height, control.x, control.y
            ),
            ControlKind::DatePicker => format!(
                "    <DatePicker x:Name=\"{name}\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, control.height, control.x, control.y
            ),
            ControlKind::NumericUpDown => format!(
                "    <NumericUpDown x:Name=\"{name}\" Value=\"5\" Minimum=\"0\" Maximum=\"100\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                control.width, control.height, control.x, control.y
            ),
            ControlKind::Image => format!(
                "    <Image x:Name=\"{name}\" Source=\"{}\" Stretch=\"Uniform\" Width=\"{:.0}\" Height=\"{:.0}\" Canvas.Left=\"{:.0}\" Canvas.Top=\"{:.0}\" />\n",
                xml_escape(&control.text),
                control.width,
                control.height,
                control.x,
                control.y
            ),
            ControlKind::Extra(_) => String::new(),
        };
        xaml.push_str(&element);
    }
    xaml.push_str("  </Canvas>\n</Window>\n");
    xaml
}

fn is_supported_xaml(project_name: &str, contents: &str, controls: &[DesignerControl]) -> bool {
    if contents == render_xaml(project_name, controls)
        || contents == render_legacy_xaml(project_name, controls)
        || contents == render_initial_template_xaml(project_name)
    {
        return true;
    }

    // Rebuild the exact generated form with the current window settings. This
    // keeps XAML produced by the designer editable even when its root size or
    // icon differs from the serializer's defaults.
    if let Ok(document) = roxmltree::Document::parse(contents) {
        let root = document.root_element();
        if root.tag_name().name() == "Window" {
            let title = root.attribute("Title").unwrap_or(project_name);
            let width = root
                .attribute("Width")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(1100.0);
            let height = root
                .attribute("Height")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(760.0);
            let icon = root.attribute("Icon").unwrap_or_default();
            let auto_resize = root
                .children()
                .find(roxmltree::Node::is_element)
                .filter(|node| node.tag_name().name() == "Viewbox")
                .and_then(|node| node.attribute("Stretch"))
                .is_none_or(|stretch| stretch == "Uniform");
            let expected = apply_window_settings(
                &render_xaml(project_name, controls),
                title,
                width,
                height,
                icon,
                auto_resize,
            );
            if contents == expected {
                return true;
            }
        }
    }

    // Avalonia and editors commonly reformat XAML (including putting each
    // attribute on its own line). Validate its actual XML structure instead
    // of comparing it byte-for-byte with our serializer.
    let Ok(document) = roxmltree::Document::parse(contents) else {
        return false;
    };
    let root = document.root_element();
    if root.tag_name().name() != "Window"
        || root.children().filter(roxmltree::Node::is_element).count() != 1
    {
        return false;
    }
    let Some(content) = root.children().find(roxmltree::Node::is_element) else {
        return false;
    };
    let is_viewbox = content.tag_name().name() == "Viewbox";
    if (is_viewbox
        && (content.attributes().any(|attr| attr.name() != "Stretch")
            || content
                .children()
                .filter(roxmltree::Node::is_element)
                .count()
                != 1))
        || (!is_viewbox && content.tag_name().name() != "Canvas")
    {
        return false;
    }
    let canvas = if is_viewbox {
        let Some(canvas) = content.children().find(roxmltree::Node::is_element) else {
            return false;
        };
        canvas
    } else {
        content
    };
    if canvas.tag_name().name() != "Canvas"
        || canvas
            .attributes()
            .any(|attr| !matches!(attr.name(), "Background" | "Width" | "Height"))
    {
        return false;
    }
    let allowed_attributes = [
        "Name",
        "x:Name",
        // Design-time previewer annotations are not part of the runtime
        // layout. Accept them so otherwise-supported documents remain
        // editable in the visual designer.
        "DesignWidth",
        "DesignHeight",
        "DesignData",
        "Content",
        "Header",
        "Source",
        "Stretch",
        "BorderBrush",
        "BorderThickness",
        "CornerRadius",
        "Text",
        "Watermark",
        "PasswordChar",
        "AcceptsReturn",
        "Width",
        "Height",
        "MinHeight",
        "FontSize",
        "FontFamily",
        "FontWeight",
        "Margin",
        "Padding",
        "Background",
        "Foreground",
        "Fill",
        "Points",
        "Watermark",
        "Canvas.Left",
        "Canvas.Top",
        "IsVisible",
        "IsEnabled",
        "Opacity",
        "ToolTip.Tip",
        "Click",
        "TextChanged",
        "ValueChanged",
        "PointerPressed",
        "PointerEntered",
        "PointerExited",
        "KeyDown",
        "Loaded",
        "SelectionChanged",
        "Minimum",
        "Maximum",
        "Value",
        "SelectedIndex",
        "Placement",
        "IsChecked",
    ];
    canvas
        .children()
        .filter(roxmltree::Node::is_element)
        .all(|node| supported_xaml_control(node, &allowed_attributes))
}

fn supported_xaml_control(node: roxmltree::Node<'_, '_>, allowed_attributes: &[&str]) -> bool {
    let supported_tag = matches!(
        node.tag_name().name(),
        "Button"
            | "TextBlock"
            | "TextBox"
            | "CheckBox"
            | "ToggleSwitch"
            | "Slider"
            | "ProgressBar"
            | "RadioButton"
            | "Separator"
            | "Border"
            | "ComboBox"
            | "Calendar"
            | "CalendarDatePicker"
            | "DatePicker"
            | "NumericUpDown"
            | "Image"
            | "ToggleButton"
            | "SplitButton"
            | "DropDownButton"
            | "AutoCompleteBox"
            | "ListBox"
            | "TreeView"
            | "TreeViewItem"
            | "TimePicker"
            | "Grid"
            | "StackPanel"
            | "WrapPanel"
            | "DockPanel"
            | "ScrollViewer"
            | "Expander"
            | "TabControl"
            | "ContentControl"
            | "ItemsControl"
            | "Menu"
            | "MenuItem"
            | "MenuFlyout"
            | "SplitButton.Flyout"
            | "ListBoxItem"
            | "Rectangle"
            | "Ellipse"
            | "Polygon"
            | "TabItem"
            // Backward compatibility for controls exposed by older toolbox
            // builds. The parser maps these to stock equivalents below.
            | "RichTextBox"
            | "RangeSlider"
            | "PasswordBox"
    );
    // Attributes that have no editor are captured in `source_attributes` and
    // written back on save. The element/container structure is still checked
    // below because the designer cannot safely edit an unknown visual tree.
    let attributes_supported = true;
    if !supported_tag || !attributes_supported {
        return false;
    }
    let children = node
        .children()
        .filter(roxmltree::Node::is_element)
        .collect::<Vec<_>>();
    match node.tag_name().name() {
        "ComboBox" => children.iter().all(|child| {
            child.tag_name().name() == "ComboBoxItem"
                && child.children().all(|grandchild| !grandchild.is_element())
                && child.attributes().all(|attr| attr.name() == "Content")
        }),
        "ListBox" => children.iter().all(|child| {
            child.tag_name().name() == "ListBoxItem"
                && child.attributes().all(|attr| attr.name() == "Content")
                && child.children().all(|grandchild| !grandchild.is_element())
        }),
        "ItemsControl" => children.iter().all(|child| {
            child.tag_name().name() == "TextBlock"
                && child
                    .attributes()
                    .all(|attr| matches!(attr.name(), "Text" | "Margin" | "Foreground"))
                && child.children().all(|grandchild| !grandchild.is_element())
        }),
        "TabControl" => children.iter().all(|child| {
            child.tag_name().name() == "TabItem"
                && supported_xaml_control(*child, allowed_attributes)
        }),
        "TabItem" => children.iter().all(|child| {
            child.tag_name().name() == "TextBlock"
                && supported_xaml_control(*child, allowed_attributes)
        }),
        "SplitButton" => {
            children.len() == 1
                && children[0].tag_name().name() == "SplitButton.Flyout"
                && supported_xaml_control(children[0], allowed_attributes)
        }
        "SplitButton.Flyout" => {
            children.len() == 1
                && children[0].tag_name().name() == "MenuFlyout"
                && supported_xaml_control(children[0], allowed_attributes)
        }
        "MenuFlyout" => children.iter().all(|child| {
            child.tag_name().name() == "MenuItem"
                && supported_xaml_control(*child, allowed_attributes)
        }),
        "Menu" => children.iter().all(|child| {
            child.tag_name().name() == "MenuItem"
                && supported_xaml_control(*child, allowed_attributes)
        }),
        "MenuItem" => children.iter().all(|child| {
            child.tag_name().name() == "MenuItem"
                && supported_xaml_control(*child, allowed_attributes)
        }),
        "TreeView" => children.iter().all(|child| {
            child.tag_name().name() == "TreeViewItem"
                && supported_xaml_control(*child, allowed_attributes)
        }),
        "TreeViewItem" => children.iter().all(|child| {
            child.tag_name().name() == "TreeViewItem"
                && supported_xaml_control(*child, allowed_attributes)
        }),
        "Border" if node_attribute(node, "BorderThickness").is_some() => {
            children.len() == 1
                && children[0].tag_name().name() == "Canvas"
                && children[0]
                    .attributes()
                    .all(|attr| matches!(attr.name(), "Background" | "Width" | "Height"))
                && children[0]
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .all(|child| supported_xaml_control(child, allowed_attributes))
        }
        "Grid" | "StackPanel" | "WrapPanel" | "DockPanel" | "ScrollViewer" | "Expander"
        | "ContentControl" => {
            children.len() == 1
                && children[0].tag_name().name() == "Canvas"
                && children[0]
                    .attributes()
                    .all(|attr| matches!(attr.name(), "Background" | "Width" | "Height"))
                && children[0]
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .all(|child| supported_xaml_control(child, allowed_attributes))
        }
        _ => children.is_empty(),
    }
}

fn is_xaml_event_attribute(name: &str) -> bool {
    matches!(
        name,
        "Click"
            | "TextChanged"
            | "ValueChanged"
            | "PointerPressed"
            | "PointerEntered"
            | "PointerExited"
            | "KeyDown"
            | "Loaded"
            | "SelectionChanged"
    )
}

fn xaml_control_kind(node: roxmltree::Node<'_, '_>) -> Option<ControlKind> {
    let tag = node.tag_name().name();
    Some(match tag {
        "Button" => ControlKind::Button,
        "TextBlock" => ControlKind::Label,
        "TextBox" | "RichTextBox" => ControlKind::TextBox,
        "PasswordBox" => ControlKind::Extra(
            EXTRA_CONTROLS
                .iter()
                .position(|item| item.0 == "Password box")? as u8,
        ),
        "CheckBox" => ControlKind::CheckBox,
        "ToggleSwitch" => ControlKind::ToggleSwitch,
        "Slider" | "RangeSlider" => ControlKind::Slider,
        "ProgressBar" => ControlKind::ProgressBar,
        "RadioButton" => ControlKind::RadioButton,
        "Border" if node_attribute(node, "BorderThickness").is_some() => ControlKind::Frame,
        "Border" | "Separator" => ControlKind::Separator,
        "ComboBox" => ControlKind::ComboBox,
        "Calendar" => ControlKind::Calendar,
        "CalendarDatePicker" => ControlKind::CalendarDatePicker,
        "DatePicker" => ControlKind::DatePicker,
        "NumericUpDown" => ControlKind::NumericUpDown,
        "Image" => ControlKind::Image,
        _ => ControlKind::Extra(EXTRA_CONTROLS.iter().position(|item| item.1 == tag)? as u8),
    })
}

fn describe_unsupported_xaml(contents: &str) -> String {
    let document = match roxmltree::Document::parse(contents) {
        Ok(document) => document,
        Err(error) => {
            return format!(
                "This XAML is not well-formed ({error}). Fix it in Code view; the designer is read-only until it parses."
            );
        }
    };
    let root = document.root_element();
    if root.tag_name().name() != "Window" {
        return format!(
            "The root element is <{}>; the visual designer edits <Window> files only.",
            root.tag_name().name()
        );
    }
    let children = root
        .children()
        .filter(roxmltree::Node::is_element)
        .collect::<Vec<_>>();
    let [content] = children.as_slice() else {
        return "The <Window> must contain exactly one <Canvas>, optionally inside a <Viewbox>."
            .to_owned();
    };
    let canvas = if content.tag_name().name() == "Viewbox" {
        content.children().find(roxmltree::Node::is_element)
    } else {
        Some(*content)
    };
    let Some(canvas) = canvas.filter(|node| node.tag_name().name() == "Canvas") else {
        return format!(
            "The window content is <{}>; the designer needs a <Canvas> surface to position controls.",
            content.tag_name().name()
        );
    };
    if let Some(attribute) = canvas
        .attributes()
        .find(|attribute| !matches!(attribute.name(), "Background" | "Width" | "Height"))
    {
        return format!(
            "The <Canvas> property '{}' is not supported by the designer.",
            attribute.name()
        );
    }
    for node in canvas
        .descendants()
        .filter(roxmltree::Node::is_element)
        .skip(1)
    {
        let tag = node.tag_name().name();
        if !is_supported_xaml_tag(tag) && tag != "ComboBoxItem" {
            let name =
                node_attribute(node, "Name").map_or_else(String::new, |name| format!(" '{name}'"));
            return format!("The <{tag}>{name} element is not supported by the visual designer.");
        }
    }
    "The XAML uses a layout or control structure the designer cannot represent yet.".to_owned()
}

fn is_supported_xaml_tag(tag: &str) -> bool {
    matches!(
        tag,
        "Button"
            | "TextBlock"
            | "TextBox"
            | "CheckBox"
            | "ToggleSwitch"
            | "Slider"
            | "ProgressBar"
            | "RadioButton"
            | "Separator"
            | "Border"
            | "ComboBox"
            | "Calendar"
            | "CalendarDatePicker"
            | "DatePicker"
            | "NumericUpDown"
            | "Image"
            | "ToggleButton"
            | "SplitButton"
            | "DropDownButton"
            | "AutoCompleteBox"
            | "ListBox"
            | "TreeView"
            | "TreeViewItem"
            | "TimePicker"
            | "Grid"
            | "StackPanel"
            | "WrapPanel"
            | "DockPanel"
            | "ScrollViewer"
            | "Expander"
            | "TabControl"
            | "ContentControl"
            | "ItemsControl"
            | "Menu"
            | "MenuItem"
            | "MenuFlyout"
            | "SplitButton.Flyout"
            | "ListBoxItem"
            | "Rectangle"
            | "Ellipse"
            | "Polygon"
            | "TabItem"
            | "RichTextBox"
            | "RangeSlider"
            | "PasswordBox"
            | "Canvas"
    )
}

fn render_initial_template_xaml(project_name: &str) -> String {
    let project_name = xml_escape(project_name);
    format!(
        "<Window xmlns=\"https://github.com/avaloniaui\"\n        xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"\n        x:Class=\"CairnApp.MainWindow\"\n        Title=\"{project_name}\" Width=\"1100\" Height=\"760\" MinWidth=\"480\" MinHeight=\"360\">\n  <Viewbox Stretch=\"Uniform\">\n    <Canvas Width=\"900\" Height=\"564\" Background=\"#FF1E1E1E\">\n    <TextBlock x:Name=\"WelcomeText\" Text=\"Welcome to {project_name}!\" Width=\"500\" FontSize=\"24\" Foreground=\"White\" Canvas.Left=\"36\" Canvas.Top=\"32\" />\n    </Canvas>\n  </Viewbox>\n</Window>\n"
    )
}

fn default_control_color(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Button => "#FF4C8BF5",
        ControlKind::Label => "#FFFFFFFF",
        ControlKind::TextBox => "#FF1E1E1E",
        ControlKind::CheckBox
        | ControlKind::RadioButton
        | ControlKind::ToggleSwitch
        | ControlKind::Slider
        | ControlKind::ProgressBar
        | ControlKind::ComboBox
        | ControlKind::Calendar
        | ControlKind::CalendarDatePicker
        | ControlKind::DatePicker
        | ControlKind::NumericUpDown => "#FF4C8BF5",
        ControlKind::Image => "#FF252932",
        ControlKind::Frame => "#FF7A8494",
        ControlKind::Extra(_) => "#FF4C8BF5",
        ControlKind::Separator => "#FF7A8494",
    }
}

fn supports_color_property(kind: ControlKind) -> bool {
    match kind {
        ControlKind::Button
        | ControlKind::Label
        | ControlKind::TextBox
        | ControlKind::CheckBox
        | ControlKind::ToggleSwitch
        | ControlKind::Slider
        | ControlKind::ProgressBar
        | ControlKind::RadioButton
        | ControlKind::Separator
        | ControlKind::ComboBox
        | ControlKind::Calendar
        | ControlKind::CalendarDatePicker
        | ControlKind::DatePicker
        | ControlKind::NumericUpDown
        | ControlKind::Image
        | ControlKind::Frame => true,
        ControlKind::Extra(index) => extra_control(index).is_some_and(|item| {
            matches!(
                item.0,
                "Toggle button"
                    | "Split button"
                    | "Drop-down button"
                    | "Auto-complete box"
                    | "List box"
                    | "Tree view"
                    | "Time picker"
                    | "Tab control"
                    | "Content control"
                    | "Items control"
                    | "Menu item"
                    | "Tab item"
                    | "Rectangle"
                    | "Ellipse"
                    | "Polygon"
            )
        }),
    }
}

fn supports_font_properties(kind: ControlKind) -> bool {
    match kind {
        ControlKind::Button
        | ControlKind::Label
        | ControlKind::TextBox
        | ControlKind::CheckBox
        | ControlKind::ToggleSwitch
        | ControlKind::RadioButton
        | ControlKind::ComboBox
        | ControlKind::CalendarDatePicker
        | ControlKind::NumericUpDown => true,
        ControlKind::Extra(index) => extra_control(index).is_some_and(|item| {
            matches!(
                item.0,
                "Toggle button"
                    | "Split button"
                    | "Drop-down button"
                    | "Auto-complete box"
                    | "Content control"
                    | "Menu item"
                    | "Tab item"
            )
        }),
        _ => false,
    }
}

fn extra_color_property_name(kind: ControlKind) -> Option<&'static str> {
    let ControlKind::Extra(index) = kind else {
        return None;
    };
    let (title, tag, _, _, _) = extra_control(index)?;
    match tag {
        "Rectangle" | "Ellipse" | "Polygon" => Some("Fill"),
        "ToggleButton" | "SplitButton" | "DropDownButton" | "AutoCompleteBox" | "ListBox"
        | "TreeView" | "TimePicker" | "TabControl" | "ContentControl" | "ItemsControl" => {
            Some("Background")
        }
        "MenuItem" | "TabItem" => Some("Foreground"),
        _ if title == "Password box" => Some("Background"),
        _ => None,
    }
}

fn uses_default_theme_surface(kind: ControlKind, color: &str) -> bool {
    let ControlKind::Extra(index) = kind else {
        return false;
    };
    let is_theme_control = extra_control(index).is_some_and(|item| {
        matches!(
            item.0,
            "Password box"
                | "Auto-complete box"
                | "List box"
                | "Tree view"
                | "Time picker"
                | "Tab control"
                | "Content control"
                | "Items control"
        )
    });
    is_theme_control && parse_color(color) == Some(Color32::from_rgb(76, 139, 245))
}

fn extra_preview_caption<'a>(
    index: u8,
    text: &'a str,
    accent: Color32,
) -> (&'a str, Color32, bool) {
    let title = extra_control(index).map_or("", |item| item.0);
    match title {
        "Rectangle" | "Ellipse" | "Polygon" | "Line" | "Grid" | "Stack panel" | "Wrap panel"
        | "Dock panel" | "Scroll viewer" | "Expander" | "Tab control" | "Content control"
        | "Items control" | "List box" | "Tree view" | "Menu bar" => ("", theme::text(), true),
        "Password box" => (
            if text == title { "Password" } else { text },
            theme::muted(),
            true,
        ),
        "Auto-complete box" => (
            if text == title {
                "Type to search…"
            } else {
                text
            },
            theme::muted(),
            true,
        ),
        "Split button" => (
            text.lines().next().unwrap_or("Main action"),
            Color32::WHITE,
            false,
        ),
        "Toggle button" | "Drop-down button" => (text, Color32::WHITE, false),
        _ => (text, accent, true),
    }
}

fn paint_extra_preview(
    painter: &egui::Painter,
    control: &DesignerControl,
    rect: egui::Rect,
    color: Color32,
    zoom: f32,
    show_design_guides: bool,
) {
    let ControlKind::Extra(index) = control.kind else {
        return;
    };
    let title = extra_control(index).map_or("", |item| item.0);
    let radius = egui::CornerRadius::same(4);
    let line = theme::line();
    let panel = theme::panel();
    let surface = if uses_default_theme_surface(control.kind, &control.color) {
        theme::editor()
    } else {
        color
    };
    let stroke = egui::Stroke::new(1.0, line);
    let text = |position, value: &str, color| {
        painter.text(
            position,
            egui::Align2::LEFT_CENTER,
            value,
            egui::FontId::proportional((11.0 * zoom).max(8.0)),
            color,
        );
    };
    match title {
        "Rectangle" => {
            painter.rect_filled(rect, egui::CornerRadius::ZERO, color);
        }
        "Ellipse" => {
            painter.circle_filled(rect.center(), rect.width().min(rect.height()) * 0.5, color);
        }
        "Polygon" => {
            let points = [
                egui::pos2(rect.center().x, rect.top()),
                egui::pos2(rect.right(), rect.bottom()),
                egui::pos2(rect.left(), rect.bottom()),
            ];
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                color,
                egui::Stroke::NONE,
            ));
        }
        "Toggle button" => {
            painter.rect_filled(rect, radius, color);
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
        }
        "Split button" | "Drop-down button" => {
            painter.rect_filled(rect, radius, color);
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
            let split_x = rect.right() - 30.0 * zoom;
            painter.line_segment(
                [
                    egui::pos2(split_x, rect.top()),
                    egui::pos2(split_x, rect.bottom()),
                ],
                stroke,
            );
            painter.text(
                egui::pos2((split_x + rect.right()) * 0.5, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "▾",
                egui::FontId::proportional(12.0 * zoom),
                Color32::WHITE,
            );
        }
        "Password box" | "Auto-complete box" => {
            painter.rect_filled(rect, radius, surface);
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
            if title == "Auto-complete box" {
                painter.circle_stroke(
                    egui::pos2(rect.right() - 17.0 * zoom, rect.center().y - 1.0 * zoom),
                    4.5 * zoom,
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(rect.right() - 14.0 * zoom, rect.center().y + 2.0 * zoom),
                        egui::pos2(rect.right() - 10.0 * zoom, rect.center().y + 6.0 * zoom),
                    ],
                    stroke,
                );
            }
        }
        "List box" | "Items control" => {
            painter.rect_filled(rect, radius, surface);
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
            let items = control
                .text
                .lines()
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            for (row, item) in items.iter().enumerate() {
                let row_height = (rect.height() / items.len().max(1) as f32).min(28.0 * zoom);
                let y = rect.top() + (row as f32 + 0.5) * row_height + 4.0 * zoom;
                if row == 0 {
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(rect.left() + 1.0, y - 8.0 * zoom),
                            egui::pos2(rect.right() - 1.0, y + 8.0 * zoom),
                        ),
                        egui::CornerRadius::same(2),
                        color,
                    );
                }
                text(
                    egui::pos2(rect.left() + 8.0 * zoom, y),
                    item,
                    if row == 0 {
                        Color32::WHITE
                    } else {
                        theme::text()
                    },
                );
            }
        }
        "Tree view" => {
            painter.rect_filled(rect, radius, surface);
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
            let rows = parse_hierarchical_items(&control.text)
                .into_iter()
                .map(|(parent, _children)| parent)
                .collect::<Vec<_>>();
            let row_height = ((rect.height() - 8.0 * zoom) / rows.len().max(1) as f32)
                .clamp(16.0 * zoom, 28.0 * zoom);
            for (row, label) in rows.iter().enumerate() {
                let y = rect.top() + 5.0 * zoom + (row as f32 + 0.5) * row_height;
                text(
                    egui::pos2(rect.left() + 8.0 * zoom, y),
                    &format!("▸  {label}"),
                    theme::text(),
                );
            }
        }
        "Time picker" | "Menu bar" | "Menu item" | "Tab item" => {
            painter.rect_filled(
                rect,
                radius,
                if title == "Time picker" {
                    surface
                } else {
                    theme::editor()
                },
            );
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
            if title == "Time picker" {
                painter.text(
                    rect.right_center() - egui::vec2(19.0 * zoom, 0.0),
                    egui::Align2::CENTER_CENTER,
                    "◷",
                    egui::FontId::proportional(15.0 * zoom),
                    color,
                );
            } else if title == "Menu bar" {
                painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::editor());
                let categories = parse_hierarchical_items(&control.text);
                let mut x = rect.left() + 12.0 * zoom;
                for (category, _) in categories {
                    text(egui::pos2(x, rect.center().y), &category, theme::text());
                    x += (category.chars().count() as f32 * 8.0 + 24.0) * zoom;
                }
                painter.line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    egui::Stroke::new(1.0, line),
                );
            } else if title == "Tab item" || title == "Menu item" {
                painter.rect_filled(rect, radius, theme::editor());
            }
        }
        "Tab control" => {
            painter.rect_filled(rect, radius, surface);
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
            let tabs = control
                .text
                .lines()
                .map(str::trim)
                .filter(|tab| !tab.is_empty())
                .collect::<Vec<_>>();
            let tab_height = 26.0 * zoom;
            let mut x = rect.left();
            for (index, tab_title) in tabs.iter().enumerate() {
                let tab_width = (tab_title.chars().count() as f32 * 8.0 + 24.0) * zoom;
                let tab = egui::Rect::from_min_size(
                    egui::pos2(x, rect.top()),
                    egui::vec2(tab_width, tab_height),
                );
                painter.rect_filled(tab, radius, if index == 0 { color } else { panel });
                text(
                    tab.left_center() + egui::vec2(8.0 * zoom, 0.0),
                    tab_title,
                    if index == 0 {
                        Color32::WHITE
                    } else {
                        theme::muted()
                    },
                );
                x += tab_width;
            }
            let content = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + tab_height),
                rect.max,
            );
            painter.rect_stroke(content, radius, stroke, egui::StrokeKind::Inside);
            text(
                content.left_top() + egui::vec2(12.0 * zoom, 24.0 * zoom),
                &format!("{} content", tabs.first().copied().unwrap_or("Tab")),
                theme::text(),
            );
        }
        "Grid" | "Stack panel" | "Wrap panel" | "Dock panel" | "Scroll viewer" | "Expander"
        | "Content control" => {
            if !show_design_guides {
                if title == "Content control" {
                    painter.rect_filled(rect, radius, surface);
                    painter.text(
                        rect.left_center() + egui::vec2(8.0 * zoom, 0.0),
                        egui::Align2::LEFT_CENTER,
                        if control.text.is_empty() {
                            "Content"
                        } else {
                            control.text.as_str()
                        },
                        egui::FontId::proportional((control.font_size * zoom).max(8.0)),
                        theme::text(),
                    );
                } else if title == "Expander" {
                    painter.rect_filled(rect, radius, panel);
                    painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
                    painter.text(
                        rect.left_center() + egui::vec2(10.0 * zoom, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &format!("▸  {}", control.text),
                        egui::FontId::proportional((11.0 * zoom).max(8.0)),
                        theme::text(),
                    );
                }
                return;
            }
            painter.rect_filled(
                rect,
                radius,
                if title == "Content control" {
                    surface
                } else {
                    panel
                },
            );
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
            let heading = match title {
                "Stack panel" => "StackPanel",
                "Wrap panel" => "WrapPanel",
                "Dock panel" => "DockPanel",
                "Scroll viewer" => "ScrollViewer",
                "Content control" => "Content",
                other => other,
            };
            let top = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 23.0 * zoom));
            painter.rect_filled(top, radius, theme::editor());
            text(
                top.left_center() + egui::vec2(8.0 * zoom, 0.0),
                heading,
                theme::muted(),
            );
            let inner = rect.shrink(8.0 * zoom);
            if title == "Grid" || title == "Dock panel" {
                for fraction in [0.33, 0.66] {
                    let x = inner.left() + inner.width() * fraction;
                    painter.line_segment(
                        [egui::pos2(x, top.bottom()), egui::pos2(x, inner.bottom())],
                        stroke,
                    );
                }
                painter.line_segment(
                    [
                        egui::pos2(inner.left(), inner.center().y),
                        egui::pos2(inner.right(), inner.center().y),
                    ],
                    stroke,
                );
            } else if title == "Wrap panel" {
                for row in 0..2 {
                    for column in 0..3 {
                        let cell = egui::Rect::from_min_size(
                            egui::pos2(
                                inner.left() + column as f32 * 34.0 * zoom,
                                inner.top() + row as f32 * 28.0 * zoom + 22.0 * zoom,
                            ),
                            egui::vec2(28.0 * zoom, 20.0 * zoom),
                        );
                        painter.rect_stroke(
                            cell,
                            egui::CornerRadius::same(2),
                            stroke,
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            } else if title == "Expander" {
                painter.text(
                    rect.left_center() + egui::vec2(10.0 * zoom, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &format!("▸  {}", control.text),
                    egui::FontId::proportional(11.0 * zoom),
                    theme::text(),
                );
            }
        }
        _ => {
            painter.rect_filled(rect, radius, panel);
            painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
        }
    }
}

fn parse_color(value: &str) -> Option<Color32> {
    let hex = value.strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => Some(Color32::from_rgb(
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        )),
        8 => Some(Color32::from_rgba_unmultiplied(
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
            u8::from_str_radix(&hex[0..2], 16).ok()?,
        )),
        _ => None,
    }
}

fn with_opacity(color: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (f32::from(color.a()) * opacity.clamp(0.0, 1.0)).round() as u8,
    )
}

fn render_qt_layout(project_name: &str, controls: &[DesignerControl]) -> String {
    let mut layout = format!("window\t{}\t900\t600\n", layout_escape(project_name));
    for control in controls {
        layout.push_str(&format!(
            "{}\t{}\t{}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{}\t{}\t{}\t{:.2}\t{}\t{}\t{}\t{:.0}\n",
            match control.kind {
                ControlKind::Button => "button",
                ControlKind::Label => "label",
                ControlKind::TextBox => "textbox",
                ControlKind::CheckBox => "checkbox",
                ControlKind::ToggleSwitch => "toggle",
                ControlKind::Slider => "slider",
                ControlKind::ProgressBar => "progress",
                ControlKind::RadioButton => "radio",
                ControlKind::Separator => "separator",
                ControlKind::ComboBox => "combobox",
                ControlKind::Calendar => "calendar",
                ControlKind::CalendarDatePicker => "calendardatepicker",
                ControlKind::DatePicker => "datepicker",
                ControlKind::NumericUpDown => "numericupdown",
                ControlKind::Image => "image",
                ControlKind::Frame => "frame",
                ControlKind::Extra(_) => "button",
            },
            layout_escape(&control.name),
            layout_escape(&control.text),
            control.x,
            control.y,
            control.width,
            control.height,
            control.margin,
            layout_escape(&control.color),
            control.is_visible,
            control.is_enabled,
            control.opacity,
            layout_escape(&control.tooltip),
            layout_escape(&control.font_family),
            layout_escape(&control.font_weight),
            control.padding,
        ));
    }
    layout
}

fn parse_qt_controls(layout: &str) -> Vec<DesignerControl> {
    layout
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let kind = match fields.next()? {
                "button" => ControlKind::Button,
                "label" => ControlKind::Label,
                "textbox" => ControlKind::TextBox,
                "checkbox" => ControlKind::CheckBox,
                "toggle" => ControlKind::ToggleSwitch,
                "slider" => ControlKind::Slider,
                "progress" => ControlKind::ProgressBar,
                "radio" => ControlKind::RadioButton,
                "separator" => ControlKind::Separator,
                "combobox" => ControlKind::ComboBox,
                "calendar" => ControlKind::Calendar,
                "calendardatepicker" => ControlKind::CalendarDatePicker,
                "datepicker" => ControlKind::DatePicker,
                "numericupdown" => ControlKind::NumericUpDown,
                "image" => ControlKind::Image,
                "frame" => ControlKind::Frame,
                _ => return None,
            };
            Some(DesignerControl {
                kind,
                name: layout_unescape(fields.next()?),
                text: layout_unescape(fields.next()?),
                x: fields.next()?.parse().ok()?,
                y: fields.next()?.parse().ok()?,
                width: fields
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(kind.size().x),
                height: fields
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(kind.size().y),
                font_size: fields
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(14.0),
                margin: fields
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0),
                color: fields
                    .next()
                    .map(layout_unescape)
                    .unwrap_or_else(|| default_control_color(kind).to_owned()),
                is_visible: fields
                    .next()
                    .is_none_or(|value| value.eq_ignore_ascii_case("true")),
                is_enabled: fields
                    .next()
                    .is_none_or(|value| value.eq_ignore_ascii_case("true")),
                opacity: fields
                    .next()
                    .and_then(|value| value.parse::<f32>().ok())
                    .unwrap_or(1.0)
                    .clamp(0.0, 1.0),
                tooltip: fields.next().map(layout_unescape).unwrap_or_default(),
                parent: None,
                event_name: None,
                font_family: fields.next().map(layout_unescape).unwrap_or_default(),
                font_weight: fields.next().map(layout_unescape).unwrap_or_default(),
                padding: fields
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0),
                source_attributes: Vec::new(),
            })
        })
        .collect()
}

fn layout_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn layout_unescape(value: &str) -> String {
    let mut result = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            result.push(match character {
                't' => '\t',
                'n' => '\n',
                'r' => '\r',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            result.push(character);
        }
    }
    if escaped {
        result.push('\\');
    }
    result
}

fn cpp_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn render_qt_ui_code(project: &ProjectInfo, controls: &[DesignerControl]) -> String {
    let mut code = format!(
        "    setWindowTitle(\"{}\");\n    resize(900, 600);\n",
        cpp_escape(&project.name)
    );
    for control in controls {
        let class = match control.kind {
            ControlKind::Button => "QPushButton",
            ControlKind::Label => "QLabel",
            ControlKind::TextBox => "QLineEdit",
            ControlKind::CheckBox | ControlKind::ToggleSwitch => "QCheckBox",
            ControlKind::RadioButton => "QRadioButton",
            ControlKind::Slider => "QSlider",
            ControlKind::ProgressBar => "QProgressBar",
            ControlKind::Separator => "QFrame",
            ControlKind::ComboBox => "QComboBox",
            ControlKind::Calendar => "QCalendarWidget",
            ControlKind::CalendarDatePicker | ControlKind::DatePicker => "QDateEdit",
            ControlKind::NumericUpDown => "QSpinBox",
            ControlKind::Image => "QLabel",
            ControlKind::Frame => "QFrame",
            ControlKind::Extra(_) => "QLabel",
        };
        // Controls are class members, so handlers can use them: Text1->setText("...").
        let constructor = match control.kind {
            ControlKind::Slider
            | ControlKind::ProgressBar
            | ControlKind::Separator
            | ControlKind::Frame
            | ControlKind::Calendar
            | ControlKind::CalendarDatePicker
            | ControlKind::DatePicker
            | ControlKind::NumericUpDown
            | ControlKind::ComboBox => {
                format!("{class}(this)")
            }
            _ => format!("{class}(\"{}\", this)", cpp_escape(&control.text)),
        };
        code.push_str(&format!(
            "    {name} = new {constructor};\n    {name}->setGeometry({x}, {y}, {width}, {height});\n",
            name = control.name,
            constructor = constructor,
            x = control.x as i32,
            y = control.y as i32,
            width = control.width as i32,
            height = control.height as i32,
        ));
        if control.kind == ControlKind::Button {
            code.push_str(&format!(
                "    QObject::connect({0}, &QPushButton::clicked, this, &MainWindow::{0}_Click);\n",
                control.name
            ));
        }
    }
    code
}

fn render_qt_members(controls: &[DesignerControl]) -> String {
    let mut members = String::new();
    for control in controls {
        let class = match control.kind {
            ControlKind::Button => "QPushButton",
            ControlKind::Label => "QLabel",
            ControlKind::TextBox => "QLineEdit",
            ControlKind::CheckBox | ControlKind::ToggleSwitch => "QCheckBox",
            ControlKind::RadioButton => "QRadioButton",
            ControlKind::Slider => "QSlider",
            ControlKind::ProgressBar => "QProgressBar",
            ControlKind::Separator => "QFrame",
            ControlKind::ComboBox => "QComboBox",
            ControlKind::Calendar => "QCalendarWidget",
            ControlKind::CalendarDatePicker | ControlKind::DatePicker => "QDateEdit",
            ControlKind::NumericUpDown => "QSpinBox",
            ControlKind::Image => "QLabel",
            ControlKind::Frame => "QFrame",
            ControlKind::Extra(_) => "QLabel",
        };
        members.push_str(&format!("    {class}* {} = nullptr;\n", control.name));
    }
    members
}

/// Keeps the generated member declarations (one pointer per designer control) in MainWindow.h.
fn update_qt_header_members(header: &str, generated: &str) -> Result<String, String> {
    let start_marker = "    // <cairn-generated-members>\n";
    let end_marker = "    // </cairn-generated-members>";
    let mut header = header.to_owned();
    for include in ["QLabel", "QLineEdit", "QPushButton"] {
        let directive = format!("#include <{include}>");
        if !header.contains(&directive) {
            let class_start = header
                .find("class MainWindow")
                .ok_or_else(|| "Could not find the MainWindow class in MainWindow.h.".to_owned())?;
            header.insert_str(class_start, &format!("{directive}\n"));
        }
    }
    if !header.contains(start_marker) {
        let class_end = header
            .rfind("};")
            .ok_or_else(|| "Could not find the end of the MainWindow class.".to_owned())?;
        header.insert_str(class_end, &format!("public:\n{start_marker}{end_marker}\n"));
    }
    let start = header.find(start_marker).unwrap_or_default() + start_marker.len();
    let end = header[start..]
        .find(end_marker)
        .map(|offset| start + offset)
        .ok_or_else(|| "Could not find the end of the generated members marker.".to_owned())?;
    header.replace_range(start..end, generated);
    Ok(header)
}

fn replace_generated_qt_ui(code: &str, generated: &str) -> Result<String, String> {
    let start_marker = "    // <cairn-generated-ui>\n";
    let end_marker = "    // </cairn-generated-ui>";
    let start = code
        .find(start_marker)
        .ok_or_else(|| "Could not find the generated UI marker in MainWindow.cpp.".to_owned())?;
    let content_start = start + start_marker.len();
    let end = code[content_start..]
        .find(end_marker)
        .map(|offset| content_start + offset)
        .ok_or_else(|| "Could not find the end of the generated UI marker.".to_owned())?;
    Ok(format!(
        "{}{}{}{}",
        &code[..content_start],
        generated,
        if generated.ends_with('\n') { "" } else { "\n" },
        &code[end..]
    ))
}

fn ensure_qt_button_handler(project: &ProjectInfo, name: &str) -> Result<(), String> {
    let header_path = project.path.join("src/MainWindow.h");
    let cpp_path = project.path.join("src/MainWindow.cpp");
    let mut header = std::fs::read_to_string(&header_path)
        .map_err(|error| format!("Could not read '{}': {error}", header_path.display()))?;
    let declaration = format!("    void {name}_Click();");
    if !header.contains(&declaration) {
        let class_end = header
            .rfind("};")
            .ok_or_else(|| "Could not find the end of the MainWindow class.".to_owned())?;
        let slots = if header.contains("private slots:") {
            format!("{declaration}\n")
        } else {
            format!("private slots:\n{declaration}\n")
        };
        header.insert_str(class_end, &slots);
        std::fs::write(&header_path, header)
            .map_err(|error| format!("Could not update '{}': {error}", header_path.display()))?;
    }

    let mut cpp = std::fs::read_to_string(&cpp_path)
        .map_err(|error| format!("Could not read '{}': {error}", cpp_path.display()))?;
    let definition = format!("void MainWindow::{name}_Click()");
    if !cpp.contains(&definition) {
        cpp.push_str(&format!("\n{definition} {{\n}}\n"));
        std::fs::write(&cpp_path, cpp)
            .map_err(|error| format!("Could not update '{}': {error}", cpp_path.display()))?;
    }
    Ok(())
}

fn save_qt_project(project: &ProjectInfo, controls: &[DesignerControl]) -> Result<(), String> {
    for control in controls
        .iter()
        .filter(|control| control.kind == ControlKind::Button)
    {
        ensure_qt_button_handler(project, &control.name)?;
    }
    let code_path = project.path.join("src/MainWindow.cpp");
    let code = std::fs::read_to_string(&code_path)
        .map_err(|error| format!("Could not read '{}': {error}", code_path.display()))?;
    let updated_code = replace_generated_qt_ui(&code, &render_qt_ui_code(project, controls))?;
    let header_path = project.path.join("src/MainWindow.h");
    let header = std::fs::read_to_string(&header_path)
        .map_err(|error| format!("Could not read '{}': {error}", header_path.display()))?;
    let updated_header = update_qt_header_members(&header, &render_qt_members(controls))?;
    let layout_path = project.path.join("MainWindow.cairn-ui");
    std::fs::write(&layout_path, render_qt_layout(&project.name, controls))
        .map_err(|error| format!("Could not save '{}': {error}", layout_path.display()))?;
    if updated_header != header {
        std::fs::write(&header_path, updated_header)
            .map_err(|error| format!("Could not update '{}': {error}", header_path.display()))?;
    }
    if updated_code != code {
        std::fs::write(&code_path, updated_code)
            .map_err(|error| format!("Could not update '{}': {error}", code_path.display()))?;
    }
    Ok(())
}

fn control_name(value: &str, index: usize, existing: &[String]) -> String {
    let mut name = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>();
    if name.is_empty() {
        name = format!("Control{}", index + 1);
    } else if !name
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        name.insert_str(0, "Control");
    }
    let base = name.clone();
    let mut suffix = 2;
    while existing.iter().any(|existing| existing == &name) {
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    name
}

fn nested_items_text(node: roxmltree::Node<'_, '_>, item_tag: &str) -> String {
    let mut lines = Vec::new();
    for item in node
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == item_tag)
    {
        if let Some(header) = node_attribute(item, "Header") {
            lines.push(header);
        }
        for child in item
            .children()
            .filter(|child| child.is_element() && child.tag_name().name() == item_tag)
        {
            if let Some(header) = node_attribute(child, "Header") {
                lines.push(format!("  {header}"));
            }
        }
    }
    lines.join("\n")
}

fn direct_item_text(node: roxmltree::Node<'_, '_>, item_tag: &str, attribute: &str) -> String {
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == item_tag)
        .filter_map(|item| node_attribute(item, attribute))
        .collect::<Vec<_>>()
        .join("\n")
}

fn unmanaged_attributes(
    node: roxmltree::Node<'_, '_>,
    control_name: &str,
) -> Vec<(String, String)> {
    const MANAGED: &[&str] = &[
        "Name",
        "Content",
        "Text",
        "Watermark",
        "Header",
        "Source",
        "Width",
        "Height",
        "FontSize",
        "FontFamily",
        "FontWeight",
        "Margin",
        "Padding",
        "Background",
        "Foreground",
        "BorderBrush",
        "Canvas.Left",
        "Canvas.Top",
        "IsVisible",
        "IsEnabled",
        "Opacity",
        "ToolTip.Tip",
        "Value",
        "PasswordChar",
    ];
    let kind = xaml_control_kind(node);
    node.attributes()
        .filter_map(|attribute| {
            let qualified_name = match attribute.namespace() {
                Some(uri) => node
                    .lookup_prefix(uri)
                    .map(|prefix| format!("{prefix}:{}", attribute.name()))
                    .unwrap_or_else(|| attribute.name().to_owned()),
                None => attribute.name().to_owned(),
            };
            if qualified_name == "x:Name"
                || (attribute.namespace().is_none() && MANAGED.contains(&attribute.name()))
            {
                return None;
            }
            if attribute.namespace().is_none()
                && is_xaml_event_attribute(attribute.name())
                && attribute.value() == format!("{control_name}_{}", attribute.name())
                && kind.is_some_and(|kind| {
                    designer_events(kind)
                        .iter()
                        .any(|(event, _)| *event == attribute.name())
                })
            {
                return None;
            }
            Some((qualified_name, attribute.value().to_owned()))
        })
        .collect()
}

fn merge_preserved_control_attributes(
    control: &DesignerControl,
    mut element: String,
    mut common: String,
) -> (String, String) {
    for (name, value) in &control.source_attributes {
        let marker = format!(" {name}=\"");
        if element.contains(&marker) {
            set_xml_attribute(&mut element, name, &xml_escape(value));
        } else if common.contains(&marker) {
            set_xml_attribute(&mut common, name, &xml_escape(value));
        } else {
            common.push_str(&format!(" {name}=\"{}\"", xml_escape(value)));
        }
    }
    (element, common)
}

fn preserve_root_attributes(source: &str, generated: &str) -> String {
    const DESIGNER_OWNED: [&str; 4] = ["Title", "Width", "Height", "Icon"];
    let Ok(document) = roxmltree::Document::parse(source) else {
        return generated.to_owned();
    };
    let Some(start) = generated.find("<Window") else {
        return generated.to_owned();
    };
    let Some(relative_end) = generated[start..].find('>') else {
        return generated.to_owned();
    };
    let end = start + relative_end;
    let mut opening = generated[start..end].to_owned();
    let root = document.root_element();
    for namespace in root.namespaces() {
        if namespace.name() == Some("xml") {
            continue;
        }
        let declaration = match namespace.name() {
            Some(prefix) => format!("xmlns:{prefix}"),
            None => "xmlns".to_owned(),
        };
        let value = xml_escape(namespace.uri());
        if opening.contains(&format!(" {declaration}=\"")) {
            set_xml_attribute(&mut opening, &declaration, &value);
        } else {
            opening.push_str(&format!("\n        {declaration}=\"{value}\""));
        }
    }
    for attribute in root.attributes() {
        let qualified = match attribute.namespace() {
            Some(uri) => match root.lookup_prefix(uri) {
                Some(prefix) if !prefix.is_empty() => format!("{prefix}:{}", attribute.name()),
                _ => attribute.name().to_owned(),
            },
            None => attribute.name().to_owned(),
        };
        if DESIGNER_OWNED.contains(&qualified.as_str()) {
            continue;
        }
        let marker = format!(" {qualified}=\"");
        let value = xml_escape(attribute.value());
        match opening.find(&marker) {
            Some(at) => {
                let value_start = at + marker.len();
                if let Some(length) = opening[value_start..].find('"') {
                    opening.replace_range(value_start..value_start + length, &value);
                }
            }
            None => opening.push_str(&format!("\n        {qualified}=\"{value}\"")),
        }
    }
    let mut merged = generated.to_owned();
    merged.replace_range(start..end, &opening);
    merged
}

fn parse_controls(xaml: &str) -> Vec<DesignerControl> {
    let Ok(document) = roxmltree::Document::parse(xaml) else {
        return Vec::new();
    };
    let root = document.root_element();
    let Some(content) = root.children().find(roxmltree::Node::is_element) else {
        return Vec::new();
    };
    let canvas = if content.tag_name().name() == "Viewbox" {
        content.children().find(|node| node.is_element())
    } else if content.tag_name().name() == "Canvas" {
        Some(content)
    } else {
        None
    };
    let Some(canvas) = canvas else {
        return Vec::new();
    };
    canvas
        .descendants()
        .filter(|node| node.is_element())
        .filter(|node| {
            !node.ancestors().skip(1).any(|ancestor| {
                matches!(
                    ancestor.tag_name().name(),
                    "Menu" | "TreeView" | "ListBox" | "TabControl" | "SplitButton"
                )
            })
        })
        .filter_map(|node| {
            let kind = if node.tag_name().name() == "Button" {
                ControlKind::Button
            } else if node.tag_name().name() == "TextBlock" {
                ControlKind::Label
            } else if node.tag_name().name() == "PasswordBox" {
                ControlKind::Extra(
                    EXTRA_CONTROLS
                        .iter()
                        .position(|item| item.0 == "Password box")? as u8,
                )
            } else if node.tag_name().name() == "RichTextBox" {
                ControlKind::TextBox
            } else if node.tag_name().name() == "RangeSlider" {
                ControlKind::Slider
            } else if node.tag_name().name() == "TextBox"
                && node_attribute(node, "PasswordChar").is_some()
            {
                ControlKind::Extra(
                    EXTRA_CONTROLS
                        .iter()
                        .position(|item| item.0 == "Password box")? as u8,
                )
            } else if node.tag_name().name() == "TextBox" {
                ControlKind::TextBox
            } else if node.tag_name().name() == "CheckBox" {
                ControlKind::CheckBox
            } else if node.tag_name().name() == "ToggleSwitch" {
                ControlKind::ToggleSwitch
            } else if node.tag_name().name() == "Slider" {
                ControlKind::Slider
            } else if node.tag_name().name() == "ProgressBar" {
                ControlKind::ProgressBar
            } else if node.tag_name().name() == "RadioButton" {
                ControlKind::RadioButton
            } else if node.tag_name().name() == "Border"
                && node_attribute(node, "BorderThickness").is_some()
            {
                ControlKind::Frame
            } else if node.tag_name().name() == "Separator" || node.tag_name().name() == "Border" {
                ControlKind::Separator
            } else if node.tag_name().name() == "ComboBox" {
                ControlKind::ComboBox
            } else if node.tag_name().name() == "Calendar" {
                ControlKind::Calendar
            } else if node.tag_name().name() == "CalendarDatePicker" {
                ControlKind::CalendarDatePicker
            } else if node.tag_name().name() == "DatePicker" {
                ControlKind::DatePicker
            } else if node.tag_name().name() == "NumericUpDown" {
                ControlKind::NumericUpDown
            } else if node.tag_name().name() == "Image" {
                ControlKind::Image
            } else if let Some(index) = EXTRA_CONTROLS
                .iter()
                .position(|item| item.1 == node.tag_name().name())
            {
                ControlKind::Extra(index as u8)
            } else {
                return None;
            };
            let name = node_attribute(node, "Name")?;
            let get = |key| node_attribute(node, key);
            let text = match kind {
                ControlKind::Button => get("Content").unwrap_or_default(),
                ControlKind::Label => get("Text").unwrap_or_default(),
                ControlKind::TextBox => get("Watermark").unwrap_or_default(),
                ControlKind::CheckBox | ControlKind::ToggleSwitch | ControlKind::RadioButton => {
                    get("Content").unwrap_or_default()
                }
                ControlKind::Slider => "Value".to_owned(),
                ControlKind::ProgressBar | ControlKind::Separator => String::new(),
                ControlKind::ComboBox => node
                    .children()
                    .filter(|child| child.is_element() && child.tag_name().name() == "ComboBoxItem")
                    .filter_map(|item| node_attribute(item, "Content"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                ControlKind::Calendar | ControlKind::DatePicker => String::new(),
                ControlKind::CalendarDatePicker => get("Watermark").unwrap_or_default(),
                ControlKind::NumericUpDown => get("Value").unwrap_or_else(|| "5".to_owned()),
                ControlKind::Image => get("Source").unwrap_or_default(),
                ControlKind::Frame => String::new(),
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Menu bar") =>
                {
                    nested_items_text(node, "MenuItem")
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Tree view") =>
                {
                    nested_items_text(node, "TreeViewItem")
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "List box") =>
                {
                    direct_item_text(node, "ListBoxItem", "Content")
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Items control") =>
                {
                    direct_item_text(node, "TextBlock", "Text")
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Tab control") =>
                {
                    direct_item_text(node, "TabItem", "Header")
                }
                ControlKind::Extra(index)
                    if extra_control(index).is_some_and(|item| item.0 == "Split button") =>
                {
                    let label = get("Content").unwrap_or_else(|| "Main action".to_owned());
                    let items = node
                        .descendants()
                        .filter(|child| child.is_element() && child.tag_name().name() == "MenuItem")
                        .filter_map(|item| node_attribute(item, "Header"))
                        .collect::<Vec<_>>();
                    if items.is_empty() {
                        label
                    } else {
                        format!("{label}\n{}", items.join("\n"))
                    }
                }
                ControlKind::Extra(_) => get("Content")
                    .or_else(|| get("Header"))
                    .or_else(|| get("Watermark"))
                    .or_else(|| get("Text"))
                    .unwrap_or_default(),
            };
            let event_name = designer_events(kind).iter().find_map(|(event, _)| {
                get(event).and_then(|handler| {
                    (handler == format!("{name}_{event}")).then(|| (*event).to_owned())
                })
            });
            let source_attributes = unmanaged_attributes(node, &name);
            Some(DesignerControl {
                kind,
                name,
                text,
                x: get("Canvas.Left")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(36.0),
                y: get("Canvas.Top")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(32.0),
                width: get("Width")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(kind.size().x),
                height: get("Height")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(kind.size().y),
                font_size: get("FontSize")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(14.0),
                margin: get("Margin")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0),
                color: get(match kind {
                    ControlKind::Button
                    | ControlKind::TextBox
                    | ControlKind::ProgressBar
                    | ControlKind::Separator => "Background",
                    ControlKind::Label
                    | ControlKind::CheckBox
                    | ControlKind::ToggleSwitch
                    | ControlKind::RadioButton
                    | ControlKind::Slider
                    | ControlKind::ComboBox
                    | ControlKind::Calendar
                    | ControlKind::CalendarDatePicker
                    | ControlKind::DatePicker
                    | ControlKind::NumericUpDown => "Foreground",
                    ControlKind::Image => "Source",
                    ControlKind::Frame => "BorderBrush",
                    ControlKind::Extra(_) => {
                        extra_color_property_name(kind).unwrap_or("Foreground")
                    }
                })
                .unwrap_or_else(|| default_control_color(kind).to_owned()),
                is_visible: get("IsVisible").is_none_or(|value| value.eq_ignore_ascii_case("true")),
                is_enabled: get("IsEnabled").is_none_or(|value| value.eq_ignore_ascii_case("true")),
                opacity: get("Opacity")
                    .and_then(|value| value.parse::<f32>().ok())
                    .unwrap_or(1.0)
                    .clamp(0.0, 1.0),
                tooltip: get("ToolTip.Tip").unwrap_or_default(),
                parent: node
                    .ancestors()
                    .skip(1)
                    .find(|ancestor| is_named_container_node(*ancestor))
                    .and_then(|ancestor| node_attribute(ancestor, "Name")),
                event_name,
                font_family: get("FontFamily").unwrap_or_default(),
                font_weight: get("FontWeight").unwrap_or_default(),
                padding: get("Padding")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0),
                source_attributes,
            })
        })
        .collect()
}

fn node_attribute(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.attributes()
        .find(|attribute| {
            attribute.name() == name || attribute.name().ends_with(&format!(":{name}"))
        })
        // roxmltree has already decoded XML entities in attribute values.
        .map(|attribute| attribute.value().to_owned())
}

fn is_named_container_node(node: roxmltree::Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "Grid"
            | "StackPanel"
            | "WrapPanel"
            | "DockPanel"
            | "ScrollViewer"
            | "Expander"
            | "ContentControl"
    ) || (node.tag_name().name() == "Border" && node_attribute(node, "BorderThickness").is_some())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn apply_window_settings(
    xaml: &str,
    title: &str,
    width: f32,
    height: f32,
    icon: &str,
    auto_resize_content: bool,
) -> String {
    let Some(start) = xaml.find("<Window") else {
        return xaml.to_owned();
    };
    let Some(relative_end) = xaml[start..].find('>') else {
        return xaml.to_owned();
    };
    let end = start + relative_end;
    let mut opening = xaml[start..end].to_owned();
    set_xml_attribute(&mut opening, "Title", &xml_escape(title));
    set_xml_attribute(&mut opening, "Width", &format!("{width:.0}"));
    set_xml_attribute(&mut opening, "Height", &format!("{height:.0}"));
    set_xml_attribute(&mut opening, "CanResize", "True");
    if icon.trim().is_empty() {
        remove_xml_attribute(&mut opening, "Icon");
    } else {
        set_xml_attribute(&mut opening, "Icon", &xml_escape(icon));
    }
    let mut updated = xaml.to_owned();
    updated.replace_range(start..end, &opening);
    if let Some(canvas_start) = updated[start..]
        .find("<Canvas ")
        .map(|offset| start + offset)
        && let Some(canvas_end) = updated[canvas_start..]
            .find('>')
            .map(|offset| canvas_start + offset)
    {
        let mut canvas_opening = updated[canvas_start..canvas_end].to_owned();
        set_xml_size_attribute(&mut canvas_opening, "Width", width);
        set_xml_size_attribute(&mut canvas_opening, "Height", (height - TITLE_H).max(120.0));
        updated.replace_range(canvas_start..canvas_end, &canvas_opening);
    }
    if let Some(viewbox_start) = updated[start..]
        .find("<Viewbox ")
        .map(|offset| start + offset)
        && let Some(viewbox_end) = updated[viewbox_start..]
            .find('>')
            .map(|offset| viewbox_start + offset)
    {
        let mut viewbox_opening = updated[viewbox_start..viewbox_end].to_owned();
        set_xml_attribute(
            &mut viewbox_opening,
            "Stretch",
            if auto_resize_content {
                "Uniform"
            } else {
                "None"
            },
        );
        updated.replace_range(viewbox_start..viewbox_end, &viewbox_opening);
    }
    updated
}

fn set_xml_attribute(tag: &mut String, name: &str, value: &str) {
    let attribute = format!("{name}=\"");
    if let Some(start) = tag.find(&attribute) {
        let value_start = start + attribute.len();
        if let Some(relative_end) = tag[value_start..].find('"') {
            tag.replace_range(value_start..value_start + relative_end, value);
        }
    } else {
        tag.push_str(&format!(" {name}=\"{value}\""));
    }
}

fn remove_xml_attribute(tag: &mut String, name: &str) {
    let attribute = format!(" {name}=\"");
    let Some(start) = tag.find(&attribute) else {
        return;
    };
    let value_start = start + attribute.len();
    let Some(relative_end) = tag[value_start..].find('"') else {
        return;
    };
    tag.replace_range(start..=value_start + relative_end, "");
}

fn copy_window_icon(project: &ProjectInfo, source: &std::path::Path) -> Result<String, String> {
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "The selected icon has no valid filename.".to_owned())?;
    let assets = project.path.join("Assets");
    std::fs::create_dir_all(&assets)
        .map_err(|error| format!("Could not create '{}': {error}", assets.display()))?;
    let destination = assets.join(file_name);
    if source != destination {
        std::fs::copy(source, &destination).map_err(|error| {
            format!(
                "Could not copy window icon to '{}': {error}",
                destination.display()
            )
        })?;
    }
    let project_file = project.path.join(format!("{}.csproj", project.name));
    let project_xml = std::fs::read_to_string(&project_file)
        .map_err(|error| format!("Could not read '{}': {error}", project_file.display()))?;
    let include = format!("Assets\\{file_name}");
    if !project_xml.contains(&include) {
        let Some(project_end) = project_xml.rfind("</Project>") else {
            return Err(format!(
                "Could not add the icon resource to '{}'.",
                project_file.display()
            ));
        };
        let mut updated = project_xml;
        updated.insert_str(
            project_end,
            &format!(
                "  <ItemGroup>\n    <AvaloniaResource Include=\"{include}\" />\n  </ItemGroup>\n"
            ),
        );
        std::fs::write(&project_file, updated)
            .map_err(|error| format!("Could not register the icon resource: {error}"))?;
    }
    Ok(format!("/Assets/{file_name}"))
}

/// Named controls (Text1, Button1, ...) are only assigned by the generated
/// `InitializeComponent()`. Older projects call `AvaloniaXamlLoader.Load(this)`,
/// which leaves them null and crashes the app on `Text1.Text = ...`.
fn use_generated_initialize_component(project: &ProjectInfo) {
    let path = project.path.join("MainWindow.axaml.cs");
    if let Ok(code) = std::fs::read_to_string(&path)
        && code.contains("partial class MainWindow")
        && code.contains("AvaloniaXamlLoader.Load(this);")
        && !code.contains("InitializeComponent(")
    {
        let _ = std::fs::write(
            &path,
            code.replace("AvaloniaXamlLoader.Load(this);", "InitializeComponent();"),
        );
    }
}

fn ensure_button_handler(project: &ProjectInfo, name: &str) -> Result<(), String> {
    use_generated_initialize_component(project);
    let path = project.path.join("MainWindow.axaml.cs");
    let mut code = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read code-behind '{}': {error}", path.display()))?;
    let handler = format!("{name}_Click");
    if code.contains(&format!(" {handler}(")) {
        return Ok(());
    }
    let class_end = code
        .rfind('}')
        .ok_or_else(|| "Could not find the end of MainWindow class.".to_owned())?;
    code.insert_str(
        class_end,
        &format!(
            "\n    private void {handler}(object? sender, Avalonia.Interactivity.RoutedEventArgs e)\n    {{\n    }}\n"
        ),
    );
    std::fs::write(&path, code)
        .map_err(|error| format!("Could not update code-behind '{}': {error}", path.display()))
}

fn ensure_control_handler(project: &ProjectInfo, name: &str) -> Result<(), String> {
    if project.template == ProjectTemplate::QtWidgetsApp {
        ensure_qt_button_handler(project, name)
    } else {
        ensure_button_handler(project, name)
    }
}

fn event_for_control(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Button
        | ControlKind::CheckBox
        | ControlKind::ToggleSwitch
        | ControlKind::RadioButton => "Click",
        ControlKind::TextBox => "TextChanged",
        ControlKind::Slider => "ValueChanged",
        ControlKind::ComboBox => "SelectionChanged",
        ControlKind::Label
        | ControlKind::ProgressBar
        | ControlKind::Separator
        | ControlKind::Image
        | ControlKind::Frame => "PointerPressed",
        ControlKind::Calendar
        | ControlKind::CalendarDatePicker
        | ControlKind::DatePicker
        | ControlKind::NumericUpDown
        | ControlKind::Extra(_) => "PointerPressed",
    }
}

fn designer_events(kind: ControlKind) -> &'static [(&'static str, &'static str)] {
    match kind {
        ControlKind::Button
        | ControlKind::CheckBox
        | ControlKind::ToggleSwitch
        | ControlKind::RadioButton => &[
            ("Click", "Raised when the control is activated."),
            (
                "PointerEntered",
                "Raised when the pointer enters the control.",
            ),
            (
                "PointerExited",
                "Raised when the pointer leaves the control.",
            ),
            ("KeyDown", "Raised when a key is pressed while focused."),
            ("Loaded", "Raised when the control is attached and loaded."),
        ],
        ControlKind::TextBox => &[
            ("TextChanged", "Raised when the text value changes."),
            (
                "PointerEntered",
                "Raised when the pointer enters the control.",
            ),
            (
                "PointerExited",
                "Raised when the pointer leaves the control.",
            ),
            ("KeyDown", "Raised when a key is pressed while focused."),
            ("Loaded", "Raised when the control is attached and loaded."),
        ],
        ControlKind::Slider => &[
            ("ValueChanged", "Raised when the slider value changes."),
            (
                "PointerEntered",
                "Raised when the pointer enters the control.",
            ),
            (
                "PointerExited",
                "Raised when the pointer leaves the control.",
            ),
            ("KeyDown", "Raised when a key is pressed while focused."),
            ("Loaded", "Raised when the control is attached and loaded."),
        ],
        ControlKind::Label
        | ControlKind::ProgressBar
        | ControlKind::Separator
        | ControlKind::Image
        | ControlKind::Frame => &[
            (
                "PointerPressed",
                "Raised when a pointer button is pressed over the control.",
            ),
            (
                "PointerEntered",
                "Raised when the pointer enters the control.",
            ),
            (
                "PointerExited",
                "Raised when the pointer leaves the control.",
            ),
            ("Loaded", "Raised when the control is attached and loaded."),
        ],
        ControlKind::ComboBox => &[
            ("SelectionChanged", "Raised when the selected item changes."),
            (
                "PointerEntered",
                "Raised when the pointer enters the control.",
            ),
            (
                "PointerExited",
                "Raised when the pointer leaves the control.",
            ),
            ("Loaded", "Raised when the control is attached and loaded."),
        ],
        ControlKind::Calendar
        | ControlKind::CalendarDatePicker
        | ControlKind::DatePicker
        | ControlKind::NumericUpDown
        | ControlKind::Extra(_) => &[
            (
                "PointerPressed",
                "Raised when the pointer presses the control.",
            ),
            (
                "PointerEntered",
                "Raised when the pointer enters the control.",
            ),
            (
                "PointerExited",
                "Raised when the pointer leaves the control.",
            ),
            ("Loaded", "Raised when the control is attached and loaded."),
        ],
    }
}

fn avalonia_handler_source(name: &str, event: &str) -> String {
    let args = match event {
        "TextChanged" => "Avalonia.Controls.TextChangedEventArgs",
        "ValueChanged" => "Avalonia.Controls.Primitives.RangeBaseValueChangedEventArgs",
        "SelectionChanged" => "Avalonia.Controls.SelectionChangedEventArgs",
        "PointerPressed" => "Avalonia.Input.PointerPressedEventArgs",
        "PointerEntered" | "PointerExited" => "Avalonia.Input.PointerEventArgs",
        "KeyDown" => "Avalonia.Input.KeyEventArgs",
        _ => "Avalonia.Interactivity.RoutedEventArgs",
    };
    format!("    public void {name}(object? sender, {args} e)\n    {{\n    }}\n")
}

fn ensure_avalonia_event_handler(
    project: &ProjectInfo,
    name: &str,
    event: &str,
) -> Result<String, String> {
    use_generated_initialize_component(project);
    let path = project.path.join("MainWindow.axaml.cs");
    let mut code = std::fs::read_to_string(&path)
        .map_err(|error| format!("Could not read code-behind '{}': {error}", path.display()))?;
    let source = avalonia_handler_source(name, event);
    let signature = source.lines().next().unwrap_or_default();
    if code.contains(signature.trim()) {
        return Ok(String::new());
    }
    let previous_signature = source.replacen("public void", "private void", 1);
    if code.contains(&previous_signature) {
        code = code.replacen(&previous_signature, &source, 1);
        std::fs::write(&path, code).map_err(|error| {
            format!("Could not update code-behind '{}': {error}", path.display())
        })?;
        return Ok(source);
    }
    // Older designer builds could create the selected handler name with the
    // control's default event signature. Upgrade that exact empty generated
    // stub in place so existing projects recover without duplicate overloads.
    for previous_event in [
        "Click",
        "TextChanged",
        "ValueChanged",
        "SelectionChanged",
        "PointerPressed",
        "PointerEntered",
        "PointerExited",
        "KeyDown",
        "Loaded",
    ] {
        let previous = avalonia_handler_source(name, previous_event);
        for previous in [
            previous.clone(),
            previous.replacen("public void", "private void", 1),
        ] {
            if code.contains(&previous) {
                code = code.replacen(&previous, &source, 1);
                std::fs::write(&path, code).map_err(|error| {
                    format!("Could not update code-behind '{}': {error}", path.display())
                })?;
                return Ok(source);
            }
        }
    }
    if code.contains(&format!("{name}(")) {
        return Err(format!(
            "Handler '{name}' already exists with a different signature. Update its parameters to match the selected '{event}' event; the designer preserved your method."
        ));
    }
    let class_end = code
        .rfind('}')
        .ok_or_else(|| "Could not find the end of MainWindow class.".to_owned())?;
    code.insert_str(class_end, &source);
    std::fs::write(&path, code)
        .map_err(|error| format!("Could not update code-behind '{}': {error}", path.display()))?;
    Ok(source)
}

/// Repairs empty event-handler stubs written by older designer versions before
/// a build. Only exact generated stubs are changed; user-authored methods are
/// left untouched.
pub(crate) fn repair_stale_avalonia_event_handlers(project: &ProjectInfo) -> Result<(), String> {
    if project.template != ProjectTemplate::AvaloniaApp {
        return Ok(());
    }
    let project_file = project.path.join(format!("{}.csproj", project.name));
    if let Ok(project_xml) = std::fs::read_to_string(&project_file) {
        // Avalonia's build targets include .axaml files automatically. A
        // second wildcard AvaloniaResource declaration compiles App.axaml
        // twice and produces AVLN2002 duplicate x:Class errors.
        let mut lines = project_xml.lines().map(str::to_owned).collect::<Vec<_>>();
        let wildcard_indices = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let compact = line.replace([' ', '\t', '\\'], "");
                (compact.contains("<AvaloniaResource")
                    && compact.contains("*.axaml")
                    && compact.contains("**"))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        for index in wildcard_indices.into_iter().rev() {
            lines.remove(index);
            if index > 0 && lines[index - 1].trim() == "<ItemGroup>" {
                lines.remove(index - 1);
                if index - 1 < lines.len() && lines[index - 1].trim() == "</ItemGroup>" {
                    lines.remove(index - 1);
                }
            } else if index < lines.len() && lines[index].trim() == "</ItemGroup>" {
                lines.remove(index);
            }
        }
        let mut repaired = lines.join("\n");
        if project_xml.ends_with('\n') {
            repaired.push('\n');
        }
        if repaired != project_xml {
            std::fs::write(&project_file, repaired).map_err(|error| {
                format!(
                    "Could not remove the duplicate Avalonia XAML resource glob from '{}': {error}",
                    project_file.display()
                )
            })?;
        }
    }
    let xaml_path = project.path.join("MainWindow.axaml");
    let mut xaml = match std::fs::read_to_string(&xaml_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Could not inspect Avalonia event handlers in '{}': {error}",
                xaml_path.display()
            ));
        }
    };
    // These names were exposed by an earlier toolbox version but do not exist
    // in the bundled Avalonia controls. Convert those generated elements to
    // their closest stock controls before building old user projects.
    let compatible_xaml = xaml
        .replace("<RichTextBox", "<TextBox AcceptsReturn=\"True\"")
        .replace("</RichTextBox>", "</TextBox>")
        .replace("<RangeSlider", "<Slider")
        .replace("</RangeSlider>", "</Slider>")
        .replace("<PasswordBox", "<TextBox PasswordChar=\"●\"")
        .replace("</PasswordBox>", "</TextBox>");
    if compatible_xaml != xaml {
        std::fs::write(&xaml_path, &compatible_xaml).map_err(|error| {
            format!(
                "Could not update unsupported Avalonia controls in '{}': {error}",
                xaml_path.display()
            )
        })?;
        xaml = compatible_xaml;
    }
    // Older projects may use a root Canvas with different formatting,
    // background colors, and Canvas dimensions. Fit that surface uniformly
    // inside the actual Window client area so child controls remain visible.
    if let Some(repaired) = wrap_root_canvas_in_viewbox(&xaml) {
        std::fs::write(&xaml_path, &repaired).map_err(|error| {
            format!(
                "Could not make the Avalonia design surface responsive in '{}': {error}",
                xaml_path.display()
            )
        })?;
        xaml = repaired;
    }
    let (xaml_with_runtime_sizes, runtime_sizes_changed) =
        upgrade_generated_runtime_control_sizes(&xaml);
    if runtime_sizes_changed {
        std::fs::write(&xaml_path, &xaml_with_runtime_sizes).map_err(|error| {
            format!(
                "Could not fit Avalonia date controls in '{}': {error}",
                xaml_path.display()
            )
        })?;
        xaml = xaml_with_runtime_sizes;
    }
    if let Some(repaired) = fit_root_canvas_to_content(&xaml) {
        std::fs::write(&xaml_path, &repaired).map_err(|error| {
            format!(
                "Could not expand the Avalonia design surface in '{}': {error}",
                xaml_path.display()
            )
        })?;
        xaml = repaired;
    }
    let (xaml_with_watermarks, watermarks_changed) = upgrade_calendar_date_picker_watermarks(&xaml);
    if watermarks_changed {
        std::fs::write(&xaml_path, &xaml_with_watermarks).map_err(|error| {
            format!(
                "Could not repair CalendarDatePicker watermarks in '{}': {error}",
                xaml_path.display()
            )
        })?;
        xaml = xaml_with_watermarks;
    }
    let code_path = project.path.join("MainWindow.axaml.cs");
    let mut code = match std::fs::read_to_string(&code_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Could not inspect Avalonia code-behind '{}': {error}",
                code_path.display()
            ));
        }
    };
    let mut changed = false;
    for control in parse_controls(&xaml) {
        let Some(event) = control.event_name.as_deref() else {
            continue;
        };
        let name = format!("{}_{}", control.name, event);
        let expected = avalonia_handler_source(&name, event);
        if code.contains(expected.lines().next().unwrap_or_default().trim()) {
            continue;
        }
        let private_expected = expected.replacen("public void", "private void", 1);
        if code.contains(&private_expected) {
            code = code.replacen(&private_expected, &expected, 1);
            changed = true;
            continue;
        }
        for previous_event in [
            "Click",
            "TextChanged",
            "ValueChanged",
            "SelectionChanged",
            "PointerPressed",
            "PointerEntered",
            "PointerExited",
            "KeyDown",
            "Loaded",
        ] {
            if previous_event == event {
                continue;
            }
            let previous = avalonia_handler_source(&name, previous_event);
            for previous in [
                previous.clone(),
                previous.replacen("public void", "private void", 1),
            ] {
                if code.contains(&previous) {
                    code = code.replacen(&previous, &expected, 1);
                    changed = true;
                    break;
                }
            }
            if changed {
                break;
            }
        }
        if !code.contains(&format!("{name}(")) {
            let Some(class_end) = code.rfind('}') else {
                return Err(format!(
                    "Could not add the Avalonia event handler '{name}' because '{}' has no closing class brace.",
                    code_path.display()
                ));
            };
            code.insert_str(class_end, &expected);
            changed = true;
        }
    }
    if changed {
        std::fs::write(&code_path, code).map_err(|error| {
            format!(
                "Could not repair generated Avalonia event handlers in '{}': {error}",
                code_path.display()
            )
        })?;
    }
    Ok(())
}

fn wrap_root_canvas_in_viewbox(xaml: &str) -> Option<String> {
    let document = roxmltree::Document::parse(xaml).ok()?;
    let root = document.root_element();
    if root.tag_name().name() != "Window" {
        return None;
    }
    let mut elements = root.children().filter(roxmltree::Node::is_element);
    let canvas = elements.next()?;
    if elements.next().is_some() || canvas.tag_name().name() != "Canvas" {
        return None;
    }

    let range = canvas.range();
    let has_width = canvas.attribute("Width").is_some();
    let has_height = canvas.attribute("Height").is_some();
    let mut repaired = xaml.to_owned();
    let opening_wrapper = "<Viewbox Stretch=\"Uniform\">\n";
    repaired.insert_str(range.end, "\n</Viewbox>");
    repaired.insert_str(range.start, opening_wrapper);

    let canvas_start = range.start + opening_wrapper.len();
    let canvas_end = repaired[canvas_start..].find('>')? + canvas_start;
    let mut attributes = String::new();
    if !has_width {
        attributes.push_str(" Width=\"1100\"");
    }
    if !has_height {
        attributes.push_str(" Height=\"760\"");
    }
    repaired.insert_str(canvas_end, &attributes);
    Some(repaired)
}

fn fit_root_canvas_to_content(xaml: &str) -> Option<String> {
    let document = roxmltree::Document::parse(xaml).ok()?;
    let root = document.root_element();
    let mut content = root.children().find(roxmltree::Node::is_element)?;
    if content.tag_name().name() == "Viewbox" {
        content = content.children().find(roxmltree::Node::is_element)?;
    }
    if content.tag_name().name() != "Canvas" {
        return None;
    }
    let mut width = content
        .attribute("Width")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(900.0)
        .max(900.0);
    let mut height = content
        .attribute("Height")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(564.0)
        .max(564.0);
    for child in content.children().filter(roxmltree::Node::is_element) {
        let left = node_attribute(child, "Left")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let top = node_attribute(child, "Top")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let child_width = child
            .attribute("Width")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(180.0);
        let child_height = child
            .attribute("Height")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(48.0);
        width = width.max(left + child_width);
        height = height.max(top + child_height);
    }
    let range = content.range();
    let opening_end = xaml[range.start..range.end].find('>')? + range.start;
    let mut opening = xaml[range.start..opening_end].to_owned();
    set_xml_size_attribute(&mut opening, "Width", width);
    set_xml_size_attribute(&mut opening, "Height", height);
    if opening == xaml[range.start..opening_end] {
        return None;
    }
    let mut repaired = xaml.to_owned();
    repaired.replace_range(range.start..opening_end, &opening);
    Some(repaired)
}

fn set_xml_size_attribute(tag: &mut String, name: &str, value: f32) {
    let attribute = format!("{name}=\"");
    if let Some(start) = tag.find(&attribute) {
        let value_start = start + attribute.len();
        if let Some(relative_end) = tag[value_start..].find('"') {
            tag.replace_range(
                value_start..value_start + relative_end,
                &format!("{value:.0}"),
            );
        }
    } else {
        tag.push_str(&format!(" {name}=\"{value:.0}\""));
    }
}

fn upgrade_calendar_date_picker_watermarks(xaml: &str) -> (String, bool) {
    let mut repaired = xaml.to_owned();
    let mut cursor = 0;
    let mut changed = false;
    while let Some(relative_start) = repaired[cursor..].find("<CalendarDatePicker") {
        let start = cursor + relative_start;
        let Some(relative_end) = repaired[start..].find('>') else {
            break;
        };
        let end = start + relative_end;
        if repaired[start..=end].contains("PlaceholderText=") {
            let opening = repaired[start..=end].replace("PlaceholderText=", "Watermark=");
            repaired.replace_range(start..=end, &opening);
            changed = true;
            cursor = start + opening.len();
        } else {
            cursor = end + 1;
        }
    }
    (repaired, changed)
}

fn upgrade_generated_runtime_control_sizes(xaml: &str) -> (String, bool) {
    let Ok(document) = roxmltree::Document::parse(xaml) else {
        return (xaml.to_owned(), false);
    };
    let mut updates = Vec::<(usize, usize, String)>::new();
    for node in document.descendants().filter(roxmltree::Node::is_element) {
        let tag = node.tag_name().name();
        let (mut width, mut height): (f32, f32) = match tag {
            "Calendar" => (340.0, 400.0),
            "DatePicker" => (360.0, 48.0),
            "CalendarDatePicker" => (240.0, 44.0),
            _ => continue,
        };
        width = width.max(
            node.attribute("Width")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0),
        );
        height = height.max(
            node.attribute("Height")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0),
        );
        let range = node.range();
        let Some(relative_end) = xaml[range.start..range.end].find('>') else {
            continue;
        };
        let end = relative_end + range.start;
        let mut opening = xaml[range.start..end].to_owned();
        set_xml_size_attribute(&mut opening, "Width", width);
        set_xml_size_attribute(&mut opening, "Height", height);
        if opening != xaml[range.start..end] {
            updates.push((range.start, end, opening));
        }
    }
    // Avalonia's native date controls render wider and taller than the
    // designer's compact placeholder. Grow their containing Frame and its
    // child Canvas so neither the control nor its popup anchor gets clipped.
    for border in document.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "Border"
            && node.attribute("CornerRadius").is_some()
    }) {
        let Some(canvas) = border
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "Canvas")
        else {
            continue;
        };
        let mut required_width: f32 = border
            .attribute("Width")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0.0);
        let mut required_height: f32 = border
            .attribute("Height")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0.0);
        for child in canvas.children().filter(roxmltree::Node::is_element) {
            let left = node_attribute(child, "Left")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(0.0);
            let top = node_attribute(child, "Top")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(0.0);
            let (min_width, min_height) = match child.tag_name().name() {
                "Calendar" => (340.0, 400.0),
                "DatePicker" => (360.0, 48.0),
                "CalendarDatePicker" => (240.0, 44.0),
                _ => (0.0, 0.0),
            };
            let child_width = child
                .attribute("Width")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(min_width)
                .max(min_width);
            let child_height = child
                .attribute("Height")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(min_height)
                .max(min_height);
            required_width = required_width.max(left + child_width + 20.0);
            required_height = required_height.max(top + child_height + 20.0);
        }
        for node in [border, canvas] {
            let range = node.range();
            let Some(relative_end) = xaml[range.start..range.end].find('>') else {
                continue;
            };
            let end = range.start + relative_end;
            let mut opening = xaml[range.start..end].to_owned();
            set_xml_size_attribute(&mut opening, "Width", required_width);
            set_xml_size_attribute(&mut opening, "Height", required_height);
            if opening != xaml[range.start..end] {
                updates.push((range.start, end, opening));
            }
        }
    }
    if updates.is_empty() {
        return (xaml.to_owned(), false);
    }
    updates.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut repaired = xaml.to_owned();
    for (start, end, opening) in updates {
        repaired.replace_range(start..end, &opening);
    }
    (repaired, true)
}

#[cfg(test)]
mod tests {
    use super::{
        ControlKind, DesignerControl, control_name, outline_order, parse_controls,
        parse_qt_controls, render_qt_layout, render_qt_ui_code, render_xaml,
    };
    use crate::project::{ProjectInfo, ProjectLanguage, ProjectTemplate};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    fn temp_directory() -> PathBuf {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cairn-designer-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("temporary directory created");
        path
    }

    #[test]
    fn designer_control_names_are_valid_and_unique() {
        let existing = vec!["Submit".to_owned(), "Control2".to_owned()];
        assert_eq!(control_name("Submit", 0, &existing), "Submit2");
        assert_eq!(control_name("2 Save!", 1, &existing), "Control2Save");
        assert_eq!(control_name("!!!", 4, &existing), "Control5");
    }

    #[test]
    fn designer_xaml_round_trips_controls_and_escapes_text() {
        let controls = vec![
            DesignerControl {
                kind: ControlKind::Button,
                name: "Button1".to_owned(),
                text: "Save & Close".to_owned(),
                x: 36.0,
                y: 92.0,
                width: 120.0,
                height: 38.0,
                font_size: 14.0,
                margin: 2.0,
                color: "#FF4C8BF5".to_owned(),
                is_visible: false,
                is_enabled: false,
                opacity: 0.55,
                tooltip: "Save the current file".to_owned(),
                parent: None,
                event_name: Some("Click".to_owned()),
                font_family: "Inter".to_owned(),
                font_weight: "Bold".to_owned(),
                padding: 8.0,
                source_attributes: Vec::new(),
            },
            DesignerControl {
                kind: ControlKind::TextBox,
                name: "TextBox2".to_owned(),
                text: "Enter name".to_owned(),
                x: 100.0,
                y: 140.0,
                width: 200.0,
                height: 36.0,
                font_size: 14.0,
                margin: 0.0,
                color: "#FF1E1E1E".to_owned(),
                is_visible: true,
                is_enabled: true,
                opacity: 1.0,
                tooltip: String::new(),
                parent: None,
                event_name: None,
                font_family: String::new(),
                font_weight: String::new(),
                padding: 0.0,
                source_attributes: Vec::new(),
            },
        ];
        let xaml = render_xaml("Demo", &controls);
        assert!(xaml.contains("Save &amp; Close"));
        assert_eq!(parse_controls(&xaml).len(), 2);
        assert_eq!(parse_controls(&xaml)[0].text, "Save & Close");
        assert_eq!(parse_controls(&xaml)[0].width, 120.0);
        assert_eq!(parse_controls(&xaml)[0].margin, 2.0);
        assert_eq!(parse_controls(&xaml)[0].color, "#FF4C8BF5");
        assert!(!parse_controls(&xaml)[0].is_visible);
        assert!(!parse_controls(&xaml)[0].is_enabled);
        assert_eq!(parse_controls(&xaml)[0].opacity, 0.55);
        assert_eq!(parse_controls(&xaml)[0].tooltip, "Save the current file");
        assert_eq!(parse_controls(&xaml)[0].font_family, "Inter");
        assert_eq!(parse_controls(&xaml)[0].font_weight, "Bold");
        assert_eq!(parse_controls(&xaml)[0].padding, 8.0);
        assert_eq!(
            parse_controls(&xaml)[0].event_name.as_deref(),
            Some("Click")
        );
    }

    #[test]
    fn frame_nesting_round_trips_through_xaml_and_outline_order() {
        let frame = DesignerControl {
            kind: ControlKind::Frame,
            name: "Frame1".to_owned(),
            text: String::new(),
            x: 100.0,
            y: 80.0,
            width: 360.0,
            height: 240.0,
            font_size: 14.0,
            margin: 0.0,
            color: "#FF4C8BF5".to_owned(),
            is_visible: true,
            is_enabled: true,
            opacity: 1.0,
            tooltip: String::new(),
            event_name: None,
            parent: None,
            font_family: String::new(),
            font_weight: String::new(),
            padding: 0.0,
            source_attributes: Vec::new(),
        };
        let child = DesignerControl {
            kind: ControlKind::Button,
            name: "Button1".to_owned(),
            text: "Apply".to_owned(),
            x: 16.0,
            y: 20.0,
            width: 120.0,
            height: 38.0,
            font_size: 14.0,
            margin: 0.0,
            color: "#FF4C8BF5".to_owned(),
            is_visible: true,
            is_enabled: true,
            opacity: 1.0,
            tooltip: String::new(),
            event_name: None,
            parent: Some("Frame1".to_owned()),
            font_family: String::new(),
            font_weight: String::new(),
            padding: 0.0,
            source_attributes: Vec::new(),
        };
        let controls = vec![frame, child];
        let xaml = render_xaml("Demo", &controls);
        assert!(xaml.contains("<Border x:Name=\"Frame1\""));
        assert!(xaml.contains("<Border x:Name=\"Frame1\""));
        assert!(xaml.contains("<Canvas Width=\"360\" Height=\"240\""));
        assert!(xaml.contains("<Button x:Name=\"Button1\""));
        assert!(super::is_supported_xaml("Demo", &xaml, &controls));
        let parsed = parse_controls(&xaml);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].parent.as_deref(), Some("Frame1"));
        assert_eq!(outline_order(&parsed), vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn new_avalonia_template_opens_as_editable_designer_document() {
        let xaml = super::render_initial_template_xaml("Demo");
        let controls = parse_controls(&xaml);
        assert!(super::is_supported_xaml("Demo", &xaml, &controls));
    }

    #[test]
    fn formatted_supported_xaml_remains_editable_and_preserves_properties() {
        let xaml = r##"<Window xmlns="https://github.com/avaloniaui"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        x:Class="CairnApp.MainWindow" Title="Demo" Width="1100" Height="760">
  <Canvas Background="#FF1E1E1E">
    <Button
      x:Name="SaveButton"
      Content="Save &amp; Close"
      Width="120" Height="38"
      Canvas.Left="36" Canvas.Top="92"
      IsVisible="True" IsEnabled="True" Opacity="1.00"
      Click="SaveButton_Click" />
  </Canvas>
</Window>"##;
        let controls = parse_controls(xaml);
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].text, "Save & Close");
        assert_eq!(controls[0].x, 36.0);
        assert!(super::is_supported_xaml("Demo", xaml, &controls));

        let with_unmanaged_property = xaml.replace("Width=\"120\"", "Width=\"120\" Grid.Row=\"1\"");
        let preserved_controls = parse_controls(&with_unmanaged_property);
        assert!(super::is_supported_xaml(
            "Demo",
            &with_unmanaged_property,
            &preserved_controls
        ));
        let round_trip = render_xaml("Demo", &preserved_controls);
        assert!(round_trip.contains("Grid.Row=\"1\""));
    }

    #[test]
    fn toggle_switch_xaml_height_is_clamped_for_existing_controls() {
        let toggle = DesignerControl {
            kind: ControlKind::ToggleSwitch,
            name: "EnabledSwitch".to_owned(),
            text: "Enabled".to_owned(),
            x: 200.0,
            y: 300.0,
            width: 220.0,
            height: 48.0,
            font_size: 14.0,
            margin: 0.0,
            color: "#FF4C8BF5".to_owned(),
            is_visible: true,
            is_enabled: true,
            opacity: 1.0,
            tooltip: String::new(),
            parent: None,
            event_name: None,
            font_family: String::new(),
            font_weight: String::new(),
            padding: 0.0,
            source_attributes: Vec::new(),
        };
        let xaml = render_xaml("Demo", &[toggle]);
        assert!(xaml.contains("Height=\"64\" MinHeight=\"56\""));
    }

    #[test]
    fn slider_xaml_height_is_clamped_for_existing_controls() {
        let slider = DesignerControl {
            kind: ControlKind::Slider,
            name: "ValueSlider".to_owned(),
            text: "Value".to_owned(),
            x: 100.0,
            y: 200.0,
            width: 220.0,
            height: 44.0,
            font_size: 14.0,
            margin: 0.0,
            color: "#FF4C8BF5".to_owned(),
            is_visible: true,
            is_enabled: true,
            opacity: 1.0,
            tooltip: String::new(),
            parent: None,
            event_name: None,
            font_family: String::new(),
            font_weight: String::new(),
            padding: 0.0,
            source_attributes: Vec::new(),
        };
        let xaml = render_xaml("Demo", &[slider]);
        assert!(xaml.contains("Height=\"56\" MinHeight=\"48\""));
    }

    #[test]
    fn avalonia_event_handler_generation_is_idempotent() {
        let root = temp_directory();
        let project = ProjectInfo {
            name: "Demo".to_owned(),
            path: root.clone(),
            language: ProjectLanguage::CSharp,
            template: ProjectTemplate::AvaloniaApp,
            entry_file: PathBuf::from("MainWindow.axaml"),
            project_file: None,
            properties: Default::default(),
        };
        fs::write(
            root.join("MainWindow.axaml.cs"),
            "using Avalonia.Controls;\nnamespace CairnApp;\npublic partial class MainWindow : Window { }\n",
        )
        .expect("code behind written");

        let generated = super::ensure_avalonia_event_handler(&project, "Button1_Click", "Click")
            .expect("event handler generated");
        let second = super::ensure_avalonia_event_handler(&project, "Button1_Click", "Click")
            .expect("existing handler accepted");
        let code = fs::read_to_string(root.join("MainWindow.axaml.cs")).unwrap();
        assert!(generated.contains("Button1_Click"));
        assert!(second.is_empty());
        assert_eq!(code.matches("Button1_Click(").count(), 1);
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn avalonia_event_handler_upgrades_old_default_signature_for_selected_event() {
        let root = temp_directory();
        let project = ProjectInfo {
            name: "Demo".to_owned(),
            path: root.clone(),
            language: ProjectLanguage::CSharp,
            template: ProjectTemplate::AvaloniaApp,
            entry_file: PathBuf::from("MainWindow.axaml"),
            project_file: None,
            properties: Default::default(),
        };
        fs::write(
            root.join("MainWindow.axaml.cs"),
            "using Avalonia.Controls;\nnamespace CairnApp;\npublic partial class MainWindow : Window {\n    private void DatePicker1_Loaded(object? sender, Avalonia.Input.PointerPressedEventArgs e)\n    {\n    }\n}\n",
        )
        .expect("legacy code behind written");

        let updated =
            super::ensure_avalonia_event_handler(&project, "DatePicker1_Loaded", "Loaded")
                .expect("legacy generated handler upgraded");
        let code = fs::read_to_string(root.join("MainWindow.axaml.cs")).unwrap();
        assert!(updated.contains("Avalonia.Interactivity.RoutedEventArgs"));
        assert_eq!(code.matches("DatePicker1_Loaded(").count(), 1);
        assert!(code.contains(
            "DatePicker1_Loaded(object? sender, Avalonia.Interactivity.RoutedEventArgs e)"
        ));
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn build_repair_updates_stale_loaded_handler_for_date_picker() {
        let root = temp_directory();
        let project = ProjectInfo {
            name: "Demo".to_owned(),
            path: root.clone(),
            language: ProjectLanguage::CSharp,
            template: ProjectTemplate::AvaloniaApp,
            entry_file: PathBuf::from("MainWindow.axaml"),
            project_file: None,
            properties: Default::default(),
        };
        let controls = [DesignerControl {
            kind: ControlKind::DatePicker,
            name: "DatePicker12".to_owned(),
            text: String::new(),
            x: 20.0,
            y: 30.0,
            width: 220.0,
            height: 38.0,
            font_size: 14.0,
            margin: 0.0,
            color: "#FF4C8BF5".to_owned(),
            is_visible: true,
            is_enabled: true,
            opacity: 1.0,
            tooltip: String::new(),
            parent: None,
            event_name: Some("Loaded".to_owned()),
            font_family: String::new(),
            font_weight: String::new(),
            padding: 0.0,
            source_attributes: Vec::new(),
        }];
        fs::write(
            root.join("MainWindow.axaml"),
            render_xaml("Demo", &controls),
        )
        .expect("XAML written");
        fs::write(
            root.join("MainWindow.axaml.cs"),
            "using Avalonia.Controls;\nnamespace CairnApp;\npublic partial class MainWindow : Window {\n    private void DatePicker12_Loaded(object? sender, Avalonia.Input.PointerPressedEventArgs e)\n    {\n    }\n}\n",
        )
        .expect("stale code-behind written");

        super::repair_stale_avalonia_event_handlers(&project).expect("stale handler repaired");
        let code = fs::read_to_string(root.join("MainWindow.axaml.cs")).unwrap();
        assert!(code.contains(
            "DatePicker12_Loaded(object? sender, Avalonia.Interactivity.RoutedEventArgs e)"
        ));
        assert!(!code.contains("Avalonia.Input.PointerPressedEventArgs"));
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn generated_handler_is_merged_into_an_unsaved_code_buffer_once() {
        let handler = "    private void Button1_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)\n    {\n    }\n";
        let mut buffer =
            "namespace CairnApp;\npublic partial class MainWindow {\n    // unsaved edit\n}\n"
                .to_owned();
        assert!(super::merge_generated_handler(&mut buffer, handler).unwrap());
        assert!(buffer.contains("// unsaved edit"));
        assert!(buffer.contains("Button1_Click"));
        assert!(!super::merge_generated_handler(&mut buffer, handler).unwrap());
        assert_eq!(buffer.matches("Button1_Click(").count(), 1);
    }

    #[test]
    fn avalonia_designer_does_not_replace_a_file_changed_outside_the_designer() {
        let root = temp_directory();
        let project = ProjectInfo {
            name: "Demo".to_owned(),
            path: root.clone(),
            language: ProjectLanguage::CSharp,
            template: ProjectTemplate::AvaloniaApp,
            entry_file: PathBuf::from("MainWindow.axaml"),
            project_file: None,
            properties: Default::default(),
        };
        let path = root.join("MainWindow.axaml");
        let original = "<Window custom=\"source\" />\n";
        fs::write(&path, original).expect("initial XAML written");
        let mut designer = super::WindowDesigner {
            project_path: Some(root.clone()),
            source_xaml: Some(original.to_owned()),
            unsupported_xaml: false,
            ..Default::default()
        };
        let external_edit = "<Window custom=\"user edit\" />\n";
        fs::write(&path, external_edit).expect("external edit written");

        designer.save_project(&project);

        assert_eq!(fs::read_to_string(path).unwrap(), external_edit);
        assert!(
            designer
                .error
                .as_deref()
                .is_some_and(|error| error.contains("changed outside the designer"))
        );
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn qt_layout_round_trips_controls_and_generates_signal_connections() {
        let controls = vec![DesignerControl {
            kind: ControlKind::Button,
            name: "SaveButton".to_owned(),
            text: "Save \"now\"".to_owned(),
            x: 48.0,
            y: 96.0,
            width: 120.0,
            height: 38.0,
            font_size: 14.0,
            margin: 0.0,
            color: "#FF4C8BF5".to_owned(),
            is_visible: true,
            is_enabled: true,
            opacity: 1.0,
            tooltip: String::new(),
            parent: None,
            event_name: Some("Click".to_owned()),
            font_family: String::new(),
            font_weight: String::new(),
            padding: 0.0,
            source_attributes: Vec::new(),
        }];
        let layout = render_qt_layout("Demo", &controls);
        assert_eq!(parse_qt_controls(&layout).len(), 1);

        let project = ProjectInfo {
            name: "Demo".to_owned(),
            path: PathBuf::new(),
            language: ProjectLanguage::Cpp,
            template: ProjectTemplate::QtWidgetsApp,
            entry_file: PathBuf::from("src/MainWindow.cpp"),
            project_file: None,
            properties: Default::default(),
        };
        let generated = render_qt_ui_code(&project, &controls);
        assert!(generated.contains("QPushButton(\"Save \\\"now\\\"\", this)"));
        assert!(generated.contains("&MainWindow::SaveButton_Click"));
        assert!(generated.contains("SaveButton = new QPushButton("));
        assert!(super::render_qt_members(&controls).contains("QPushButton* SaveButton = nullptr;"));
    }

    #[test]
    fn qt_designer_preserves_user_code_outside_generated_region_and_wires_handlers() {
        let root = temp_directory();
        let project = ProjectInfo {
            name: "Demo".to_owned(),
            path: root.clone(),
            language: ProjectLanguage::Cpp,
            template: ProjectTemplate::QtWidgetsApp,
            entry_file: PathBuf::from("src/MainWindow.cpp"),
            project_file: None,
            properties: Default::default(),
        };
        fs::create_dir_all(root.join("src")).expect("source directory created");
        fs::write(
            root.join("src/MainWindow.h"),
            "#pragma once\n#include <QWidget>\nclass MainWindow : public QWidget {\n    Q_OBJECT\npublic:\n    explicit MainWindow(QWidget* parent = nullptr);\n};\n",
        )
        .expect("header created");
        fs::write(
            root.join("src/MainWindow.cpp"),
            "#include \"MainWindow.h\"\n#include <QPushButton>\nMainWindow::MainWindow(QWidget* parent) : QWidget(parent) {\n    // <cairn-generated-ui>\n    // </cairn-generated-ui>\n}\n\nvoid customFunction() { }\n",
        )
        .expect("source created");
        super::save_qt_project(
            &project,
            &[DesignerControl {
                kind: ControlKind::Button,
                name: "Button1".to_owned(),
                text: "Click".to_owned(),
                x: 36.0,
                y: 92.0,
                width: 120.0,
                height: 38.0,
                font_size: 14.0,
                margin: 0.0,
                color: "#FF4C8BF5".to_owned(),
                is_visible: true,
                is_enabled: true,
                opacity: 1.0,
                tooltip: String::new(),
                parent: None,
                event_name: Some("Click".to_owned()),
                font_family: String::new(),
                font_weight: String::new(),
                padding: 0.0,
                source_attributes: Vec::new(),
            }],
        )
        .expect("Qt design saved");
        let source = fs::read_to_string(root.join("src/MainWindow.cpp")).unwrap();
        let header = fs::read_to_string(root.join("src/MainWindow.h")).unwrap();
        assert!(source.contains("void MainWindow::Button1_Click()"));
        assert!(source.contains("void customFunction()"));
        assert!(header.contains("void Button1_Click();"));
        assert!(header.contains("QPushButton* Button1 = nullptr;"));
        assert!(source.contains("Button1 = new QPushButton(\"Click\", this);"));
        fs::remove_dir_all(root).expect("temporary directory removed");
    }
}
