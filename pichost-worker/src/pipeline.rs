use pichost_core::config::AppConfig;
use pichost_core::StorageRouter;
use sqlx::PgPool;

use crate::processor;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("storage read failed: {0}")]
    StorageRead(String),
    #[allow(dead_code)]
    #[error("storage write failed: {0}")]
    StorageWrite(String),
    #[error("image decode failed: {0}")]
    Decode(String),
    #[error("thumbnail generation failed: {0}")]
    Thumbnail(String),
    #[error("webp conversion failed: {0}")]
    Webp(String),
    #[error("database update failed: {0}")]
    Database(String),
}

use crate::queue::TaskPayload;

pub async fn process_task(
    pool: &PgPool,
    router: &StorageRouter,
    config: &AppConfig,
    task: &TaskPayload,
) -> Result<(), PipelineError> {
    let (img, fmt, _bytes) = read_source_image(router, task).await?;
    let (width, height) = (img.width() as i32, img.height() as i32);
    let thumb_key = format!("{}/thumb.{}", task.user_id, task.image_id);
    let webp_key = format!("{}/webp.{}", task.user_id, task.image_id);
    let source_backend = router.for_backend(&task.storage_backend);

    let (thumb_written, webp_written) = process_image_variants(
        &img, fmt, source_backend.as_ref(), &thumb_key, &webp_key, config,
    )
    .await?;

    let public_url = config.server.public_url.trim_end_matches('/');
    update_image_record(
        pool, task, width, height, &thumb_key, &webp_key,
        thumb_written, webp_written, public_url,
    )
    .await?;

    tracing::info!(
        image_id = %task.image_id, width, height,
        thumb = thumb_written, webp = webp_written,
        backend = task.storage_backend,
        "processing complete"
    );
    Ok(())
}

async fn process_image_variants(
    img: &image::DynamicImage,
    fmt: image::ImageFormat,
    source_backend: &(dyn pichost_core::storage::StorageBackend + '_),
    thumb_key: &str,
    webp_key: &str,
    config: &AppConfig,
) -> Result<(bool, bool), PipelineError> {
    let (thumb_written, _) = processor::generate_thumbnail(
        img,
        fmt,
        source_backend,
        thumb_key,
        config.worker.processing.thumbnail_size,
        config.worker.processing.thumbnail_quality,
    )
    .await
    .map_err(PipelineError::Thumbnail)?;

    let (webp_written, _) = processor::convert_to_webp(
        img,
        fmt,
        source_backend,
        webp_key,
        config.worker.processing.webp_quality,
    )
    .await
    .map_err(PipelineError::Webp)?;

    Ok((thumb_written, webp_written))
}

/// Read and decode the source image from the configured storage backend.
async fn read_source_image(
    router: &StorageRouter,
    task: &TaskPayload,
) -> Result<(image::DynamicImage, image::ImageFormat, Vec<u8>), PipelineError> {
    let source_backend = router.for_backend(&task.storage_backend);

    let bytes = source_backend
        .get(&task.source_key)
        .await
        .map_err(|e| PipelineError::StorageRead(e.to_string()))?;

    let img = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| PipelineError::Decode(e.to_string()))?
        .decode()
        .map_err(|e| PipelineError::Decode(e.to_string()))?;

    let fmt = image::guess_format(&bytes).map_err(|e| PipelineError::Decode(e.to_string()))?;

    Ok((img, fmt, bytes))
}

/// Persist processing results into the images table.
#[allow(clippy::too_many_arguments)]
async fn update_image_record(
    pool: &PgPool,
    task: &TaskPayload,
    width: i32,
    height: i32,
    thumb_key: &str,
    webp_key: &str,
    thumb_written: bool,
    webp_written: bool,
    public_url: &str,
) -> Result<(), PipelineError> {
    let thumb_url = format!("{}/u/thumb-{}", public_url, task.image_id);
    let webp_url = format!("{}/u/webp-{}", public_url, task.image_id);
    sqlx::query(
        r#"UPDATE images SET
            width = $1, height = $2,
            thumbnail_key = $3, thumbnail_url = $4,
            webp_key = $5, webp_url = $6,
            status = 'ready'
           WHERE id = $7"#,
    )
    .bind(width)
    .bind(height)
    .bind(some_if(thumb_written, thumb_key))
    .bind(some_if(thumb_written, thumb_url.as_str()))
    .bind(some_if(webp_written, webp_key))
    .bind(some_if(webp_written, webp_url.as_str()))
    .bind(task.image_id)
    .execute(pool)
    .await
    .map_err(|e| PipelineError::Database(e.to_string()))?;
    Ok(())
}

fn some_if(flag: bool, val: &str) -> Option<&str> {
    if flag { Some(val) } else { None }
}
