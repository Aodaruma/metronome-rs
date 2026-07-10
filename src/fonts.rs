use std::path::Path;
use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn install_japanese_font(ctx: &Context) {
    let Some((name, bytes)) = load_first_available_font() else {
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.clone());
    }

    ctx.set_fonts(fonts);
}

fn load_first_available_font() -> Option<(String, Vec<u8>)> {
    font_candidates().iter().find_map(|path| {
        std::fs::read(path)
            .ok()
            .map(|bytes| (font_name(path), bytes))
    })
}

fn font_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("system-japanese-font")
        .to_owned()
}

fn font_candidates() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &[
            "C:/Windows/Fonts/YuGothM.ttc",
            "C:/Windows/Fonts/BIZ-UDGothicR.ttc",
            "C:/Windows/Fonts/meiryo.ttc",
            "C:/Windows/Fonts/NotoSansJP-VF.ttf",
            "C:/Windows/Fonts/YuGothR.ttc",
            "C:/Windows/Fonts/msgothic.ttc",
        ]
    }

    #[cfg(target_os = "macos")]
    {
        &[
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc",
            "/Library/Fonts/NotoSansCJKjp-Regular.otf",
            "/Library/Fonts/NotoSansJP-Regular.otf",
        ]
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
            "/usr/share/fonts/truetype/noto/NotoSansJP-Regular.ttf",
            "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
        ]
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        &[]
    }
}
