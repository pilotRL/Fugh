//! The Settings page. Every control here edits a value that the rest of the
//! IDE actually reads (see `crate::settings` and `crate::theme`).

use eframe::egui::{self, Color32, RichText};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

use crate::{
    app::IdeApp,
    settings::{
        AccentChoice, Action, CppBraceStyle, CppFormatStyle, CustomThemeColors, EditorFontFamily,
        InterfaceDensity, KeyBinding, MAX_FONT_SIZE, MIN_FONT_SIZE, ThemeChoice, UiFontFamily,
        UserSettings,
    },
    theme,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Category {
    #[default]
    General,
    Appearance,
    Editor,
    Formatting,
    Keyboard,
    Tools,
}

impl Category {
    const ALL: [Self; 6] = [
        Self::General,
        Self::Appearance,
        Self::Editor,
        Self::Formatting,
        Self::Keyboard,
        Self::Tools,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Editor => "Editor",
            Self::Formatting => "Formatting",
            Self::Keyboard => "Keyboard",
            Self::Tools => "Build & Tools",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::General => "Project defaults, startup, and safety",
            Self::Appearance => "Theme, colors, and interface layout",
            Self::Editor => "Code editor behavior and typography",
            Self::Formatting => "Indentation and formatter preferences",
            Self::Keyboard => "Keyboard shortcuts and commands",
            Self::Tools => "Compilers, SDKs, and build tools",
        }
    }
}

