use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, RichText, Sense};

use crate::{app::IdeApp, config::WorkspacePage, theme};

/// Visual Studio style start window: recent projects on the left, big
/// action cards on the right. No bottom panels here.
pub(super) fn draw_start(app: &mut IdeApp, root_ui: &mut egui::Ui) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::bg())
                .inner_margin(egui::Margin::symmetric(48, 32)),
        )
        .show(root_ui, |ui| {
            ui.label(RichText::new("Fugh").size(28.0).color(theme::text()));
            ui.add_space(24.0);
            ui.columns(2, |columns| {
                recent_column(&mut columns[0], app);
                actions_column(&mut columns[1], app);
            });
        });
}

fn recent_column(ui: &mut egui::Ui, app: &mut IdeApp) {
    if !app.settings.user.general.show_recent_projects {
        ui.label(
            RichText::new("Recent projects are hidden. Change this in Settings → General.")
                .color(theme::muted()),
        );
        return;
    }
    ui.label(RichText::new("Open recent").size(20.0));
    ui.add_space(10.0);
    let search_id = ui.make_persistent_id(egui::IdSalt::new("start-recent-search"));
    let mut query: String = ui.data_mut(|d| d.get_temp(search_id)).unwrap_or_default();
    ui.add_sized(
        [ui.available_width().min(420.0), 30.0],
        egui::TextEdit::singleline(&mut query).hint_text("Search recent projects"),
    );
    ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
    ui.add_space(12.0);

    let needle = query.to_lowercase();
    let recent: Vec<_> = app
        .recent_projects()
        .into_iter()
        .filter(|project| project.name.to_lowercase().contains(&needle))
        .take(usize::from(app.settings.user.general.recent_projects_count))
        .collect();
    if recent.is_empty() {
        ui.label(
            RichText::new("Your recently opened projects will appear here.").color(theme::muted()),
        );
        return;
    }
    ui.label(RichText::new("Recent").small().color(theme::muted()));
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("recent-projects-scroll")
        .show(ui, |ui| {
            for project in recent {
                let path_text = project.path.display().to_string();
                if recent_row(ui, &project.name, &path_text, project.language.label()) {
                    app.open_project(&project.path);
                }
            }
        });
}

fn recent_row(ui: &mut egui::Ui, name: &str, path: &str, language: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 54.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::card());
    }
    let painter = ui.painter().with_clip_rect(rect);
    painter.text(
        rect.left_top() + egui::vec2(14.0, 9.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(15.0),
        theme::text(),
    );
    painter.text(
        rect.left_top() + egui::vec2(14.0, 31.0),
        Align2::LEFT_TOP,
        path,
        FontId::proportional(12.0),
        theme::muted(),
    );
    painter.text(
        rect.right_top() + egui::vec2(-14.0, 9.0),
        Align2::RIGHT_TOP,
        language,
        FontId::proportional(12.0),
        theme::accent_hover(),
    );
    response.clicked()
}

fn actions_column(ui: &mut egui::Ui, app: &mut IdeApp) {
    ui.label(RichText::new("Get started").size(20.0));
    ui.add_space(10.0);

    if card(
        ui,
        "＋",
        "Create a new project",
        "Choose a language and starter template.",
        false,
    ) {
        app.open_project_wizard();
    }
    if card(
        ui,
        "↗",
        "Open a project or folder",
        "Browse to a CMake or .NET project folder.",
        false,
    ) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            app.open_project(&path);
        }
    }
    if card(
        ui,
        "⇩",
        "Import an existing project",
        "Import a Visual Studio solution/project or a CMake project file.",
        false,
    ) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Fugh project", &["fugh"])
            .add_filter("Visual Studio solution", &["sln", "slnx"])
            .add_filter("Visual Studio project", &["csproj", "vcxproj"])
            .add_filter("CMake project file", &["txt"])
            .pick_file()
        {
            app.open_project(&path);
        }
    }
    if card(
        ui,
        "▶",
        "Continue without code",
        "Open the editor with an empty scratch file.",
        false,
    ) {
        app.settings.page = WorkspacePage::Workspace;
    }

    ui.add_space(4.0);
    if ui.link("Toolchain setup").clicked() {
        app.open_toolchain_window();
    }
    if ui.link("Settings").clicked() {
        app.open_settings();
    }
    if app.settings.user.general.notifications
        && app.status_message != "Welcome to Fugh"
        && !app.status_message.is_empty()
    {
        ui.add_space(8.0);
        ui.colored_label(Color32::LIGHT_RED, &app.status_message);
    }
}

/// A large clickable card with an accent bar on hover / selection.
pub(super) fn card(
    ui: &mut egui::Ui,
    glyph: &str,
    title: &str,
    description: &str,
    selected: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 74.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let active = response.hovered() || selected;
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(
        rect,
        CornerRadius::same(8),
        if active {
            theme::hover()
        } else {
            theme::card()
        },
    );
    if active {
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
            CornerRadius::same(2),
            theme::accent(),
        );
    }
    painter.text(
        egui::pos2(rect.left() + 34.0, rect.center().y),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(24.0),
        theme::accent_hover(),
    );
    painter.text(
        rect.left_top() + egui::vec2(72.0, 16.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(16.0),
        theme::text(),
    );
    painter.text(
        rect.left_top() + egui::vec2(72.0, 42.0),
        Align2::LEFT_TOP,
        description,
        FontId::proportional(12.0),
        theme::muted(),
    );
    ui.add_space(10.0);
    response.clicked()
}

/// Toolchains / Settings pages, shown in the same single window.
pub(super) fn draw_simple_page(app: &mut IdeApp, root_ui: &mut egui::Ui) {
    egui::Panel::top("simple_page_top")
        .exact_size(42.0)
        .show(root_ui, |ui| {
            ui.horizontal_centered(|ui| {
                if ui.button("←  Back").clicked() {
                    app.settings.page = if app.settings.page == WorkspacePage::Settings {
                        app.settings_return_page
                    } else {
                        if app.current_project().is_some() {
                            WorkspacePage::Workspace
                        } else {
                            WorkspacePage::Welcome
                        }
                    };
                }
                ui.label(RichText::new(app.settings.page.label()).strong());
            });
        });
    egui::CentralPanel::default().show(root_ui, |ui| match app.settings.page {
        WorkspacePage::Toolchains => draw_toolchains(app, ui),
        WorkspacePage::Settings => super::settings_ui::draw_settings(app, ui),
        _ => {}
    });
}

fn draw_toolchains(app: &IdeApp, ui: &mut egui::Ui) {
    let toolchains = app.toolchain_manager();
    if toolchains.tools().is_empty() {
        toolchains.refresh(ui.ctx().clone());
    }
    ui.add_space(18.0);
    ui.heading("Toolchains");
    ui.label("Detected from PATH on this computer:");
    if toolchains.is_refreshing() {
        ui.spinner();
    }
    for tool in toolchains.tools() {
        let value = tool.version.unwrap_or_else(|| "Missing".to_owned());
        ui.horizontal(|ui| {
            ui.label(tool.name);
            ui.label(RichText::new(&value).color(if value == "Missing" {
                Color32::LIGHT_RED
            } else {
                Color32::from_rgb(133, 202, 156)
            }));
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
            ui.label("Automatic installation is currently supported on Fedora Linux.")
                .on_hover_text("Other systems can install tools with their package manager.");
        }
    });
    ui.add_space(10.0);
    ui.label("Fedora packages can be installed manually with:");
    ui.code("sudo dnf install gcc-c++ cmake dotnet-sdk clang-tools-extra git gdb lldb");
}
