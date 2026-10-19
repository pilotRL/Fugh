use std::sync::{
    Mutex,
    atomic::{AtomicU8, AtomicU32, Ordering},
};

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

use crate::settings::{
    AccentChoice, AppearanceSettings, CustomThemeColors, EditorSettings, InterfaceDensity,
    ThemeChoice, UiFontFamily,
};

static THEME: AtomicU8 = AtomicU8::new(0);
static CUSTOM_COLORS: Mutex<Option<CustomThemeColors>> = Mutex::new(None);

pub fn custom_colors() -> Option<CustomThemeColors> {
    *CUSTOM_COLORS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn set_custom_colors(colors: Option<CustomThemeColors>) {
    *CUSTOM_COLORS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = colors;
}

fn custom(token: impl FnOnce(CustomThemeColors) -> u32) -> Option<Color32> {
    custom_colors().map(|colors| {
        let rgb = token(colors);
        Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    })
}

fn theme_index() -> u8 {
    THEME.load(Ordering::Relaxed)
}

fn themed_color(dark: [u8; 3], graphite: [u8; 3], light: [u8; 3], extra: [[u8; 3]; 4]) -> Color32 {
    let [r, g, b] = match theme_index() {
        1 => graphite,
        2 => light,
        index @ 3..=6 => extra[(index - 3) as usize],
        _ => dark,
    };
    Color32::from_rgb(r, g, b)
}

pub fn bg() -> Color32 {
    custom(|c| c.background).unwrap_or_else(|| {
        themed_color(
            [31, 31, 31],
            [25, 30, 38],
            [239, 242, 247],
            [[40, 42, 54], [46, 52, 64], [0, 43, 54], [39, 40, 34]],
        )
    })
}
pub fn bg_dark() -> Color32 {
    custom(|c| c.background).unwrap_or_else(|| {
        themed_color(
            [24, 24, 24],
            [18, 22, 29],
            [226, 231, 239],
            [[33, 34, 44], [36, 41, 52], [0, 35, 43], [30, 31, 28]],
        )
    })
}
pub fn panel() -> Color32 {
    custom(|c| c.panel).unwrap_or_else(|| {
        themed_color(
            [37, 37, 38],
            [34, 41, 52],
            [248, 249, 252],
            [[49, 51, 65], [59, 66, 82], [7, 54, 66], [49, 50, 43]],
        )
    })
}
pub fn card() -> Color32 {
    custom(|c| c.card).unwrap_or_else(|| {
        themed_color(
            [45, 45, 48],
            [43, 52, 66],
            [231, 235, 242],
            [[58, 60, 76], [67, 76, 94], [12, 62, 74], [58, 59, 50]],
        )
    })
}
pub fn hover() -> Color32 {
    custom(|c| c.hover).unwrap_or_else(|| {
        themed_color(
            [62, 62, 66],
            [57, 69, 87],
            [216, 224, 235],
            [[72, 74, 92], [79, 91, 111], [20, 70, 82], [72, 73, 62]],
        )
    })
}
pub fn line() -> Color32 {
    custom(|c| c.border).unwrap_or_else(|| {
        themed_color(
            [63, 63, 70],
            [71, 84, 103],
            [192, 201, 214],
            [[98, 100, 122], [76, 86, 106], [38, 88, 100], [95, 96, 81]],
        )
    })
}
pub fn editor() -> Color32 {
    custom(|c| c.editor_background).unwrap_or_else(|| {
        themed_color(
            [30, 30, 30],
            [22, 27, 35],
            [255, 255, 255],
            [[40, 42, 54], [46, 52, 64], [0, 43, 54], [39, 40, 34]],
        )
    })
}
pub fn text() -> Color32 {
    custom(|c| c.foreground).unwrap_or_else(|| {
        themed_color(
            [230, 230, 230],
            [226, 232, 240],
            [35, 43, 55],
            [
                [248, 248, 242],
                [236, 239, 244],
                [131, 148, 150],
                [248, 248, 242],
            ],
        )
    })
}
pub fn muted() -> Color32 {
    custom(|c| c.muted).unwrap_or_else(|| {
        themed_color(
            [157, 157, 163],
            [157, 170, 188],
            [93, 105, 122],
            [
                [139, 142, 164],
                [143, 157, 181],
                [101, 123, 131],
                [166, 166, 150],
            ],
        )
    })
}
pub fn selection() -> Color32 {
    custom(|c| c.selection).unwrap_or_else(accent)
}
pub fn cursor() -> Color32 {
    custom(|c| c.cursor).unwrap_or_else(accent_hover)
}

// The accent is user-configurable, so it lives in atomics instead of consts.
static ACCENT_RGB: AtomicU32 = AtomicU32::new(0x7160E8);
static ACCENT_HOVER_RGB: AtomicU32 = AtomicU32::new(0x8A7CF0);

fn pack(rgb: [u8; 3]) -> u32 {
    (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])
}