#[derive(Debug, Default, Clone)]
struct PageState {
    category: Category,
    query: String,
    confirm_reset: bool,
    capturing: Option<Action>,
    message: Option<(bool, String)>,
    location_text: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ThemeFile {
    format_version: u32,
    appearance: crate::settings::AppearanceSettings,
    syntax: CodeTheme,
}

const UI_SCALE_PRESETS: [f32; 8] = [0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

pub(super) fn draw_settings(app: &mut IdeApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let state_id = egui::Id::unique("cairn.settings-page-state");
    let mut state: PageState = ctx
        .data_mut(|data| data.get_temp::<PageState>(state_id))
        .unwrap_or_default();

    ui.add_space(12.0);
    if ui.available_width() < 620.0 {
        ui.vertical(|ui| {
            ui.heading("Settings");
            ui.horizontal(|ui| {
                ui.add_sized(
                    [ui.available_width().min(420.0).max(160.0), 28.0],
                    egui::TextEdit::singleline(&mut state.query).hint_text("Search settings"),
                );
                if !state.query.is_empty() && ui.button("Clear").clicked() {
                    state.query.clear();
                }
            });
        });
    } else {
        ui.horizontal(|ui| {
            ui.heading("Settings");
            ui.add_space(16.0);
            ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text("Search settings")
                    .desired_width(280.0),
            );
            if !state.query.is_empty() && ui.button("Clear").clicked() {
                state.query.clear();
            }
        });
    }
    ui.label(
        RichText::new("Changes are saved automatically after you edit a setting.")
            .small()
            .color(theme::muted()),
    );
    if let Some(path) = super::user_settings_file_path() {
        ui.label(
            RichText::new(format!("Preferences file: {}", path.display()))
                .small()
                .color(theme::muted()),
        );
    }
    if let Some(path) = super::formatting_settings_file_path() {
        ui.label(
            RichText::new(format!("Formatting file: {}", path.display()))
                .small()
                .color(theme::muted()),
        );
    }
    if app.status_message.starts_with("Could not save")
        || app
            .status_message
            .starts_with("Could not load saved settings")
        || app
            .status_message
            .starts_with("Could not load formatting preferences")
    {
        ui.colored_label(Color32::LIGHT_RED, &app.status_message);
    } else if app.status_message == "Settings saved" {
        ui.colored_label(Color32::from_rgb(133, 202, 156), &app.status_message);
    }
    ui.add_space(6.0);

    let searching = !state.query.trim().is_empty();
    let query = state.query.trim().to_lowercase();
    let selected = state.category;
    let visible = |category: Category, title: &str, keywords: &str| -> bool {
        if query.is_empty() {
            category == selected
        } else {
            title.to_lowercase().contains(&query)
                || keywords.to_lowercase().contains(&query)
                || category.label().to_lowercase().contains(&query)
        }
    };

    let mut user: UserSettings = app.settings.user.clone();
    let mut shown_any = false;

    let navigation_width = ui.available_width().clamp(120.0, 160.0);
    egui::Panel::left("settings-category-navigation")
        .exact_size(navigation_width)
        .resizable(false)
        .frame(egui::Frame::new().fill(theme::panel()).inner_margin(12.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new("SETTINGS SECTIONS")
                    .small()
                    .strong()
                    .color(theme::muted()),
            );
            ui.label(
                RichText::new("Choose a section to adjust Fugh.")
                    .small()
                    .color(theme::muted()),
            );
            ui.add_space(12.0);
            for category in Category::ALL {
                let clicked = ui
                    .add_enabled_ui(!searching, |ui| {
                        ui.add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::selectable(state.category == category, category.label()),
                        )
                        .clicked()
                    })
                    .inner;
                if clicked {
                    state.category = category;
                }
                ui.add_space(3.0);
            }
            if searching {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Search results include every category.")
                        .small()
                        .color(theme::muted()),
                );
            }
        });
    egui::Panel::left("settings-content-gutter")
        .exact_size(14.0)
        .resizable(false)
        .frame(egui::Frame::new().fill(ui.visuals().window_fill))
        .show(ui, |_| {});
    ui.separator();

    egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .inner_margin(egui::Margin {
            left: 22,
            right: 20,
            top: 14,
            bottom: 18,
        })
        .show(ui, |ui| {
            ui.heading(if searching {
                "Search results"
            } else {
                selected.label()
            });
            ui.label(
                RichText::new(if searching {
                    "Matching settings from every section"
                } else {
                    selected.description()
                })
                .small()
                .color(theme::muted()),
            );
            ui.add_space(12.0);
            egui::ScrollArea::vertical()
                .id_salt("settings-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
            // ---- General -------------------------------------------------
            if visible(
                Category::General,
                "IDE language",
                "language localization interface",
            ) {
                shown_any = true;
                setting_row(ui, searching, Category::General, "IDE language", "English is the only interface language currently available.");
                ui.label("English");
                ui.add_space(10.0);
            }
            if visible(
                Category::General,
                "Default project location",
                "folder directory new project wizard path",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::General,
                    "Default project location",
                    "Folder offered first in the New Project wizard.",
                );
                draw_location(ui, &mut state, &mut user);
                ui.add_space(10.0);
            }
            if visible(
                Category::General,
                "Restore default settings",
                "reset defaults factory",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::General,
                    "Restore default settings",
                    "Resets every setting on this page, including keyboard shortcuts.",
                );
                if state.confirm_reset {
                    ui.horizontal(|ui| {
                        ui.label("Reset all settings?");
                        if ui.button("Yes, reset").clicked() {
                            user.restore_defaults();
                            app.windows.set_editor_layout(
                                crate::app::layout::EditorLayout::default(),
                            );
                            state.location_text = None;
                            state.confirm_reset = false;
                            state.message = Some((true, "Settings restored to defaults.".to_owned()));
                        }
                        if ui.button("Cancel").clicked() {
                            state.confirm_reset = false;
                        }
                    });
                } else if ui.button("Restore defaults…").clicked() {
                    state.confirm_reset = true;
                }
                ui.add_space(10.0);
            }
            if visible(
                Category::General,
                "Startup and autosave",
                "startup reopen last project welcome screen recent projects restore open files window layout autosave recovery crash encoding utf-8 line endings lf crlf terminal shell browser file manager update proxy notifications accessibility confirmation delete unsaved import export settings backup",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::General,
                    "Startup and autosave",
                    "Recent projects are saved with workspace state. Autosave writes dirty files to disk.",
                );
                ui.checkbox(
                    &mut user.general.reopen_last_project,
                    "Reopen the last project at startup",
                );
                ui.checkbox(&mut user.general.show_welcome_screen, "Show Welcome screen at startup");
                ui.checkbox(&mut user.general.show_recent_projects, "Show recent projects on Welcome");
                ui.horizontal(|ui| {
                    ui.label("Recent project limit");
                    ui.add(egui::DragValue::new(&mut user.general.recent_projects_count).range(1..=30));
                });
                ui.checkbox(&mut user.general.restore_open_files, "Restore open files at startup");
                ui.checkbox(&mut user.general.restore_window_layout, "Restore the last window layout");
                ui.checkbox(&mut user.general.autosave, "Autosave changed files");
                ui.horizontal(|ui| {
                    ui.label("Autosave interval (minutes)");
                    ui.add(
                        egui::DragValue::new(&mut user.general.autosave_interval_minutes)
                            .range(1..=60),
                    );
                });
                ui.checkbox(&mut user.general.crash_recovery, "Recover unsaved files after a crash");
                ui.add_space(8.0);
                setting_row(ui, searching, Category::General, "Files and terminal", "Defaults used by file saves and process launch. UTF-8 is the supported encoding.");
                ui.horizontal(|ui| {
                    ui.label("Encoding");
                    ui.label("UTF-8");
                    ui.label("Line endings");
                    egui::ComboBox::from_id_salt("settings-line-ending")
                        .selected_text(match user.general.line_ending { crate::settings::LineEnding::Lf => "LF", crate::settings::LineEnding::CrLf => "CRLF" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut user.general.line_ending, crate::settings::LineEnding::Lf, "LF");
                            ui.selectable_value(&mut user.general.line_ending, crate::settings::LineEnding::CrLf, "CRLF");
                        });
                });
                ui.label(RichText::new("UTF-8 is currently the only supported file encoding.").small().color(theme::muted()));
                ui.horizontal(|ui| {
                    ui.label("Terminal command");
                    ui.add(egui::TextEdit::singleline(&mut user.general.terminal).hint_text("Auto-detect terminal").desired_width(230.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Shell");
                    ui.add(egui::TextEdit::singleline(&mut user.general.shell).hint_text("Default shell").desired_width(230.0));
                });
                ui.add_space(8.0);
                setting_row(ui, searching, Category::General, "Safety and notifications", "Control confirmations before destructive actions and transient notifications.");
                ui.checkbox(&mut user.general.confirm_close_unsaved, "Confirm before closing unsaved files");
                ui.checkbox(&mut user.general.confirm_delete, "Confirm before deleting files");
                ui.checkbox(&mut user.general.notifications, "Show IDE notifications");
                ui.add_space(8.0);
                setting_row(ui, searching, Category::General, "External applications and updates", "Custom commands are applied when the corresponding application is launched.");
                ui.horizontal(|ui| { ui.label("External browser"); ui.add(egui::TextEdit::singleline(&mut user.general.external_browser).hint_text("System default").desired_width(230.0)); });
                ui.horizontal(|ui| { ui.label("File manager"); ui.add(egui::TextEdit::singleline(&mut user.general.file_manager).hint_text("System default").desired_width(230.0)); });
                ui.label(RichText::new("IDE update checking and proxy configuration are not implemented. Toolchain package updates remain separate.").small().color(theme::muted()));
                ui.label(RichText::new("This build does not send telemetry, so there is no telemetry preference to configure.").small().color(theme::muted()));
                ui.horizontal(|ui| {
                    if ui.button("Test browser").clicked() {
                        state.message = Some(match launch_external(&user.general.external_browser, "https://www.rust-lang.org") {
                            Ok(()) => (true, "Opened the test page in the selected browser.".to_owned()),
                            Err(error) => (false, error),
                        });
                    }
                    let has_project = app.current_project().is_some();
                    if ui.add_enabled(has_project, egui::Button::new("Open project folder")).clicked()
                        && let Some(project) = app.current_project() {
                        state.message = Some(match launch_external(&user.general.file_manager, &project.path.display().to_string()) {
                            Ok(()) => (true, "Opened the project folder.".to_owned()),
                            Err(error) => (false, error),
                        });
                    }
                });
                ui.label(RichText::new("Accessibility: interface scaling and UI font size are available under Appearance; editor font size is under Editor. High-contrast and screen-reader modes are not supported by the current UI toolkit integration.").small().color(theme::muted()));
                ui.add_space(10.0);
                setting_row(ui, searching, Category::General, "Settings backup", "Export or import your personal preferences as a JSON file.");
                ui.horizontal(|ui| {
                    if ui.button("Export settings…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Fugh settings", &["json"]).set_file_name("fugh-settings.json").save_file() {
                            state.message = match serde_json::to_vec_pretty(&user).and_then(|bytes| std::fs::write(&path, bytes).map_err(serde_json::Error::io)) { Ok(()) => Some((true, format!("Settings exported to {}", path.display()))), Err(error) => Some((false, format!("Could not export settings: {error}"))) };
                        }
                    }
                    if ui.button("Import settings…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Fugh settings", &["json"]).pick_file() {
                            state.message = match std::fs::read(&path).map_err(|error| error.to_string()).and_then(|bytes| serde_json::from_slice::<UserSettings>(&bytes).map_err(|error| error.to_string())) {
                                Ok(mut imported) => { imported.sanitize(); user = imported; Some((true, "Settings imported. Changes are saved with the application.".to_owned())) },
                                Err(error) => Some((false, format!("Could not import settings: {error}"))),
                            };
                        }
                    }
                });
                ui.add_space(10.0);
            }

            // ---- Appearance ---------------------------------------------
            if visible(
                Category::Appearance,
                "Theme",
                "dark light colors palette",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Appearance,
                    "Theme",
                    "Changes the full interface palette immediately.",
                );
                egui::ComboBox::from_id_salt("settings-theme")
                    .selected_text(user.appearance.theme.label())
                    .show_ui(ui, |ui| {
                        for choice in ThemeChoice::ALL {
                            ui.selectable_value(
                                &mut user.appearance.theme,
                                choice,
                                choice.label(),
                            );
                        }
                    });
                ui.horizontal(|ui| {
                    for choice in ThemeChoice::ALL {
                        let fill = match choice {
                            ThemeChoice::CairnDark => Color32::from_rgb(31, 31, 31),
                            ThemeChoice::Graphite => Color32::from_rgb(25, 30, 38),
                            ThemeChoice::Light => Color32::from_rgb(239, 242, 247),
                            ThemeChoice::Dracula => Color32::from_rgb(40, 42, 54),
                            ThemeChoice::Nord => Color32::from_rgb(46, 52, 64),
                            ThemeChoice::SolarizedDark => Color32::from_rgb(0, 43, 54),
                            ThemeChoice::Monokai => Color32::from_rgb(39, 40, 34),
                            ThemeChoice::System => theme::bg(),
                        };
                        egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, theme::line()))
                            .inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                                ui.label(RichText::new(choice.label()).color(if choice == ThemeChoice::Light { Color32::BLACK } else { Color32::WHITE }));
                            });
                    }
                });
                ui.add_space(10.0);
            }
            if visible(
                Category::Appearance,
                "Accent colour",
                "accent color highlight selection",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Appearance,
                    "Accent colour",
                    "Used for selection, active controls, the status bar and the designer.",
                );
                ui.horizontal(|ui| {
                    for choice in AccentChoice::ALL {
                        let [r, g, b] = choice.rgb();
                        ui.selectable_value(
                            &mut user.appearance.accent,
                            choice,
                            RichText::new(choice.label()).color(Color32::from_rgb(r, g, b)),
                        );
                    }
                });
                ui.add_space(10.0);
            }
            if visible(
                Category::Appearance,
                "Interface scale",
                "ui scaling zoom size density dpi",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Appearance,
                    "Interface scale",
                    "Scales the whole window.",
                );
                egui::ComboBox::from_id_salt("settings-ui-scale")
                    .selected_text(format!("{:.0}%", user.appearance.ui_scale * 100.0))
                    .show_ui(ui, |ui| {
                        for preset in UI_SCALE_PRESETS {
                            ui.selectable_value(
                                &mut user.appearance.ui_scale,
                                preset,
                                format!("{:.0}%", preset * 100.0),
                            );
                        }
                    });
                ui.add_space(10.0);
            }
            if visible(
                Category::Appearance,
                "Interface font size",
                "text menu labels typography",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Appearance,
                    "Interface font size",
                    "Size of menus, labels and buttons, in points.",
                );
                ui.add(
                    egui::DragValue::new(&mut user.appearance.ui_font_size)
                        .range(MIN_FONT_SIZE..=MAX_FONT_SIZE)
                        .speed(0.1)
                        .max_decimals(1),
                );
                ui.add_space(10.0);
            }
            if visible(Category::Appearance, "Typography and density", "font family line height weight compact comfortable density") {
                shown_any = true;
                setting_row(ui, searching, Category::Appearance, "Typography and density", "Choose available built-in font families, code line spacing, and interface spacing.");
                ui.horizontal(|ui| {
                    ui.label("Interface font");
                    egui::ComboBox::from_id_salt("ui-font-family").selected_text(match user.appearance.ui_font_family { UiFontFamily::Proportional => "Proportional", UiFontFamily::Monospace => "Monospace" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut user.appearance.ui_font_family, UiFontFamily::Proportional, "Proportional");
                        ui.selectable_value(&mut user.appearance.ui_font_family, UiFontFamily::Monospace, "Monospace");
                    });
                    ui.label("Editor font");
                    egui::ComboBox::from_id_salt("editor-font-family").selected_text(match user.editor.font_family { EditorFontFamily::Monospace => "Monospace", EditorFontFamily::Proportional => "Proportional" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut user.editor.font_family, EditorFontFamily::Monospace, "Monospace");
                        ui.selectable_value(&mut user.editor.font_family, EditorFontFamily::Proportional, "Proportional");
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Editor line height");
                    ui.add(egui::Slider::new(&mut user.editor.line_height, 0.0..=12.0).suffix(" pt"));
                    ui.selectable_value(&mut user.appearance.density, InterfaceDensity::Compact, "Compact");
                    ui.selectable_value(&mut user.appearance.density, InterfaceDensity::Comfortable, "Comfortable");
                });
                ui.label(RichText::new("Font weight and installing custom font files are not supported by the current font setup.").small().color(theme::muted()));
            }
            if visible(Category::Appearance, "Theme colors", "custom interface background foreground editor selection cursor syntax colors keyword string comment type function variable number import export preview toolbar status tabs minimap icon theme") {
                shown_any = true;
                setting_row(ui, searching, Category::Appearance, "Theme colors", "Custom semantic interface colors apply throughout the application. Syntax schemes use the available Syntect themes.");
                let mut enabled = user.appearance.custom_colors.is_some();
                if ui.checkbox(&mut enabled, "Use custom interface colors").changed() {
                    user.appearance.custom_colors = enabled.then(|| CustomThemeColors {
                        accent: rgb24(theme::accent()),
                        background: rgb24(theme::bg()), panel: rgb24(theme::panel()),
                        card: rgb24(theme::card()), hover: rgb24(theme::hover()), border: rgb24(theme::line()),
                        editor_background: rgb24(theme::editor()), foreground: rgb24(theme::text()),
                        muted: rgb24(theme::muted()), selection: rgb24(theme::accent()), cursor: rgb24(theme::cursor()),
                    });
                }
                if let Some(colors) = user.appearance.custom_colors.as_mut() {
                    color_setting(ui, "Accent", &mut colors.accent);
                    color_setting(ui, "Application background", &mut colors.background);
                    color_setting(ui, "Panels", &mut colors.panel);
                    color_setting(ui, "Cards", &mut colors.card);
                    color_setting(ui, "Hover", &mut colors.hover);
                    color_setting(ui, "Borders", &mut colors.border);
                    color_setting(ui, "Editor background", &mut colors.editor_background);
                    color_setting(ui, "Interface and editor foreground", &mut colors.foreground);
                    color_setting(ui, "Muted text", &mut colors.muted);
                    color_setting(ui, "Selection", &mut colors.selection);
                    color_setting(ui, "Text cursor", &mut colors.cursor);
                }
                ui.checkbox(&mut user.appearance.toolbar_visible, "Show editor toolbar");
                ui.checkbox(&mut user.appearance.status_bar_visible, "Show status bar");
                ui.checkbox(&mut user.appearance.tabs_rounded, "Rounded editor tabs");
                ui.horizontal(|ui| {
                    let mut syntax = CodeTheme::from_memory(&ctx, ui.style()).with_font_id(
                        egui::FontId::new(user.editor.font_size, match user.editor.font_family {
                            EditorFontFamily::Monospace => egui::FontFamily::Monospace,
                            EditorFontFamily::Proportional => egui::FontFamily::Proportional,
                        }),
                    );
                    ui.vertical(|ui| {
                        ui.strong("Syntax color scheme");
                        syntax.ui(ui);
                        syntax.store_in_memory(&ctx);
                    });
                });
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.strong("Live theme preview");
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("keyword").color(theme::accent()));
                        ui.label(RichText::new("\"string\"").color(Color32::from_rgb(120, 190, 150)));
                        ui.label(RichText::new("// comment").color(theme::muted()));
                        ui.label(RichText::new("Function() ").color(theme::text()));
                    });
                    ui.label("Theme selections apply live. Preset swatches above preview base palettes.");
                });
                ui.horizontal(|ui| {
                    if ui.button("Restore default colors").clicked() {
                        user.appearance.custom_colors = None;
                        user.appearance.accent = AccentChoice::default();
                        let code = if ui.style().visuals.dark_mode { CodeTheme::dark(user.editor.font_size) } else { CodeTheme::light(user.editor.font_size) };
                        code.store_in_memory(&ctx);
                    }
                    if ui.button("Export theme…").on_hover_text("Save a shareable Fugh theme JSON with interface colors and syntax highlighting.").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Fugh theme", &["json"]).set_file_name("fugh-theme.json").save_file() {
                            let file = ThemeFile { format_version: 1, appearance: user.appearance.clone(), syntax: CodeTheme::from_memory(&ctx, ui.style()) };
                            state.message = Some(match serde_json::to_vec_pretty(&file).and_then(|bytes| std::fs::write(&path, bytes).map_err(serde_json::Error::io)) { Ok(()) => (true, format!("Theme exported to {}", path.display())), Err(error) => (false, format!("Could not export theme: {error}")) });
                        }
                    }
                    if ui.button("Import theme…").on_hover_text("Load a Fugh theme JSON shared by another user.").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Fugh theme", &["json"]).pick_file() {
                            state.message = match std::fs::read(&path).map_err(|error| error.to_string()).and_then(|bytes| serde_json::from_slice::<ThemeFile>(&bytes).map_err(|error| error.to_string())) {
                                Ok(file) if file.format_version == 1 => {
                                    let mut check = UserSettings::default(); check.appearance = file.appearance.clone(); check.sanitize();
                                    user.appearance = check.appearance; file.syntax.store_in_memory(&ctx);
                                    Some((true, "Theme imported and applied.".to_owned()))
                                },
                                Ok(_) => Some((false, "This theme file version is not supported.".to_owned())),
                                Err(error) => Some((false, format!("Could not import theme: {error}"))),
                            };
                        }
                    }
                });
                ui.label(RichText::new("Icon themes are not supported yet. Toggle the editor minimap from the editor toolbar. Tab, sidebar, and panel colors follow the shared theme tokens.").small().color(theme::muted()));
                ui.add_space(10.0);
            }
            if visible(
                Category::Appearance,
                "Panel visibility",
                "sidebar project guide output problems panels",
            ) {
                shown_any = true;
                let mut layout = app.windows.editor_layout_snapshot();
                let old_sidebar = layout.sidebar_visible;
                let old_output = layout.output_visible;
                setting_row(
                    ui,
                    searching,
                    Category::Appearance,
                    "Panel visibility",
                    "Changes which editor panels are shown.",
                );
                ui.checkbox(&mut layout.sidebar_visible, "Show project and guide sidebar");
                ui.checkbox(&mut layout.output_visible, "Show output and problems panel");
                ui.label(
                    RichText::new("Panel placement and tab placement are controlled by the workspace layout.")
                        .small()
                        .color(theme::muted()),
                );
                if layout.sidebar_visible != old_sidebar || layout.output_visible != old_output {
                    app.windows.set_editor_layout(layout);
                }
                ui.add_space(10.0);
            }

            // ---- Editor --------------------------------------------------
            if visible(
                Category::Editor,
                "Editor font size",
                "code text monospace typography",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Editor,
                    "Editor font size",
                    "Size of code in the editor. Use Ctrl+mouse wheel or Ctrl++/Ctrl+- over the editor to zoom; Ctrl+0 restores the default size.",
                );
                ui.add(
                    egui::DragValue::new(&mut user.editor.font_size)
                        .range(MIN_FONT_SIZE..=MAX_FONT_SIZE)
                        .speed(0.1)
                        .max_decimals(1),
                );
                ui.add_space(10.0);
            }
            if visible(
                Category::Editor,
                "Word wrap",
                "wrap lines long horizontal scroll",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Editor,
                    "Word wrap",
                    "Wrap long lines instead of scrolling horizontally.",
                );
                ui.checkbox(&mut user.editor.word_wrap, "Wrap long lines");
                ui.add_space(10.0);
            }
            if visible(
                Category::Editor,
                "Indentation",
                "tabs spaces tab width indent",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Editor,
                    "Indentation",
                    "Controls indentation inserted by automatic indentation.",
                );
                ui.checkbox(&mut user.formatting.use_tabs, "Use tabs for indentation");
                ui.horizontal(|ui| {
                    ui.label("Indent width");
                    ui.add(egui::DragValue::new(&mut user.formatting.indent_width).range(1..=8));
                });
                ui.add_space(10.0);
            }
            if visible(
                Category::Editor,
                "Editor behavior",
                "line numbers auto indentation brackets quotes parentheses",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Editor,
                    "Editor behavior",
                    "These options change editing behavior and the editor gutter.",
                );
                ui.checkbox(&mut user.editor.line_numbers, "Show line numbers");
                ui.checkbox(&mut user.editor.auto_indent, "Indent new lines automatically");
                ui.checkbox(
                    &mut user.editor.auto_pairs,
                    "Auto-close brackets, quotes, and parentheses",
                );
                ui.add_space(10.0);
            }
            if searching
                && [
                    "line height",
                    "font family",
                    "current line",
                    "whitespace",
                    "bracket highlight",
                    "cursor style",
                    "find and replace",
                    "recovery",
                    "notification",
                    "confirmation",
                ]
                .iter()
                .any(|term| query.contains(term))
            {
                shown_any = true;
                ui.label(
                    RichText::new("This editor component does not provide that feature yet.")
                        .small()
                        .color(theme::muted()),
                );
            }
            if searching && (query.contains("folding") || query.contains("minimap")) {
                shown_any = true;
                ui.label(
                    RichText::new("Folding is available from the editor gutter. Toggle the Minimap button above the editor to show or hide the overview strip.")
                        .small()
                        .color(theme::muted()),
                );
            }

            // ---- Formatting --------------------------------------------
            if visible(
                Category::Formatting,
                "Code formatting",
                "clang-format dotnet format style preset indentation spaces tabs braces line length format on save per-project editorconfig",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Formatting,
                    "C++ formatter style",
                    "Uses your clang-format settings. You can choose whether project config files override them.",
                );
                egui::ComboBox::from_id_salt("settings-cpp-format-style")
                    .selected_text(user.formatting.cpp_style.label())
                    .show_ui(ui, |ui| {
                        for style in CppFormatStyle::ALL {
                            ui.selectable_value(&mut user.formatting.cpp_style, style, style.label());
                        }
                    });
                setting_row(
                    ui,
                    searching,
                    Category::Formatting,
                    "C++ brace placement",
                    "Applied by clang-format when project config override is disabled.",
                );
                egui::ComboBox::from_id_salt("settings-cpp-brace-style")
                    .selected_text(user.formatting.cpp_braces.label())
                    .show_ui(ui, |ui| {
                        for style in CppBraceStyle::ALL {
                            ui.selectable_value(&mut user.formatting.cpp_braces, style, style.label());
                        }
                    });
                setting_row(
                    ui,
                    searching,
                    Category::Formatting,
                    "C# brace placement",
                    "Applied through temporary .editorconfig rules when project config override is disabled.",
                );
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut user.formatting.csharp_braces,
                        crate::settings::CSharpBraceStyle::SameLine,
                        "Same line",
                    );
                    ui.selectable_value(
                        &mut user.formatting.csharp_braces,
                        crate::settings::CSharpBraceStyle::Allman,
                        "Allman",
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Indent");
                    ui.add(egui::DragValue::new(&mut user.formatting.indent_width).range(1..=8));
                    ui.checkbox(&mut user.formatting.use_tabs, "Tabs");
                    ui.label("Line length");
                    ui.add(egui::DragValue::new(&mut user.formatting.line_length).range(40..=240));
                });
                ui.add_space(6.0);
                ui.checkbox(&mut user.formatting.format_on_save, "Apply formatter automatically when saving");
                ui.checkbox(
                    &mut user.formatting.respect_project_config,
                    "Prefer project .clang-format / .editorconfig rules",
                );
                ui.label("C++ requires clang-format. C# uses dotnet format from the installed .NET SDK.");
                ui.label("New lines use these indentation settings immediately. Existing code is reformatted automatically when you save or autosave if the option above is enabled.");
                ui.label("Project formatter files override these choices only when the project preference above is enabled.");
                ui.label("The arrows and dots below are preview markers; the editor hides whitespace symbols.");
                ui.add_space(8.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.strong("Formatting preview");
                    ui.label("Formatting choices apply to saved code. Whitespace markers are shown in the editor and preview.");
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut user.formatting.use_tabs, "Indent with tabs");
                        let mut cpp_allman = user.formatting.cpp_braces == CppBraceStyle::Allman;
                        if ui.checkbox(&mut cpp_allman, "C++ braces on new lines").changed() {
                            user.formatting.cpp_braces = if cpp_allman {
                                CppBraceStyle::Allman
                            } else {
                                CppBraceStyle::Attach
                            };
                        }
                        let mut cs_allman = user.formatting.csharp_braces
                            == crate::settings::CSharpBraceStyle::Allman;
                        if ui.checkbox(&mut cs_allman, "C# braces on new lines").changed() {
                            user.formatting.csharp_braces = if cs_allman {
                                crate::settings::CSharpBraceStyle::Allman
                            } else {
                                crate::settings::CSharpBraceStyle::SameLine
                            };
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut user.editor.show_whitespace_spaces, "Show spaces as ·");
                        ui.checkbox(&mut user.editor.show_whitespace_tabs, "Show tabs as →");
                        ui.checkbox(&mut user.editor.line_numbers, "Line numbers");
                    });
                    if !user.formatting.use_tabs {
                        ui.label(RichText::new("This sample uses spaces, so it has no tab characters to mark. Turn on Tabs above to see → in the preview.").small().color(theme::muted()));
                    }
                    ui.columns(2, |columns| {
                        columns[0].strong(format!("C++ · {}", user.formatting.cpp_style.label()));
                        draw_colored_preview(
                            &mut columns[0],
                            &cpp_format_preview(&user.formatting),
                            "cpp",
                            &user.editor,
                        );
                        columns[1].strong("C# · .NET format");
                        draw_colored_preview(
                            &mut columns[1],
                            &csharp_format_preview(&user.formatting),
                            "cs",
                            &user.editor,
                        );
                    });
                });
                ui.add_space(10.0);
            }

            // ---- Keyboard ------------------------------------------------
            if matches!(
                (searching, selected),
                (true, _) | (false, Category::Keyboard)
            ) {
                shown_any |= draw_keyboard(ui, &mut state, &mut user, &query, searching);
            }

            // ---- Build & tools ------------------------------------------
            if visible(
                Category::Tools,
                "Detected tools",
                "compiler sdk debugger language server cmake dotnet clangd clang-format gdb versions missing install",
            ) {
                shown_any = true;
                setting_row(
                    ui,
                    searching,
                    Category::Tools,
                    "Detected tools",
                    "Found on PATH. Custom tool paths are not configurable yet.",
                );
                draw_tools(app, ui);
                ui.label(
                    RichText::new("Custom tool paths and per-project build profiles are not configurable yet.")
                        .small()
                        .color(theme::muted()),
                );
                ui.add_space(10.0);
            }

            if !shown_any {
                ui.label(
                    RichText::new(format!("No settings match \"{}\".", state.query.trim()))
                        .color(theme::muted()),
                );
            }
                });
        });

    if let Some((ok, message)) = &state.message {
        ui.separator();
        ui.colored_label(
            if *ok {
                Color32::from_rgb(133, 202, 156)
            } else {
                Color32::LIGHT_RED
            },
            message,
        );
    }

    user.sanitize();
    if user != app.settings.user {
        let formatting_changed = user.formatting != app.settings.user.formatting;
        let formatting_save =
            formatting_changed.then(|| super::save_formatting_settings_file(&user.formatting));
        app.settings.user = user;
        let settings_save = super::save_user_settings_file(&app.settings.user);
        app.status_message = match (formatting_save, settings_save) {
            (Some(Err(error)), _) => {
                format!("Could not save formatting preferences: {error}")
            }
            (_, Err(error)) => format!("Could not save settings: {error}"),
            _ => "Settings saved".to_owned(),
        }
    }
    ctx.data_mut(|data| data.insert_temp(state_id, state));
}

