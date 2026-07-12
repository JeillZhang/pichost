use image::{DynamicImage, ImageFormat};
use pichost_core::storage::StorageBackend;

fn thumbnail_output_format(img: &DynamicImage, source_fmt: ImageFormat) -> (ImageFormat, &'static str) {
    match source_fmt {
        ImageFormat::Png => {
            if img.color().has_alpha() {
                (ImageFormat::Png, "image/png")
            } else {
                (ImageFormat::Jpeg, "image/jpeg")
            }
        }
        _ => (ImageFormat::Jpeg, "image/jpeg"),
    }
}

fn should_thumbnail(fmt: ImageFormat) -> bool {
    !matches!(fmt, ImageFormat::Gif)
}

fn should_webp(fmt: ImageFormat) -> bool {
    matches!(fmt, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Avif | ImageFormat::Bmp)
}

pub async fn generate_thumbnail(
    img: &DynamicImage,
    source_fmt: ImageFormat,
    storage: &(impl StorageBackend + ?Sized),
    key: &str,
    max_size: u32,
    quality: u8,
) -> Result<(bool, String), String> {
    if !should_thumbnail(source_fmt) {
        return Ok((false, String::new()));
    }
    let (w, h) = (img.width(), img.height());
    let scale = max_size as f64 / w.max(h) as f64;
    let new_w = (w as f64 * scale).max(1.0) as u32;
    let new_h = (h as f64 * scale).max(1.0) as u32;
    let thumb = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
    let (out_fmt, mime) = thumbnail_output_format(img, source_fmt);
    let mut buf = Vec::new();
    match out_fmt {
        ImageFormat::Jpeg => {
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            thumb.write_with_encoder(encoder).map_err(|e| format!("jpeg encode: {e}"))?;
        }
        ImageFormat::Png => {
            thumb.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
                .map_err(|e| format!("png encode: {e}"))?;
        }
        _ => return Err(format!("unsupported thumb output format: {out_fmt:?}")),
    }
    storage.put(key, &buf, mime).await
        .map_err(|e| format!("thumb storage write: {e}"))?;
    Ok((true, mime.to_string()))
}

pub async fn convert_to_webp(
    img: &DynamicImage,
    source_fmt: ImageFormat,
    storage: &(impl StorageBackend + ?Sized),
    key: &str,
    quality: f32,
) -> Result<(bool, String), String> {
    if !should_webp(source_fmt) {
        return Ok((false, String::new()));
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let webp_data = webp::Encoder::from_rgba(&rgba, w, h).encode(quality);
    storage.put(key, webp_data.as_ref(), "image/webp").await
        .map_err(|e| format!("webp storage write: {e}"))?;
    Ok((true, "image/webp".to_string()))
}
