use std::sync::Arc;

use eframe::egui::{
    Context, FontData, FontDefinitions, FontFamily, FontTweak, epaint::text::VariationCoords,
};

const JETBRAINS_MONO_REGULAR: &str = "jetbrains_mono_regular";
const JETBRAINS_MONO_STRONG: &str = "jetbrains_mono_strong";
const UDEV_GOTHIC_REGULAR: &str = "udev_gothic_regular";
const UDEV_GOTHIC_STRONG: &str = "udev_gothic_strong";

pub const UI_MONO_REGULAR_FAMILY: &str = "ui_mono_regular";
pub const UI_MONO_STRONG_FAMILY: &str = "ui_mono_strong";

const JETBRAINS_MONO_BYTES: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono[wght].ttf");
const UDEV_GOTHIC_REGULAR_BYTES: &[u8] = include_bytes!("../assets/fonts/UDEVGothic-Regular.ttf");
const UDEV_GOTHIC_BOLD_BYTES: &[u8] = include_bytes!("../assets/fonts/UDEVGothic-Bold.ttf");

pub fn install_application_fonts(ctx: &Context) {
    ctx.set_fonts(application_font_definitions());
}

pub fn regular_family() -> FontFamily {
    FontFamily::Name(UI_MONO_REGULAR_FAMILY.into())
}

pub fn strong_family() -> FontFamily {
    FontFamily::Name(UI_MONO_STRONG_FAMILY.into())
}

fn application_font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        JETBRAINS_MONO_REGULAR.to_owned(),
        Arc::new(variable_font(JETBRAINS_MONO_BYTES, 400.0)),
    );
    fonts.font_data.insert(
        JETBRAINS_MONO_STRONG.to_owned(),
        Arc::new(variable_font(JETBRAINS_MONO_BYTES, 600.0)),
    );
    fonts.font_data.insert(
        UDEV_GOTHIC_REGULAR.to_owned(),
        Arc::new(FontData::from_static(UDEV_GOTHIC_REGULAR_BYTES)),
    );
    fonts.font_data.insert(
        UDEV_GOTHIC_STRONG.to_owned(),
        Arc::new(FontData::from_static(UDEV_GOTHIC_BOLD_BYTES)),
    );

    let regular_stack = vec![
        JETBRAINS_MONO_REGULAR.to_owned(),
        UDEV_GOTHIC_REGULAR.to_owned(),
    ];
    let strong_stack = vec![
        JETBRAINS_MONO_STRONG.to_owned(),
        UDEV_GOTHIC_STRONG.to_owned(),
    ];

    fonts
        .families
        .insert(regular_family(), regular_stack.clone());
    fonts.families.insert(strong_family(), strong_stack);

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let stack = fonts.families.entry(family).or_default();
        stack.retain(|name| !regular_stack.contains(name));
        stack.splice(0..0, regular_stack.iter().cloned());
    }

    fonts
}

fn variable_font(bytes: &'static [u8], weight: f32) -> FontData {
    FontData::from_static(bytes).tweak(FontTweak {
        coords: VariationCoords::new([(b"wght", weight)]),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        JETBRAINS_MONO_REGULAR, UDEV_GOTHIC_REGULAR, application_font_definitions, regular_family,
        strong_family,
    };
    use eframe::egui::FontFamily;

    #[test]
    fn bundled_monospace_fonts_are_the_primary_ui_fonts() {
        let fonts = application_font_definitions();
        let expected = [JETBRAINS_MONO_REGULAR, UDEV_GOTHIC_REGULAR];

        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            regular_family(),
        ] {
            let stack = fonts.families.get(&family).expect("font family exists");
            assert_eq!(stack.first().map(String::as_str), Some(expected[0]));
            assert_eq!(stack.get(1).map(String::as_str), Some(expected[1]));
        }
        assert!(fonts.families.contains_key(&strong_family()));
    }
}
