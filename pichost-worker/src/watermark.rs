// Functions in this module are used by watermark processing (T3/T6).
// Allow dead_code until those tasks consume these exports.
#![allow(dead_code)]

use ab_glyph::FontArc;
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use pichost_core::models::{WatermarkConfig, WatermarkPosition};

use crate::fonts;

/// Apply watermark text overlay to a DynamicImage.
/// Returns a new DynamicImage with watermark applied, or the original if disabled.
pub fn apply_watermark(
    img: &DynamicImage,
    config: &WatermarkConfig,
) -> Result<DynamicImage, String> {
    if !config.enabled || config.text.is_empty() {
        return Ok(img.clone());
    }

    let (w, h) = (img.width() as f32, img.height() as f32);
    let diagonal = (w * w + h * h).sqrt();
    let font_size = fonts::scaled_font_size(diagonal, config.font_size, config.scale);

    // Load font as ab_glyph FontArc (imageproc 0.25 uses ab_glyph internally)
    let font = load_font_arc(&config.font)?;
    let color = parse_rgba(&config.color)?;

    let mut canvas = img.to_rgba8();

    match config.position {
        WatermarkPosition::Tile => {
            draw_tiled(&mut canvas, &font, &config.text, font_size, color);
        }
        _ => {
            let (x, y) = calculate_position(
                w as u32,
                h as u32,
                &config.text,
                font_size,
                &font,
                &config.position,
                config.margin_x,
                config.margin_y,
            );
            draw_text_mut(&mut canvas, color, x, y, font_size, &font, &config.text);
        }
    }

    Ok(DynamicImage::ImageRgba8(canvas))
}

/// Load a built-in font as ab_glyph::FontArc for use with imageproc 0.25.
fn load_font_arc(name: &str) -> Result<FontArc, String> {
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
    FontArc::try_from_vec(bytes.to_vec())
        .map_err(|_| format!("Failed to parse font: {}", name))
}

fn parse_rgba(color_str: &str) -> Result<Rgba<u8>, String> {
    let s = color_str.trim();
    // rgba(r, g, b, a)
    if s.starts_with("rgba(") && s.ends_with(')') {
        let inner = &s[5..s.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 4 {
            let r: u8 = parts[0].parse().map_err(|_| "Invalid R in rgba()".to_string())?;
            let g: u8 = parts[1].parse().map_err(|_| "Invalid G in rgba()".to_string())?;
            let b: u8 = parts[2].parse().map_err(|_| "Invalid B in rgba()".to_string())?;
            let a: f64 = parts[3].parse().map_err(|_| "Invalid A in rgba()".to_string())?;
            let a_u8 = (a.clamp(0.0, 1.0) * 255.0) as u8;
            return Ok(Rgba([r, g, b, a_u8]));
        }
    }
    // #RRGGBB or #RRGGBBAA
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "Invalid hex color".to_string())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "Invalid hex color".to_string())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "Invalid hex color".to_string())?;
            return Ok(Rgba([r, g, b, 255]));
        }
        if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "Invalid hex color".to_string())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "Invalid hex color".to_string())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "Invalid hex color".to_string())?;
            let a = u8::from_str_radix(&hex[6..8], 16).map_err(|_| "Invalid hex color".to_string())?;
            return Ok(Rgba([r, g, b, a]));
        }
    }
    Err(format!(
        "Invalid color format: '{}'. Use rgba(r,g,b,a) or #RRGGBB",
        s
    ))
}

#[expect(clippy::too_many_arguments)]
fn calculate_position(
    img_w: u32,
    img_h: u32,
    text: &str,
    font_size: f32,
    font: &FontArc,
    position: &WatermarkPosition,
    margin_x: u32,
    margin_y: u32,
) -> (i32, i32) {
    let (text_w, _) = text_size(font_size, font, text);
    let text_h = font_size.ceil() as u32;
    let mx = margin_x as i32;
    let my = margin_y as i32;

    match position {
        WatermarkPosition::TopLeft => (mx, my),
        WatermarkPosition::TopRight => (img_w as i32 - text_w as i32 - mx, my),
        WatermarkPosition::BottomLeft => (mx, img_h as i32 - text_h as i32 - my),
        WatermarkPosition::BottomRight => {
            (img_w as i32 - text_w as i32 - mx, img_h as i32 - text_h as i32 - my)
        }
        WatermarkPosition::Center => {
            ((img_w as i32 - text_w as i32) / 2, (img_h as i32 - text_h as i32) / 2)
        }
        WatermarkPosition::Tile => unreachable!(),
    }
}

