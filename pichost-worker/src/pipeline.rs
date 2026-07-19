use std::sync::Arc;

use pichost_core::config::AppConfig;
use pichost_core::crypto::decode_key;
use pichost_core::models::UserStorageConfig;
use pichost_core::storage::StorageBackend;
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
    #[error("backend resolution failed: {0}")]
    BackendResolution(String),
}

use crate::queue::TaskPayload;

pub async fn process_task(
    pool: &PgPool,
    router: &StorageRouter,
    config: &AppConfig,
    task: &TaskPayload,
) -> Result<(), PipelineError> {
    let backend = resolve_backend(pool, router, config, task).await?;

    let (img, fmt, _bytes) = read_source_image(backend.as_ref(), task).await?;
    let (width, height) = (img.width() as i32, img.height() as i32);
    let thumb_key = format!("{}/thumb.{}", task.user_id, task.image_id);
    let webp_key = format!("{}/webp.{}", task.user_id, task.image_id);

    let (thumb_written, webp_written) = process_image_variants(
        &img, fmt, backend.as_ref(), &thumb_key, &webp_key, config,
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
        backend_name = task.storage_backend_name,
        "processing complete"
    );
    Ok(())
}

/// Resolve the storage backend for this task.
///
/// If the task references a storage config (git backends), the config is
/// fetched from the database and a dynamic backend is created via
/// `router.for_config()`. Otherwise falls back to `router.for_backend()`.
async fn resolve_backend(
    pool: &PgPool,
    router: &StorageRouter,
    config: &AppConfig,
    task: &TaskPayload,
) -> Result<Arc<dyn StorageBackend>, PipelineError> {
    if let Some(config_id) = &task.storage_config_id {
        let storage_config = fetch_storage_config(pool, config_id).await?;
        let enc_key = resolve_encryption_key(config);
        router
            .for_config(&storage_config, &enc_key)
            .map_err(|e| PipelineError::BackendResolution(e.to_string()))
    } else {
        Ok(router.for_backend(&task.storage_backend))
    }
}

/// Fetch a user storage config by ID from the database.
async fn fetch_storage_config(
    pool: &PgPool,
    config_id: &uuid::Uuid,
) -> Result<UserStorageConfig, PipelineError> {
    sqlx::query_as::<_, UserStorageConfig>(
        "SELECT id, user_id, name, provider, is_default, \
         config, created_at, updated_at \
         FROM user_storage_configs WHERE id = $1",
    )
    .bind(config_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        PipelineError::BackendResolution(format!("config db query failed: {e}"))
    })?
    .ok_or_else(|| {
        PipelineError::BackendResolution(format!(
            "storage config {} not found",
            config_id
        ))
    })
}

/// Decode the token encryption key from config, falling back to a zeroed key
/// if none is configured.
fn resolve_encryption_key(config: &AppConfig) -> [u8; 32] {
    config
        .token_encryption_key
        .as_ref()
        .and_then(|k| decode_key(k).ok())
        .unwrap_or([0u8; 32])
}

async fn process_image_variants(
    img: &image::DynamicImage,
    fmt: image::ImageFormat,
    source_backend: &(dyn StorageBackend + '_),
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

/// Read and decode the source image from the given storage backend.
async fn read_source_image(
    backend: &(dyn StorageBackend + '_),
    task: &TaskPayload,
) -> Result<(image::DynamicImage, image::ImageFormat, Vec<u8>), PipelineError> {
    let bytes = backend
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
