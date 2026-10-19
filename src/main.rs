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

const STARTUP_SPLASH_ARG: &str = "--fugh-startup-splash";
const SPLASH_INITIAL_SIZE: [f32; 2] = [480.0, 281.0];
const SPLASH_ASPECT_RATIO: f32 = 1024.0 / 600.0;
const SPLASH_MAX_WIDTH: f32 = 640.0;

struct StartupSplash {
    texture: egui::TextureHandle,
    started: Instant,
    display_size: egui::Vec2,
    size_adapted: bool,
}

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
            display_size: egui::vec2(SPLASH_INITIAL_SIZE[0], SPLASH_INITIAL_SIZE[1]),
            size_adapted: false,
        }
    }
}

impl eframe::App for StartupSplash {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.size_adapted
            && let Some(monitor_size) = ctx.input(|input| input.viewport().monitor_size)
        {
            // Size the splash to about a third of the active display width while
            // keeping it small on low-resolution screens and capped on large ones.
            let width = (monitor_size.x * 0.32)
                .min(SPLASH_MAX_WIDTH)
                .min(monitor_size.y * 0.45 * SPLASH_ASPECT_RATIO)
                .max(200.0);
            self.display_size = egui::vec2(width, width / SPLASH_ASPECT_RATIO);
            self.size_adapted = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.display_size));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::BLACK)
                    .inner_margin(0.0),
            )
            .show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((self.texture.id(), self.display_size)));
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
                .with_app_id("fugh-startup-splash")
                .with_title("Fugh")
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

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            // Keep the original persistence identifier so existing user
            // settings continue to load after adding desktop integration.
            .with_app_id("Fugh")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Fugh",
        options,
        Box::new(|creation_context| Ok(Box::new(app::IdeApp::new(creation_context)))),
    )?;
    Ok(())
}