fn draw_tiled(
    canvas: &mut RgbaImage,
    font: &FontArc,
    text: &str,
    font_size: f32,
    color: Rgba<u8>,
) {
    let (text_w, _) = text_size(font_size, font, text);
    let text_h = font_size.ceil() as u32;

    let tile_w = (text_w as f32 * 3.0) as u32;
    let tile_h = (text_h as f32 * 5.0) as u32;
    let (img_w, img_h) = (canvas.width(), canvas.height());

    let mut y = 0u32;
    while y < img_h {
        let mut x = 0u32;
        while x < img_w {
            draw_text_mut(canvas, color, x as i32, y as i32, font_size, font, text);
            x += tile_w;
        }
        y += tile_h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn test_parse_rgba_functional() {
        assert_eq!(parse_rgba("rgba(255,0,0,0.5)").unwrap(), Rgba([255, 0, 0, 127]));
    }

    #[test]
    fn test_parse_rgba_hex6() {
        assert_eq!(parse_rgba("#FF0000").unwrap(), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn test_parse_rgba_hex8() {
        assert_eq!(parse_rgba("#FF0000AA").unwrap(), Rgba([255, 0, 0, 170]));
    }

    #[test]
    fn test_parse_rgba_invalid() {
        assert!(parse_rgba("invalid").is_err());
    }

    #[test]
    fn test_disabled_config_returns_clone() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(100, 100));
        let config: WatermarkConfig =
            serde_json::from_str(r#"{"enabled":false,"text":""}"#).unwrap();
        let result = apply_watermark(&img, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().width(), 100);
    }

    #[test]
    fn test_enabled_returns_same_dimensions() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(200, 150));
        let config: WatermarkConfig =
            serde_json::from_str(r#"{"enabled":true,"text":"test","position":"center"}"#).unwrap();
        let result = apply_watermark(&img, &config);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert_eq!(out.width(), 200);
        assert_eq!(out.height(), 150);
    }

    #[test]
    fn test_disabled_empty_text_returns_clone() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(64, 64));
        let config: WatermarkConfig =
            serde_json::from_str(r#"{"enabled":true,"text":""}"#).unwrap();
        let result = apply_watermark(&img, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().width(), 64);
    }

    #[test]
    fn test_invalid_font_returns_err() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(100, 100));
        let config: WatermarkConfig = serde_json::from_str(
            r#"{"enabled":true,"text":"hello","font":"NonExistentFont"}"#,
        )
        .unwrap();
        let result = apply_watermark(&img, &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown font"));
    }

    #[test]
    fn test_all_positions_produce_output() {
        let positions = [
            "top-left", "top-right", "bottom-left", "bottom-right", "center", "tile",
        ];
        for pos in &positions {
            let json = format!(
                r#"{{"enabled":true,"text":"pos","position":"{}"}}"#,
                pos
            );
            let config: WatermarkConfig = serde_json::from_str(&json).unwrap();
            let img = DynamicImage::ImageRgba8(RgbaImage::new(100, 100));
            let result = apply_watermark(&img, &config);
            assert!(result.is_ok(), "Failed for position: {}", pos);
            assert_eq!(result.unwrap().width(), 100);
        }
    }

    #[test]
    fn test_parse_rgba_error_invalid_hex() {
        assert!(parse_rgba("#GGG").is_err());
    }

    #[test]
    fn test_calculate_position_bottom_right() {
        let font = load_font_arc("NotoSansSC-Regular").unwrap();
        let (x, y) = calculate_position(
            200, 100, "hello", 24.0, &font, &WatermarkPosition::BottomRight, 10, 10,
        );
        assert!(x >= 0);
        assert!(y >= 0);
    }

    #[test]
    fn test_calculate_position_center() {
        let font = load_font_arc("NotoSansSC-Regular").unwrap();
        let (x, y) = calculate_position(
            200, 100, "center", 24.0, &font, &WatermarkPosition::Center, 0, 0,
        );
        assert!(x >= 0);
        assert!(y >= 0);
    }

    #[test]
    fn test_load_font_arc_each_builtin() {
        for name in &[
            "NotoSansSC-Regular",
            "NotoSans-Regular",
            "Arial",
            "DejaVuSans",
            "FiraCode-Regular",
        ] {
            let font = load_font_arc(name);
            assert!(font.is_ok(), "Failed to load font: {}", name);
        }
    }

    #[test]
    fn test_load_font_arc_unknown() {
        let result = load_font_arc("ComicSans");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown font"));
    }
}
