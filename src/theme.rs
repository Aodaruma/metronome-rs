use eframe::egui;

use crate::{config::ThemeMode, fonts};

// Shared layout tokens. Page-specific code may opt into these without having
// to repeat the spacing choices made by the global egui style below.
pub const PAGE_PADDING: f32 = 20.0;
pub const SECTION_SPACING: f32 = 16.0;
pub const CARD_PADDING: f32 = 16.0;
pub const SETTINGS_ROW_HEIGHT: f32 = 36.0;
pub const CONTROL_CORNER_RADIUS: u8 = 7;
pub const CONTAINER_CORNER_RADIUS: u8 = 10;

// Compile-time theme palettes. Keep color tuning in this file so that UI code
// remains layout-focused and palette changes are easy to review.
pub const DARK_PANEL_FILL: [u8; 3] = [17, 22, 27];
pub const DARK_WINDOW_FILL: [u8; 3] = [22, 28, 34];
pub const DARK_EXTREME_BG: [u8; 3] = [11, 15, 19];
pub const DARK_FAINT_BG: [u8; 3] = [27, 35, 42];
pub const DARK_CONTROL_FILL: [u8; 3] = [32, 41, 48];
pub const DARK_CONTROL_HOVERED_FILL: [u8; 3] = [41, 52, 61];
pub const DARK_BORDER: [u8; 3] = [54, 67, 76];
pub const DARK_TEXT: [u8; 3] = [225, 232, 237];
pub const DARK_WEAK_TEXT: [u8; 3] = [158, 173, 183];

pub const LIGHT_PANEL_FILL: [u8; 3] = [245, 247, 248];
pub const LIGHT_WINDOW_FILL: [u8; 3] = [250, 251, 252];
pub const LIGHT_EXTREME_BG: [u8; 3] = [231, 236, 239];
pub const LIGHT_FAINT_BG: [u8; 3] = [238, 242, 244];
pub const LIGHT_INACTIVE_BG: [u8; 3] = [226, 232, 236];
pub const LIGHT_INACTIVE_WEAK_BG: [u8; 3] = [237, 241, 243];
pub const LIGHT_HOVERED_WEAK_BG: [u8; 3] = [225, 233, 237];
pub const LIGHT_BORDER: [u8; 3] = [194, 204, 210];
pub const LIGHT_TEXT: [u8; 3] = [38, 48, 54];
pub const LIGHT_WEAK_TEXT: [u8; 3] = [98, 111, 119];

pub const DARK_TEXT_ON_ACCENT: [u8; 3] = [25, 27, 29];

#[derive(Clone, Copy)]
struct Palette {
    panel: [u8; 3],
    window: [u8; 3],
    extreme: [u8; 3],
    faint: [u8; 3],
    control: [u8; 3],
    control_weak: [u8; 3],
    control_hovered: [u8; 3],
    border: [u8; 3],
    text: [u8; 3],
    weak_text: [u8; 3],
}

const DARK_PALETTE: Palette = Palette {
    panel: DARK_PANEL_FILL,
    window: DARK_WINDOW_FILL,
    extreme: DARK_EXTREME_BG,
    faint: DARK_FAINT_BG,
    control: DARK_CONTROL_FILL,
    control_weak: DARK_FAINT_BG,
    control_hovered: DARK_CONTROL_HOVERED_FILL,
    border: DARK_BORDER,
    text: DARK_TEXT,
    weak_text: DARK_WEAK_TEXT,
};

const LIGHT_PALETTE: Palette = Palette {
    panel: LIGHT_PANEL_FILL,
    window: LIGHT_WINDOW_FILL,
    extreme: LIGHT_EXTREME_BG,
    faint: LIGHT_FAINT_BG,
    control: LIGHT_INACTIVE_BG,
    control_weak: LIGHT_INACTIVE_WEAK_BG,
    control_hovered: LIGHT_HOVERED_WEAK_BG,
    border: LIGHT_BORDER,
    text: LIGHT_TEXT,
    weak_text: LIGHT_WEAK_TEXT,
};