fn cpp_format_preview(settings: &crate::settings::FormattingSettings) -> String {
    let unit = if settings.use_tabs {
        "\t".to_owned()
    } else {
        " ".repeat(settings.indent_width as usize)
    };
    let braces_new_line = settings.cpp_braces != CppBraceStyle::Attach;
    let open = if braces_new_line { "\n{" } else { " {" };
    let method_open = if braces_new_line {
        format!("\n{unit}{{")
    } else {
        " {".to_owned()
    };
    format!(
        "// {} style\nclass Greeter{open}\n{unit}public:\n{unit}void greet(){method_open}\n{unit}{unit}std::cout << \"Hello\";\n{unit}}}\n}};",
        settings.cpp_style.label()
    )
}

fn draw_colored_preview(
    ui: &mut egui::Ui,
    source: &str,
    language: &str,
    settings: &crate::settings::EditorSettings,
) {
    let mut visible = String::new();
    for ch in source.chars() {
        match ch {
            ' ' if settings.show_whitespace_spaces => visible.push('·'),
            '\t' if settings.show_whitespace_tabs => visible.push_str("→   "),
            _ => visible.push(ch),
        }
    }
    let display = if settings.line_numbers {
        visible
            .lines()
            .enumerate()
            .map(|(index, line)| format!("{:>2}  {line}", index + 1))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        visible
    };
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(egui::CornerRadius::same(5))
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt(("format-preview", language))
                .max_height(190.0)
                .show(ui, |ui| {
                    let theme = CodeTheme::from_memory(ui.ctx(), ui.style());
                    let job = highlight(ui.ctx(), ui.style(), &theme, &display, language);
                    ui.label(job);
                });
        });
}

