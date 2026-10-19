mod app;
mod config;
mod formatting;
mod language_server;
mod process;
mod project;
mod settings;
mod theme;
mod toolchain;

use eframe::egui;
use std::time::{Duration, Instant};
use std::{ffi::OsStr, path::PathBuf};

const STARTUP_SPLASH_ARG: &str = "--fugh-startup-splash";
// These IDs must match their Linux desktop-entry filenames for Wayland.
#[cfg(target_os = "linux")]
const APPLICATION_ID: &str = "io.github.pilotrl.fugh";
#[cfg(not(target_os = "linux"))]
const APPLICATION_ID: &str = "Fugh";
#[cfg(target_os = "linux")]
const SPLASH_APPLICATION_ID: &str = "io.github.pilotrl.fughsplash";
#[cfg(not(target_os = "linux"))]
const SPLASH_APPLICATION_ID: &str = "fugh-startup-splash";
const SPLASH_INITIAL_SIZE: [f32; 2] = [480.0, 288.48];

struct StartupSplash {
    texture: egui::TextureHandle,
    started: Instant,
}

fn application_icon() -> egui::IconData {
    let source = image::load_from_memory(include_bytes!("../assets/devicon/fugh-icon.png"))
        .expect("assets/devicon/fugh-icon.png must be a valid PNG")
        .into_rgba8();
    let width = source.width();
    let height = source.height();
    egui::IconData {
        rgba: source.into_raw(),
        width,
        height,
    }
}

fn startup_project_argument() -> Option<PathBuf> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if let Some(index) = arguments
        .iter()
        .position(|argument| argument == OsStr::new("--open-project"))
    {
        return arguments.get(index + 1).map(PathBuf::from);
    }
    arguments
        .into_iter()
        .find(|argument| argument != OsStr::new(STARTUP_SPLASH_ARG))
        .map(PathBuf::from)
}

#[cfg(target_os = "linux")]
fn migrate_linux_settings_directory() {
    let data_dir = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".local").join("share")));
    let Some(data_dir) = data_dir else {
        return;
    };
    let old_settings = data_dir.join("fugh");
    let new_settings = data_dir.join("io.github.pilotrl.fugh");
    if old_settings.is_dir()
        && !new_settings.exists()
        && let Err(error) = std::fs::rename(&old_settings, &new_settings)
    {
        eprintln!("Could not migrate Fugh settings directory: {error}");
    }
}

#[cfg(not(target_os = "linux"))]
fn migrate_linux_settings_directory() {}

impl StartupSplash {
    fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        let image = image::load_from_memory(include_bytes!("images/Begin.png"))
            .expect("src/images/Begin.png must be a valid PNG")
            .into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let texture = creation_context.egui_ctx.load_texture(
            "startup-splash",
            egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        Self {
            texture,
            started: Instant::now(),
        }
    }
}

impl eframe::App for StartupSplash {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::BLACK)
                    .inner_margin(0.0),
            )
            .show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((
                        self.texture.id(),
                        egui::vec2(SPLASH_INITIAL_SIZE[0], SPLASH_INITIAL_SIZE[1]),
                    )));
                });
            });

        let remaining = Duration::from_millis(1400).saturating_sub(self.started.elapsed());
        if remaining.is_zero() || ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint_after(remaining);
        }
    }
}

pub(crate) fn crash_log_path() -> std::path::PathBuf {
    std::env::temp_dir().join("cairn-crash.log")
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        let report = format!(
            "Fugh IDE panic: {info}\n{}\n\n",
            std::backtrace::Backtrace::force_capture()
        );
        eprintln!("{report}");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(crash_log_path())
        {
            let _ = file.write_all(report.as_bytes());
        }
    }));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_panic_hook();

    if std::env::args().any(|argument| argument == STARTUP_SPLASH_ARG) {
        let splash_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                // Use a dedicated app ID so the compositor does not reuse the
                // editor window's size and placement for the splash.
                .with_app_id(SPLASH_APPLICATION_ID)
                .with_title("Fugh")
                .with_icon(std::sync::Arc::new(application_icon()))
                .with_inner_size(SPLASH_INITIAL_SIZE)
                .with_decorations(false)
                .with_resizable(false),
            persist_window: false,
            centered: true,
            ..Default::default()
        };
        eframe::run_native(
            "Fugh Startup Splash",
            splash_options,
            Box::new(|creation_context| Ok(Box::new(StartupSplash::new(creation_context)))),
        )?;
        return Ok(());
    }

    migrate_linux_settings_directory();

    // Run the splash in a separate process so it receives its own native
    // window and cannot inherit the IDE window's saved size or decorations.
    let executable = std::env::current_exe()?;
    let splash_status = std::process::Command::new(executable)
        .arg(STARTUP_SPLASH_ARG)
        .status();
    match splash_status {
        Ok(status) if !status.success() => {
            eprintln!("Startup splash exited with status {status}");
        }
        Err(error) => eprintln!("Could not show startup splash; opening Fugh anyway: {error}"),
        Ok(_) => {}
    }

    let startup_project = startup_project_argument();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            // Keep the original persistence identifier so existing user
            // settings continue to load after adding desktop integration.
            .with_app_id(APPLICATION_ID)
            .with_icon(std::sync::Arc::new(application_icon()))
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Fugh",
        options,
        Box::new(move |creation_context| {
            let mut app = app::IdeApp::new(creation_context);
            if let Some(project_path) = startup_project.as_deref() {
                app.open_project(project_path);
            }
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}
