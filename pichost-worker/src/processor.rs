use image::{DynamicImage, ImageFormat};
use pichost_core::storage::StorageBackend;

fn thumbnail_output_format(
    img: &DynamicImage,
    source_fmt: ImageFormat,
) -> (ImageFormat, &'static str) {
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
    matches!(
        fmt,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Avif | ImageFormat::Bmp
    )
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
            thumb
                .write_with_encoder(encoder)
                .map_err(|e| format!("jpeg encode: {e}"))?;
        }
        ImageFormat::Png => {
            thumb
                .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
                .map_err(|e| format!("png encode: {e}"))?;
        }
        _ => return Err(format!("unsupported thumb output format: {out_fmt:?}")),
    }
    storage
        .put(key, &buf, mime)
        .await
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
    let webp_bytes: Vec<u8> = {
        let webp_data = webp::Encoder::from_rgba(&rgba, w, h).encode(quality);
        webp_data.to_vec()
        // webp_data (WebPMemory) is dropped here — it is not Send, so we
        // scope it tightly before the await boundary.
    };
    storage
        .put(key, &webp_bytes, "image/webp")
        .await
        .map_err(|e| format!("webp storage write: {e}"))?;
    Ok((true, "image/webp".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pichost_core::error::StorageError;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    struct MockBackend {
        items: Mutex<Vec<(String, Vec<u8>, String)>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self { items: Mutex::new(Vec::new()) }
        }
        fn stored(&self, key: &str) -> Option<(Vec<u8>, String)> {
            self.items
                .lock()
                .unwrap()
                .iter()
                .find(|(k, _, _)| k == key)
                .map(|(_, d, ct)| (d.clone(), ct.clone()))
        }
    }

    impl StorageBackend for MockBackend {
        fn put<'l0, 'l1, 'l2, 'l3, 'a>(
            &'l0 self,
            key: &'l1 str,
            data: &'l2 [u8],
            ct: &'l3 str,
        ) -> Pin<Box<dyn Future<Output = Result<String, StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            'l2: 'a,
            'l3: 'a,
            Self: 'a,
        {
            Box::pin(async move {
                self.items
                    .lock()
                    .unwrap()
                    .push((key.to_string(), data.to_vec(), ct.to_string()));
                Ok(key.to_string())
            })
        }
        fn get<'l0, 'l1, 'a>(
            &'l0 self,
            key: &'l1 str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            Self: 'a,
        {
            Box::pin(async move {
                Ok(self.stored(key).map(|(d, _)| d).unwrap_or_default())
            })
        }
        fn delete<'l0, 'l1, 'a>(
            &'l0 self,
            key: &'l1 str,
        ) -> Pin<Box<dyn Future<Output = Result<(), StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            Self: 'a,
        {
            Box::pin(async move {
                self.items.lock().unwrap().retain(|(k, _, _)| k != key);
                Ok(())
            })
        }
        fn exists<'l0, 'l1, 'a>(
            &'l0 self,
            key: &'l1 str,
        ) -> Pin<Box<dyn Future<Output = Result<bool, StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            Self: 'a,
        {
            Box::pin(async move {
                Ok(self.items.lock().unwrap().iter().any(|(k, _, _)| k == key))
            })
        }
        fn public_url(&self, key: &str) -> String {
            format!("/u/{key}")
        }
        fn backend_name(&self) -> &str {
            "mock"
        }
    }

    #[test]
    fn test_thumbnail_output_format_png_alpha() {
        let img = DynamicImage::new_rgba8(4, 4);
        assert_eq!(
            thumbnail_output_format(&img, ImageFormat::Png),
            (ImageFormat::Png, "image/png")
        );
    }

    #[test]
    fn test_thumbnail_output_format_png_no_alpha() {
        let img = DynamicImage::new_rgb8(4, 4);
        assert_eq!(
            thumbnail_output_format(&img, ImageFormat::Png),
            (ImageFormat::Jpeg, "image/jpeg")
        );
    }

    #[test]
    fn test_thumbnail_output_format_jpeg_and_gif() {
        let img = DynamicImage::new_rgba8(4, 4);
        assert_eq!(
            thumbnail_output_format(&img, ImageFormat::Jpeg),
            (ImageFormat::Jpeg, "image/jpeg")
        );
        assert_eq!(
            thumbnail_output_format(&img, ImageFormat::Gif),
            (ImageFormat::Jpeg, "image/jpeg")
        );
    }

    #[test]
    fn test_should_thumbnail() {
        assert!(!should_thumbnail(ImageFormat::Gif));
        assert!(should_thumbnail(ImageFormat::Png));
        assert!(should_thumbnail(ImageFormat::Jpeg));
        assert!(should_thumbnail(ImageFormat::WebP));
    }

    #[test]
    fn test_should_webp() {
        for fmt in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Avif, ImageFormat::Bmp] {
            assert!(should_webp(fmt), "{fmt:?}");
        }
        for fmt in [ImageFormat::Gif, ImageFormat::WebP] {
            assert!(!should_webp(fmt), "{fmt:?}");
        }
    }

    #[tokio::test]
    async fn test_generate_thumbnail_gif_skipped() {
        let backend = MockBackend::new();
        let img = DynamicImage::new_rgba8(10, 10);
        let (written, mime) =
            generate_thumbnail(&img, ImageFormat::Gif, &backend, "t", 64, 80).await.unwrap();
        assert_eq!((written, mime.as_str()), (false, ""));
        assert!(backend.stored("t").is_none());
    }

    #[tokio::test]
    async fn test_generate_thumbnail_png_alpha() {
        let backend = MockBackend::new();
        let img = DynamicImage::new_rgba8(100, 200);
        let (written, mime) =
            generate_thumbnail(&img, ImageFormat::Png, &backend, "t", 64, 80).await.unwrap();
        assert_eq!((written, mime.as_str()), (true, "image/png"));
        let (bytes, ct) = backend.stored("t").expect("thumb stored");
        assert_eq!(ct, "image/png");
        let out = image::load_from_memory(&bytes).unwrap();
        assert!(out.width() <= 64 && out.height() <= 64);
        assert_eq!((out.width(), out.height()), (32, 64));
    }

    #[tokio::test]
    async fn test_generate_thumbnail_png_no_alpha_jpeg() {
        let backend = MockBackend::new();
        let img = DynamicImage::new_rgb8(100, 100);
        let (written, mime) =
            generate_thumbnail(&img, ImageFormat::Png, &backend, "t", 50, 80).await.unwrap();
        assert_eq!((written, mime.as_str()), (true, "image/jpeg"));
        let (bytes, ct) = backend.stored("t").expect("thumb stored");
        assert_eq!(ct, "image/jpeg");
        assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::Jpeg);
    }

    #[tokio::test]
    async fn test_convert_to_webp_unsupported() {
        let backend = MockBackend::new();
        let img = DynamicImage::new_rgba8(10, 10);
        let (written, mime) =
            convert_to_webp(&img, ImageFormat::Gif, &backend, "w", 82.0).await.unwrap();
        assert_eq!((written, mime.as_str()), (false, ""));
        assert!(backend.stored("w").is_none());
    }

    #[tokio::test]
    async fn test_convert_to_webp_png() {
        let backend = MockBackend::new();
        let img = DynamicImage::new_rgba8(8, 8);
        let (written, mime) =
            convert_to_webp(&img, ImageFormat::Png, &backend, "w", 82.0).await.unwrap();
        assert_eq!((written, mime.as_str()), (true, "image/webp"));
        let (bytes, ct) = backend.stored("w").expect("webp stored");
        assert_eq!(ct, "image/webp");
        assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::WebP);
        let out = image::load_from_memory(&bytes).unwrap();
        assert_eq!((out.width(), out.height()), (8, 8));
    }

    #[tokio::test]
    async fn test_convert_to_webp_avif_bmp() {
        for fmt in [ImageFormat::Avif, ImageFormat::Bmp] {
            let backend = MockBackend::new();
            let img = DynamicImage::new_rgba8(6, 6);
            let (written, mime) =
                convert_to_webp(&img, fmt, &backend, "w", 82.0).await.unwrap();
            assert_eq!((written, mime.as_str()), (true, "image/webp"), "{fmt:?}");
            let (bytes, ct) = backend.stored("w").unwrap();
            assert_eq!(ct, "image/webp");
            assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::WebP);
        }
    }

    struct FailingBackend;

    impl StorageBackend for FailingBackend {
        fn put<'l0, 'l1, 'l2, 'l3, 'a>(
            &'l0 self,
            _key: &'l1 str,
            _data: &'l2 [u8],
            _ct: &'l3 str,
        ) -> Pin<Box<dyn Future<Output = Result<String, StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            'l2: 'a,
            'l3: 'a,
            Self: 'a,
        {
            Box::pin(async move { Err(StorageError::WriteFailed("boom".into())) })
        }
        fn get<'l0, 'l1, 'a>(
            &'l0 self,
            _key: &'l1 str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            Self: 'a,
        {
            Box::pin(async move { Err(StorageError::ReadFailed("boom".into())) })
        }
        fn delete<'l0, 'l1, 'a>(
            &'l0 self,
            _key: &'l1 str,
        ) -> Pin<Box<dyn Future<Output = Result<(), StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            Self: 'a,
        {
            Box::pin(async move { Err(StorageError::WriteFailed("boom".into())) })
        }
        fn exists<'l0, 'l1, 'a>(
            &'l0 self,
            _key: &'l1 str,
        ) -> Pin<Box<dyn Future<Output = Result<bool, StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            Self: 'a,
        {
            Box::pin(async move { Ok(false) })
        }
        fn public_url(&self, _key: &str) -> String {
            String::new()
        }
        fn backend_name(&self) -> &str {
            "failing"
        }
    }

    #[tokio::test]
    async fn test_generate_thumbnail_jpeg_source() {
        let backend = MockBackend::new();
        let img = DynamicImage::new_rgb8(80, 40);
        let (written, mime) =
            generate_thumbnail(&img, ImageFormat::Jpeg, &backend, "tj", 32, 90).await.unwrap();
        assert_eq!((written, mime.as_str()), (true, "image/jpeg"));
        let (bytes, ct) = backend.stored("tj").expect("thumb stored");
        assert_eq!(ct, "image/jpeg");
        assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::Jpeg);
        let out = image::load_from_memory(&bytes).unwrap();
        assert_eq!((out.width(), out.height()), (32, 16));
    }

    #[tokio::test]
    async fn test_generate_thumbnail_upscale() {
        let backend = MockBackend::new();
        let img = DynamicImage::new_rgb8(100, 50);
        let (written, mime) =
            generate_thumbnail(&img, ImageFormat::Jpeg, &backend, "tu", 200, 80).await.unwrap();
        assert_eq!((written, mime.as_str()), (true, "image/jpeg"));
        let (bytes, _) = backend.stored("tu").expect("thumb stored");
        let out = image::load_from_memory(&bytes).unwrap();
        assert_eq!((out.width(), out.height()), (200, 100));
    }

    #[tokio::test]
    async fn test_generate_thumbnail_storage_failure() {
        let backend = FailingBackend;
        let img = DynamicImage::new_rgb8(10, 10);
        let err = generate_thumbnail(&img, ImageFormat::Png, &backend, "tf", 64, 80).await;
        assert!(err.unwrap_err().contains("thumb storage write"));
    }

    #[tokio::test]
    async fn test_convert_to_webp_storage_failure() {
        let backend = FailingBackend;
        let img = DynamicImage::new_rgba8(10, 10);
        let err = convert_to_webp(&img, ImageFormat::Png, &backend, "wf", 82.0).await;
        assert!(err.unwrap_err().contains("webp storage write"));
    }

    #[tokio::test]
    async fn test_mock_backend_crud_methods() {
        let backend = MockBackend::new();
        backend.put("k", b"data", "image/png").await.unwrap();
        assert!(backend.exists("k").await.unwrap());
        assert_eq!(backend.get("k").await.unwrap(), b"data");
        backend.delete("k").await.unwrap();
        assert!(!backend.exists("k").await.unwrap());
        assert!(backend.get("k").await.unwrap().is_empty());
        assert_eq!(backend.public_url("k"), "/u/k");
        assert_eq!(backend.backend_name(), "mock");
    }

    #[tokio::test]
    async fn test_failing_backend_methods() {
        let backend = FailingBackend;
        assert!(backend.get("k").await.is_err());
        assert!(backend.delete("k").await.is_err());
        assert!(!backend.exists("k").await.unwrap());
        assert!(backend.public_url("k").is_empty());
        assert_eq!(backend.backend_name(), "failing");
    }
}