fn csharp_format_preview(settings: &crate::settings::FormattingSettings) -> String {
    let unit = if settings.use_tabs {
        "\t".to_owned()
    } else {
        " ".repeat(settings.indent_width as usize)
    };
    let open = match settings.csharp_braces {
        crate::settings::CSharpBraceStyle::SameLine => " {".to_owned(),
        crate::settings::CSharpBraceStyle::Allman => "\n{".to_owned(),
    };
    let method_open = match settings.csharp_braces {
        crate::settings::CSharpBraceStyle::SameLine => " {".to_owned(),
        crate::settings::CSharpBraceStyle::Allman => format!("\n{unit}{{"),
    };
    format!(
        "class Greeter{open}\n{unit}void Greet(){method_open}\n{unit}{unit}Console.WriteLine(\"Hello\");\n{unit}}}\n}}"
    )
}

fn setting_row(
    ui: &mut egui::Ui,
    show_category: bool,
    category: Category,
    title: &str,
    help: &str,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        if show_category {
            ui.label(
                RichText::new(format!("· {}", category.label()))
                    .small()
                    .color(theme::muted()),
            );
        }
    });
    ui.label(RichText::new(help).small().color(theme::muted()));
    ui.add_space(10.0);
}

fn rgb24(color: Color32) -> u32 {
    (u32::from(color.r()) << 16) | (u32::from(color.g()) << 8) | u32::from(color.b())
}

