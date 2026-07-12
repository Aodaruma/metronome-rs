use eframe::egui;

use crate::config::ThemeMode;

// Compile-time theme palette. Keep color tuning in this file so that UI code
// remains layout-focused and palette changes are easy to review.
pub const LIGHT_PANEL_FILL: [u8; 3] = [238, 240, 242];
pub const LIGHT_WINDOW_FILL: [u8; 3] = [232, 234, 236];
pub const LIGHT_EXTREME_BG: [u8; 3] = [205, 210, 214];
pub const LIGHT_FAINT_BG: [u8; 3] = [216, 220, 224];
pub const LIGHT_INACTIVE_BG: [u8; 3] = [174, 181, 187];
pub const LIGHT_INACTIVE_WEAK_BG: [u8; 3] = [205, 210, 214];
pub const LIGHT_HOVERED_WEAK_BG: [u8; 3] = [190, 196, 201];
pub const DARK_TEXT_ON_ACCENT: [u8; 3] = [25, 27, 29];

pub fn apply(ctx: &egui::Context, theme: ThemeMode, accent_rgb: [u8; 3]) {
    ctx.set_visuals_of(
        egui::Theme::Dark,
        visuals_for(egui::Theme::Dark, accent_rgb),
    );
    ctx.set_visuals_of(
        egui::Theme::Light,
        visuals_for(egui::Theme::Light, accent_rgb),
    );
    ctx.set_theme(match theme {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
    });
}

fn visuals_for(theme: egui::Theme, accent_rgb: [u8; 3]) -> egui::Visuals {
    let mut visuals = match theme {
        egui::Theme::Dark => egui::Visuals::dark(),
        egui::Theme::Light => {
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = rgb(LIGHT_PANEL_FILL);
            visuals.window_fill = rgb(LIGHT_WINDOW_FILL);
            visuals.extreme_bg_color = rgb(LIGHT_EXTREME_BG);
            visuals.faint_bg_color = rgb(LIGHT_FAINT_BG);
            visuals.widgets.inactive.bg_fill = rgb(LIGHT_INACTIVE_BG);
            visuals.widgets.inactive.weak_bg_fill = rgb(LIGHT_INACTIVE_WEAK_BG);
            visuals.widgets.hovered.weak_bg_fill = rgb(LIGHT_HOVERED_WEAK_BG);
            visuals
        }
    };
    let accent = rgb(accent_rgb);
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke.color = readable_on(accent);
    visuals
}

pub fn central_plate_color(
    visuals: &egui::Visuals,
    accent: egui::Color32,
    accent_amount: f32,
) -> egui::Color32 {
    mix_color(
        visuals.extreme_bg_color,
        accent,
        accent_amount.clamp(0.0, 1.0),
    )
}

fn rgb(value: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(value[0], value[1], value[2])
}

fn mix_color(from: egui::Color32, to: egui::Color32, amount: f32) -> egui::Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    egui::Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

fn readable_on(color: egui::Color32) -> egui::Color32 {
    let luminance = 0.2126 * f32::from(color.r())
        + 0.7152 * f32::from(color.g())
        + 0.0722 * f32::from(color.b());
    if luminance > 150.0 {
        rgb(DARK_TEXT_ON_ACCENT)
    } else {
        egui::Color32::WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::{LIGHT_EXTREME_BG, apply, rgb};
    use crate::config::ThemeMode;
    use eframe::egui;

    #[test]
    fn palette_values_compile_to_expected_color() {
        let color = rgb(LIGHT_EXTREME_BG);
        assert_eq!([color.r(), color.g(), color.b()], [205, 210, 214]);
    }

    #[test]
    fn apply_updates_the_egui_theme_preference() {
        let ctx = egui::Context::default();

        apply(&ctx, ThemeMode::Dark, [0, 122, 170]);
        assert_eq!(ctx.theme(), egui::Theme::Dark);

        apply(&ctx, ThemeMode::Light, [0, 122, 170]);
        assert_eq!(ctx.theme(), egui::Theme::Light);
    }
}