pub fn apply(ctx: &egui::Context, theme: ThemeMode, accent_rgb: [u8; 3]) {
    for egui_theme in [egui::Theme::Dark, egui::Theme::Light] {
        // Preserve font and text-style choices made before theme application.
        let mut style = (*ctx.style_of(egui_theme)).clone();
        style.visuals = visuals_for(egui_theme, accent_rgb);
        apply_typography(&mut style);
        apply_spacing(&mut style);
        ctx.set_style_of(egui_theme, style);
    }

    ctx.set_theme(match theme {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
    });
}

fn apply_typography(style: &mut egui::Style) {
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0, fonts::strong_family()),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, fonts::regular_family()),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.5, fonts::regular_family()),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, fonts::regular_family()),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.0, fonts::regular_family()),
    );
}

fn apply_spacing(style: &mut egui::Style) {
    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::vec2(8.0, 8.0);
    spacing.window_margin = egui::Margin::same(16);
    spacing.button_padding = egui::vec2(10.0, 5.0);
    spacing.menu_margin = egui::Margin::same(8);
    spacing.indent = 18.0;
    spacing.interact_size = egui::vec2(44.0, 30.0);
    spacing.slider_width = 140.0;
    spacing.slider_rail_height = 4.0;
    spacing.combo_width = 160.0;
    spacing.text_edit_width = 180.0;
    spacing.icon_width = 18.0;
    spacing.icon_width_inner = 10.0;
    spacing.icon_spacing = 8.0;
    spacing.tooltip_width = 320.0;
    spacing.menu_width = 240.0;

    style.animation_time = 0.12;
    style.compact_menu_style = false;
}

fn visuals_for(theme: egui::Theme, accent_rgb: [u8; 3]) -> egui::Visuals {
    let (mut visuals, palette, accent_mix) = match theme {
        egui::Theme::Dark => (egui::Visuals::dark(), DARK_PALETTE, 0.22),
        egui::Theme::Light => (egui::Visuals::light(), LIGHT_PALETTE, 0.14),
    };

    let accent = rgb(accent_rgb);
    let border = rgb(palette.border);
    let text = rgb(palette.text);
    let weak_text = rgb(palette.weak_text);
    let control = rgb(palette.control);
    let control_weak = rgb(palette.control_weak);
    let control_hovered = rgb(palette.control_hovered);
    let active_fill = mix_color(control_hovered, accent, accent_mix);
    let accent_border = mix_color(border, accent, 0.68);
    let radius = egui::CornerRadius::same(CONTROL_CORNER_RADIUS);

    visuals.panel_fill = rgb(palette.panel);
    visuals.window_fill = rgb(palette.window);
    visuals.extreme_bg_color = rgb(palette.extreme);
    visuals.text_edit_bg_color = Some(rgb(palette.extreme));
    visuals.faint_bg_color = rgb(palette.faint);
    visuals.code_bg_color = rgb(palette.faint);
    visuals.override_text_color = None;
    visuals.weak_text_color = Some(weak_text);
    visuals.hyperlink_color = accent_border;
    visuals.window_corner_radius = egui::CornerRadius::same(CONTAINER_CORNER_RADIUS);
    visuals.menu_corner_radius = egui::CornerRadius::same(CONTROL_CORNER_RADIUS);
    visuals.window_stroke = egui::Stroke::new(1.0, border);
    visuals.collapsing_header_frame = false;
    visuals.indent_has_left_vline = false;
    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.disabled_alpha = 0.58;

    visuals.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: rgb(palette.window),
        weak_bg_fill: rgb(palette.faint),
        bg_stroke: egui::Stroke::new(1.0, border),
        corner_radius: egui::CornerRadius::same(CONTAINER_CORNER_RADIUS),
        fg_stroke: egui::Stroke::new(1.0, text),
        expansion: 0.0,
    };
    visuals.widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: control,
        weak_bg_fill: control_weak,
        bg_stroke: egui::Stroke::new(1.0, border),
        corner_radius: radius,
        fg_stroke: egui::Stroke::new(1.0, text),
        expansion: 0.0,
    };
    visuals.widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: control_hovered,
        weak_bg_fill: control_hovered,
        bg_stroke: egui::Stroke::new(1.0, accent_border),
        corner_radius: radius,
        fg_stroke: egui::Stroke::new(1.25, text),
        expansion: 0.0,
    };
    visuals.widgets.active = egui::style::WidgetVisuals {
        bg_fill: active_fill,
        weak_bg_fill: active_fill,
        bg_stroke: egui::Stroke::new(1.0, accent),
        corner_radius: radius,
        fg_stroke: egui::Stroke::new(1.5, text),
        expansion: 0.0,
    };
    visuals.widgets.open = egui::style::WidgetVisuals {
        bg_fill: control_hovered,
        weak_bg_fill: control_hovered,
        bg_stroke: egui::Stroke::new(1.0, accent_border),
        corner_radius: radius,
        fg_stroke: egui::Stroke::new(1.0, text),
        expansion: 0.0,
    };

    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = egui::Stroke::new(1.5, readable_text_color(accent));
    visuals.text_cursor.stroke = egui::Stroke::new(1.5, accent);
    visuals.ime_composition.active_underline_stroke = egui::Stroke::new(2.0, accent);
    visuals.ime_composition.inactive_underline_stroke =
        egui::Stroke::new(1.0, accent.gamma_multiply(0.55));
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