fn color_setting(ui: &mut egui::Ui, label: &str, stored: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut color =
            Color32::from_rgb((*stored >> 16) as u8, (*stored >> 8) as u8, *stored as u8);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *stored = rgb24(color);
        }
    });
}

fn launch_external(override_command: &str, target: &str) -> Result<(), String> {
    let command = if override_command.trim().is_empty() {
        "xdg-open"
    } else {
        override_command.trim()
    };
    std::process::Command::new(command)
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not launch '{command}': {error}"))
}

fn draw_location(ui: &mut egui::Ui, state: &mut PageState, user: &mut UserSettings) {
    let mut text = state
        .location_text
        .clone()
        .unwrap_or_else(|| user.general.default_project_location.clone());
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("/home/you/Projects")
                    .desired_width(380.0),
            )
            .changed();
        if ui.button("Browse…").clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            text = path.display().to_string();
            changed = true;
        }
        if ui.button("Use default").clicked() {
            text.clear();
            changed = true;
        }
    });
    if changed || state.location_text.is_none() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            user.general.default_project_location.clear();
            state.message = None;
        } else if !std::path::Path::new(trimmed).is_absolute() {
            state.message = Some((
                false,
                "Project location must be an absolute path.".to_owned(),
            ));
        } else if !std::path::Path::new(trimmed).is_dir() {
            state.message = Some((false, format!("Folder does not exist: {trimmed}")));
        } else {
            user.general.default_project_location = trimmed.to_owned();
            state.message = None;
        }
        state.location_text = Some(text);
    }
}

