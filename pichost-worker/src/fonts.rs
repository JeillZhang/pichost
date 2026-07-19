// Functions in this module are used by watermark processing (T1-T3).
// Allow dead_code until those tasks consume these exports.
#![allow(dead_code)]

use rusttype::Font;

/// Load a built-in font from embedded bytes.
pub fn load_font(name: &str) -> Result<Font<'static>, String> {
    let bytes: &[u8] = match name {
        "NotoSansSC-Regular" => include_bytes!("../fonts/NotoSansSC-Regular.ttf"),
        "NotoSans-Regular" => include_bytes!("../fonts/NotoSans-Regular.ttf"),
        "Arial" => include_bytes!("../fonts/Arial.ttf"),
        "DejaVuSans" => include_bytes!("../fonts/DejaVuSans.ttf"),
        "FiraCode-Regular" => include_bytes!("../fonts/FiraCode-Regular.ttf"),
        other => {
            return Err(format!(
                "Unknown font: '{}'. Available fonts: NotoSansSC-Regular, \
                 NotoSans-Regular, Arial, DejaVuSans, FiraCode-Regular",
                other
            ))
        }
    };
    Font::try_from_bytes(bytes)
        .ok_or_else(|| format!("Failed to parse font: {}", name))
}

/// List all built-in font names.
pub fn builtin_font_names() -> Vec<&'static str> {
    vec![
        "NotoSansSC-Regular",
        "NotoSans-Regular",
        "Arial",
        "DejaVuSans",
        "FiraCode-Regular",
    ]
}

/// Calculate font size scaled relative to image diagonal.
pub fn scaled_font_size(img_diagonal: f32, base_size: u32, scale: f64) -> f32 {
    (base_size as f64 * scale * img_diagonal as f64 / 1000.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_each_builtin_font() {
        for name in builtin_font_names() {
            let font = load_font(name);
            assert!(font.is_ok(), "Failed to load font: {}", name);
        }
    }

    #[test]
    fn test_load_unknown_font_returns_err() {
        let result = load_font("ComicSans");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Available fonts"));
    }

    #[test]
    fn test_load_dejavu_sans() {
        let font = load_font("DejaVuSans").unwrap();
        let scale = rusttype::Scale::uniform(48.0);
        let glyph: Vec<_> = font
            .layout("Hello", scale, rusttype::point(0.0, 0.0))
            .collect();
        assert!(!glyph.is_empty());
    }

    #[test]
    fn test_scaled_font_size() {
        // diagonal=1000, base=48, scale=0.15 → 48 * 0.15 * 1000 / 1000 = 7.2
        let size = scaled_font_size(1000.0, 48, 0.15);
        assert!((size - 7.2).abs() < f64::EPSILON as f32);
    }
}