fn unpack(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub fn set_accent(choice: AccentChoice) {
    ACCENT_RGB.store(pack(choice.rgb()), Ordering::Relaxed);
    ACCENT_HOVER_RGB.store(pack(choice.hover_rgb()), Ordering::Relaxed);
}

pub fn accent() -> Color32 {
    custom(|c| c.accent).unwrap_or_else(|| unpack(ACCENT_RGB.load(Ordering::Relaxed)))
}

pub fn accent_hover() -> Color32 {
    custom(|c| {
        let rgb = c.accent;
        (u32::from(((rgb >> 16) as u8).saturating_add(35)) << 16)
            | (u32::from(((rgb >> 8) as u8).saturating_add(35)) << 8)
            | u32::from((rgb as u8).saturating_add(35))
    })
    .unwrap_or_else(|| unpack(ACCENT_HOVER_RGB.load(Ordering::Relaxed)))
}
pub fn apply(ctx: &egui::Context, appearance: &AppearanceSettings, editor: &EditorSettings) {
    let system_light = ctx.system_theme() == Some(egui::Theme::Light);
    THEME.store(
        match appearance.theme {
            ThemeChoice::CairnDark => 0,
            ThemeChoice::Graphite => 1,
            ThemeChoice::Light => 2,
            ThemeChoice::Dracula => 3,
            ThemeChoice::Nord => 4,
            ThemeChoice::SolarizedDark => 5,
            ThemeChoice::Monokai => 6,
            ThemeChoice::System if system_light => 2,
            ThemeChoice::System => 0,
        },
        Ordering::Relaxed,
    );
    set_accent(appearance.accent);
    set_custom_colors(appearance.custom_colors);
    let (accent, accent_hover) = (accent(), accent_hover());
    let is_light = appearance.theme == ThemeChoice::Light
        || (appearance.theme == ThemeChoice::System && system_light);
    let egui_theme = if is_light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    };
    // Style the theme egui is actually using. Otherwise an explicit light
    // theme on a dark system (or vice versa) updates an inactive style slot.
    ctx.set_theme(egui_theme);
    let mut visuals = if is_light {
        Visuals::light()
    } else {
        Visuals::dark()
    };
    visuals.panel_fill = bg();
    visuals.window_fill = panel();
    visuals.window_stroke = Stroke::new(1.0, line());
    visuals.extreme_bg_color = bg_dark();
    visuals.faint_bg_color = panel();
    visuals.override_text_color = Some(text());
    visuals.selection.bg_fill = selection().gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, selection());
    visuals.text_cursor.stroke.color = cursor();
    visuals.hyperlink_color = accent_hover;

    let radius = CornerRadius::same(4);
    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = bg();
    w.noninteractive.weak_bg_fill = bg();
    w.noninteractive.bg_stroke = Stroke::new(1.0, line());
    w.noninteractive.fg_stroke = Stroke::new(1.0, muted());
    w.inactive.bg_fill = card();
    w.inactive.weak_bg_fill = card();
    w.inactive.bg_stroke = Stroke::new(1.0, line());
    w.inactive.fg_stroke = Stroke::new(1.0, text());
    w.hovered.bg_fill = hover();
    w.hovered.weak_bg_fill = hover();
    w.hovered.bg_stroke = Stroke::new(1.0, accent);
    w.hovered.fg_stroke = Stroke::new(1.0, if is_light { text() } else { Color32::WHITE });
    w.active.bg_fill = accent;
    w.active.weak_bg_fill = accent;
    w.active.bg_stroke = Stroke::new(1.0, accent_hover);
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.open.bg_fill = card();
    w.open.weak_bg_fill = card();
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.corner_radius = radius;
    }

    ctx.set_visuals(visuals);
    if (ctx.zoom_factor() - appearance.ui_scale).abs() > f32::EPSILON {
        ctx.set_zoom_factor(appearance.ui_scale);
    }
    ctx.style_mut_of(egui_theme, |style| {
        let compact = appearance.density == InterfaceDensity::Compact;
        style.spacing.item_spacing = if compact {
            egui::vec2(5.0, 3.0)
        } else {
            egui::vec2(8.0, 6.0)
        };
        style.spacing.button_padding = if compact {
            egui::vec2(8.0, 3.0)
        } else {
            egui::vec2(12.0, 5.0)
        };
        style.spacing.interact_size.y = if compact { 22.0 } else { 26.0 };
        style.spacing.extra_text_line_spacing =
            editor.line_height + if compact { 0.0 } else { 1.0 };
        let ui_family = match appearance.ui_font_family {
            UiFontFamily::Proportional => FontFamily::Proportional,
            UiFontFamily::Monospace => FontFamily::Monospace,
        };
        let code_family = match editor.font_family {
            crate::settings::EditorFontFamily::Monospace => FontFamily::Monospace,
            crate::settings::EditorFontFamily::Proportional => FontFamily::Proportional,
        };
        // Explicit, consistent type scale (previously egui's defaults were used).
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(appearance.ui_font_size + 8.0, ui_family.clone()),
            ),
            (
                TextStyle::Body,
                FontId::new(appearance.ui_font_size, ui_family.clone()),
            ),
            (
                TextStyle::Button,
                FontId::new(appearance.ui_font_size, ui_family.clone()),
            ),
            (
                TextStyle::Small,
                FontId::new((appearance.ui_font_size - 2.0).max(8.0), ui_family),
            ),
            (
                TextStyle::Monospace,
                FontId::new(editor.font_size, code_family),
            ),
        ]
        .into();
    });
}