fn draw_keyboard(
    ui: &mut egui::Ui,
    state: &mut PageState,
    user: &mut UserSettings,
    query: &str,
    searching: bool,
) -> bool {
    let mut any = false;
    let mut capture_event: Option<KeyBinding> = None;

    if let Some(action) = state.capturing {
        let (cancel, pressed) = ui.input(|input| {
            let cancel = input.key_pressed(egui::Key::Escape);
            let pressed = input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if *key != egui::Key::Escape => Some((*key, *modifiers)),
                _ => None,
            });
            (cancel, pressed)
        });
        if cancel {
            state.capturing = None;
        } else if let Some((key, modifiers)) = pressed {
            capture_event = Some(KeyBinding {
                action,
                ctrl: modifiers.ctrl,
                shift: modifiers.shift,
                alt: modifiers.alt,
                key: key.name().to_owned(),
            });
        }
    }
    if let Some(binding) = capture_event {
        state.capturing = None;
        let label = binding.action.label();
        let chord = binding.display();
        state.message = Some(match user.keymap.rebind(binding) {
            Ok(()) => (true, format!("{label} is now {chord}.")),
            Err(error) => (false, error),
        });
    }

    for action in Action::ALL {
        let chord = user.keymap.display_bindings(action);
        let matches_query = query.is_empty()
            || action.label().to_lowercase().contains(query)
            || chord.to_lowercase().contains(query)
            || "keyboard shortcut keybinding hotkey".contains(query);
        if !matches_query {
            continue;
        }
        any = true;
        ui.horizontal(|ui| {
            ui.add_sized([220.0, 22.0], egui::Label::new(action.label()));
            let capturing_this = state.capturing == Some(action);
            ui.label(
                RichText::new(if capturing_this {
                    "Press new shortcut… (Esc cancels)".to_owned()
                } else {
                    chord.clone()
                })
                .monospace(),
            );
            if !capturing_this && ui.button("Change").clicked() {
                state.capturing = Some(action);
                state.message = None;
            }
        });
    }
    if any {
        if searching {
            ui.label(RichText::new("Keyboard").small().color(theme::muted()));
        }
        if ui.button("Reset shortcuts to defaults").clicked() {
            user.keymap.reset();
            state.capturing = None;
            state.message = Some((true, "Shortcuts restored to defaults.".to_owned()));
        }
        ui.add_space(10.0);
    }
    any
}