pub fn meter_track_stroke(visuals: &egui::Visuals) -> egui::Stroke {
    egui::Stroke::new(7.0, visuals.widgets.inactive.bg_fill)
}

pub fn meter_inactive_beat_color(visuals: &egui::Visuals) -> egui::Color32 {
    mix_color(
        visuals.widgets.inactive.bg_fill,
        visuals.weak_text_color(),
        0.55,
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

pub fn readable_text_color(color: egui::Color32) -> egui::Color32 {
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
    use super::{
        CONTROL_CORNER_RADIUS, DARK_PANEL_FILL, LIGHT_EXTREME_BG, apply, meter_inactive_beat_color,
        meter_track_stroke, rgb,
    };
    use crate::config::ThemeMode;
    use eframe::egui;

    #[test]
    fn palette_values_compile_to_expected_colors() {
        let light = rgb(LIGHT_EXTREME_BG);
        assert_eq!([light.r(), light.g(), light.b()], [231, 236, 239]);

        let dark = rgb(DARK_PANEL_FILL);
        assert_eq!([dark.r(), dark.g(), dark.b()], [17, 22, 27]);
    }

    #[test]
    fn apply_updates_both_styles_and_the_theme_preference() {
        let ctx = egui::Context::default();
        let accent = [0, 178, 246];

        apply(&ctx, ThemeMode::Dark, accent);
        assert_eq!(ctx.theme(), egui::Theme::Dark);

        let dark = ctx.style_of(egui::Theme::Dark);
        let light = ctx.style_of(egui::Theme::Light);
        assert_eq!(dark.spacing.item_spacing, egui::vec2(8.0, 8.0));
        assert_eq!(dark.spacing.interact_size, egui::vec2(44.0, 30.0));
        assert_eq!(
            dark.visuals.widgets.inactive.corner_radius,
            egui::CornerRadius::same(CONTROL_CORNER_RADIUS)
        );
        assert_eq!(
            dark.visuals.selection.bg_fill,
            egui::Color32::from_rgb(accent[0], accent[1], accent[2])
        );
        assert_ne!(dark.visuals.panel_fill, egui::Visuals::dark().panel_fill);
        assert_ne!(light.visuals.panel_fill, dark.visuals.panel_fill);
        assert_eq!(meter_track_stroke(&dark.visuals).width, 7.0);
        assert_ne!(
            meter_inactive_beat_color(&dark.visuals),
            dark.visuals.widgets.inactive.bg_fill
        );
        assert_ne!(
            meter_inactive_beat_color(&light.visuals),
            light.visuals.widgets.inactive.bg_fill
        );

        apply(&ctx, ThemeMode::Light, accent);
        assert_eq!(ctx.theme(), egui::Theme::Light);
    }
}
