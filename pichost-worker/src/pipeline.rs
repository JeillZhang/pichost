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

    let (width, height) = (img.width() as i32, img.height() as i32);
    let fmt = image::guess_format(&bytes).map_err(|e| PipelineError::Decode(e.to_string()))?;

    let thumb_key = format!("{}/thumb.{}", task.user_id, task.image_id);
    let webp_key = format!("{}/webp.{}", task.user_id, task.image_id);
    let public_url = config.server.public_url.trim_end_matches('/');
    let thumb_url = format!("{}/u/thumb-{}", public_url, task.image_id);
    let webp_url = format!("{}/u/webp-{}", public_url, task.image_id);

    let (thumb_written, _thumb_mime) = processor::generate_thumbnail(
        &img,
        fmt,
        source_backend.as_ref(),
        &thumb_key,
        config.worker.processing.thumbnail_size,
        config.worker.processing.thumbnail_quality,
    )
    .await
    .map_err(PipelineError::Thumbnail)?;

    let (webp_written, _webp_mime) = processor::convert_to_webp(
        &img,
        fmt,
        source_backend.as_ref(),
        &webp_key,
        config.worker.processing.webp_quality,
    )
    .await
    .map_err(PipelineError::Webp)?;

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
    .bind(if thumb_written {
        Some(&thumb_key)
    } else {
        None::<&String>
    })
    .bind(if thumb_written {
        Some(&thumb_url)
    } else {
        None::<&String>
    })
    .bind(if webp_written {
        Some(&webp_key)
    } else {
        None::<&String>
    })
    .bind(if webp_written {
        Some(&webp_url)
    } else {
        None::<&String>
    })
    .bind(task.image_id)
    .execute(pool)
    .await
    .map_err(|e| PipelineError::Database(e.to_string()))?;

    tracing::info!(
        image_id = %task.image_id, width, height,
        thumb = thumb_written, webp = webp_written,
        backend = task.storage_backend,
        "processing complete"
    );

    Ok(())
}