fn draw_tools(app: &IdeApp, ui: &mut egui::Ui) {
    let toolchains = app.toolchain_manager();
    if toolchains.tools().is_empty() && !toolchains.is_refreshing() {
        toolchains.refresh(ui.ctx().clone());
    }
    if toolchains.is_refreshing() {
        ui.spinner();
    }
    for tool in toolchains.tools() {
        ui.horizontal(|ui| {
            ui.add_sized([160.0, 20.0], egui::Label::new(tool.name));
            match &tool.version {
                Some(version) => {
                    ui.label(RichText::new(version).color(Color32::from_rgb(133, 202, 156)));
                }
                None => {
                    ui.label(
                        RichText::new(format!(
                            "Missing — `{}` was not found on PATH",
                            tool.command
                        ))
                        .color(Color32::LIGHT_RED),
                    );
                }
            }
        });
    }
    if ui.button("Refresh detection").clicked() {
        toolchains.refresh(ui.ctx().clone());
    }
    ui.horizontal(|ui| {
        if ui.button("Install missing tools…").clicked() {
            app.open_toolchain_window();
        }
        if !crate::toolchain::ToolchainManager::supports_fedora_package_management() {
            ui.label("Automatic package installation is available on Fedora Linux.")
                .on_hover_text("Other systems can install tools with their package manager.");
        }
    });
}
