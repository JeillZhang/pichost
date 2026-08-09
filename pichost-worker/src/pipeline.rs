use std::sync::Arc;
use pichost_core::DbType;

use pichost_core::config::AppConfig;
use pichost_core::crypto::decode_key;
use pichost_core::models::UserStorageConfig;
use pichost_core::storage::StorageBackend;
use pichost_core::StorageRouter;
use sqlx::Pool;

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
    #[error("watermark error: {0}")]
    Watermark(String),
    #[error("database update failed: {0}")]
    Database(String),
    #[error("backend resolution failed: {0}")]
    BackendResolution(String),
}

use crate::queue::TaskPayload;

async fn load_watermarked_image<DB: DbType>(
    pool: &Pool<DB>,
    task: &TaskPayload,
    raw_img: image::DynamicImage,
) -> Result<image::DynamicImage, PipelineError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
{
    let watermark_config = fetch_watermark_config(pool, task).await?;
    match watermark_config {
        Some(ref wm_cfg) if wm_cfg.enabled && !wm_cfg.text.is_empty() => {
            crate::watermark::apply_watermark(&raw_img, wm_cfg)
                .map_err(PipelineError::Watermark)
        }
        _ => Ok(raw_img),
    }
}

async fn fetch_watermark_config<DB: DbType>(
    pool: &Pool<DB>,
    task: &TaskPayload,
) -> Result<Option<pichost_core::models::WatermarkConfig>, PipelineError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<serde_json::Value>,): crate::db::DbRow<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
{
    let watermark_config: Option<pichost_core::models::WatermarkConfig> =
        sqlx::query_scalar::<DB, Option<serde_json::Value>>(
            "SELECT watermark_config FROM users WHERE id = $1",
        )
        .bind(task.user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| PipelineError::Database(format!("Failed to fetch watermark config: {e}")))?
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok());
    Ok(watermark_config)
}

pub async fn process_task<DB: DbType>(
    pool: &Pool<DB>,
    router: &StorageRouter,
    config: &AppConfig,
    task: &TaskPayload,
) -> Result<(), PipelineError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    str: sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    usize: sqlx::ColumnIndex<DB::Row>,
    for<'q> Option<&'q str>: sqlx::Encode<'q, DB>,
    for<'q> Option<&'q str>: sqlx::Type<DB>,
{
    let backend = resolve_backend(pool, router, config, task).await?;

    let (raw_img, fmt, _bytes) = read_source_image(backend.as_ref(), task).await?;
    let (width, height) = (raw_img.width() as i32, raw_img.height() as i32);

    let img = load_watermarked_image(pool, task, raw_img).await?;

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
async fn resolve_backend<DB: DbType>(
    pool: &Pool<DB>,
    router: &StorageRouter,
    config: &AppConfig,
    task: &TaskPayload,
) -> Result<Arc<dyn StorageBackend>, PipelineError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
{
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
async fn fetch_storage_config<DB: DbType>(
    pool: &Pool<DB>,
    config_id: &uuid::Uuid,
) -> Result<UserStorageConfig, PipelineError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
{
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
async fn update_image_record<DB: DbType>(
    pool: &Pool<DB>,
    task: &TaskPayload,
    width: i32,
    height: i32,
    thumb_key: &str,
    webp_key: &str,
    thumb_written: bool,
    webp_written: bool,
    public_url: &str,
) -> Result<(), PipelineError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    for<'q> Option<&'q str>: sqlx::Encode<'q, DB>,
    for<'q> Option<&'q str>: sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    str: sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let thumb_url = format!("{}/u/thumb/{}", public_url, task.image_id);
    let webp_url = format!("{}/u/webp/{}", public_url, task.image_id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use pichost_core::config::AppConfig;
    use pichost_core::error::StorageError;
    use pichost_core::storage::local::LocalStorage;
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    struct MockBackend {
        items: Mutex<Vec<(String, Vec<u8>)>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self { items: Mutex::new(Vec::new()) }
        }
        fn stored(&self, key: &str) -> Option<Vec<u8>> {
            self.items
                .lock()
                .unwrap()
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, d)| d.clone())
        }
    }

    impl StorageBackend for MockBackend {
        fn put<'l0, 'l1, 'l2, 'l3, 'a>(
            &'l0 self,
            key: &'l1 str,
            data: &'l2 [u8],
            _ct: &'l3 str,
        ) -> Pin<Box<dyn Future<Output = Result<String, StorageError>> + Send + 'a>>
        where
            'l0: 'a,
            'l1: 'a,
            'l2: 'a,
            'l3: 'a,
            Self: 'a,
        {
            Box::pin(async move {
                self.items.lock().unwrap().push((key.to_string(), data.to_vec()));
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
                Ok(self.stored(key).unwrap_or_default())
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
                self.items.lock().unwrap().retain(|(k, _)| k != key);
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
                Ok(self.items.lock().unwrap().iter().any(|(k, _)| k == key))
            })
        }
        fn public_url(&self, key: &str) -> String {
            format!("/u/{key}")
        }
        fn backend_name(&self) -> &str {
            "mock"
        }
    }

    fn sample_task() -> TaskPayload {
        TaskPayload {
            task_id: uuid::Uuid::new_v4(),
            image_id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            storage_backend: "local".into(),
            storage_config_id: None,
            storage_backend_name: "Local".into(),
            source_key: "source.png".into(),
            source_mime: "image/png".into(),
            retry_count: 0,
            max_retries: 3,
        }
    }

    fn png_bytes() -> Vec<u8> {
        let img = image::DynamicImage::new_rgba8(8, 8);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn test_some_if() {
        assert_eq!(some_if(true, "x"), Some("x"));
        assert_eq!(some_if(false, "x"), None);
    }

    #[test]
    fn test_resolve_encryption_key_none() {
        assert_eq!(resolve_encryption_key(&AppConfig::default()), [0u8; 32]);
    }

    #[test]
    fn test_resolve_encryption_key_valid() {
        let valid = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
        assert_eq!(pichost_core::crypto::decode_key(valid).unwrap(), [1u8; 32]);
        let cfg = AppConfig {
            token_encryption_key: Some(valid.to_string()),
            ..AppConfig::default()
        };
        assert_eq!(resolve_encryption_key(&cfg), [1u8; 32]);
    }

    #[test]
    fn test_resolve_encryption_key_invalid_base64() {
        let cfg = AppConfig {
            token_encryption_key: Some("!!!not-base64!!!".to_string()),
            ..AppConfig::default()
        };
        assert_eq!(resolve_encryption_key(&cfg), [0u8; 32]);
    }

    #[tokio::test]
    async fn test_read_source_image_ok() {
        let backend = MockBackend::new();
        backend.put("source.png", &png_bytes(), "image/png").await.unwrap();
        let task = sample_task();
        let (img, fmt, bytes) = read_source_image(&backend, &task).await.unwrap();
        assert_eq!(fmt, image::ImageFormat::Png);
        assert_eq!((img.width(), img.height()), (8, 8));
        assert!(!bytes.is_empty());
    }

    #[tokio::test]
    async fn test_read_source_image_decode_error() {
        let backend = MockBackend::new();
        backend.put("bad", b"not an image at all", "image/png").await.unwrap();
        let task = sample_task();
        let err = read_source_image(&backend, &task).await.unwrap_err();
        assert!(matches!(err, PipelineError::Decode(_)));
    }

    #[tokio::test]
    async fn test_process_image_variants_png() {
        let backend = MockBackend::new();
        let img = image::DynamicImage::new_rgba8(16, 16);
        let cfg = AppConfig::default();
        let result = process_image_variants(
            &img,
            image::ImageFormat::Png,
            &backend,
            "thumb",
            "webp",
            &cfg,
        )
        .await
        .unwrap();
        assert_eq!(result, (true, true));
        assert!(backend.stored("thumb").is_some());
        assert!(backend.stored("webp").is_some());
    }

    #[tokio::test]
    async fn test_process_image_variants_gif_none_written() {
        let backend = MockBackend::new();
        let img = image::DynamicImage::new_rgba8(16, 16);
        let cfg = AppConfig::default();
        let result = process_image_variants(
            &img,
            image::ImageFormat::Gif,
            &backend,
            "thumb",
            "webp",
            &cfg,
        )
        .await
        .unwrap();
        assert_eq!(result, (false, false));
        assert!(backend.stored("thumb").is_none());
        assert!(backend.stored("webp").is_none());
    }

    #[test]
    fn test_pipeline_error_display() {
        assert_eq!(PipelineError::StorageRead("r".into()).to_string(), "storage read failed: r");
        assert_eq!(PipelineError::StorageWrite("w".into()).to_string(), "storage write failed: w");
        assert_eq!(PipelineError::Decode("d".into()).to_string(), "image decode failed: d");
        assert_eq!(PipelineError::Thumbnail("t".into()).to_string(), "thumbnail generation failed: t");
        assert_eq!(PipelineError::Webp("v".into()).to_string(), "webp conversion failed: v");
        assert_eq!(PipelineError::Watermark("m".into()).to_string(), "watermark error: m");
        assert_eq!(PipelineError::Database("b".into()).to_string(), "database update failed: b");
        assert_eq!(PipelineError::BackendResolution("z".into()).to_string(), "backend resolution failed: z");
    }

    #[tokio::test]
    async fn test_mock_backend_misc_methods() {
        let backend = MockBackend::new();
        backend.put("k", b"v", "text/plain").await.unwrap();
        assert!(backend.exists("k").await.unwrap());
        assert!(!backend.exists("nope").await.unwrap());
        assert_eq!(backend.public_url("k"), "/u/k");
        assert_eq!(backend.backend_name(), "mock");
        backend.delete("k").await.unwrap();
        assert!(!backend.exists("k").await.unwrap());
    }

    const TEST_DB_URL: &str = "postgres://pichost:pichost@localhost:5432/pichost";

    type ImageRow = (
        i32,
        i32,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    );

    async fn test_pg_pool() -> sqlx::PgPool {
        let pool = crate::db::create_pg_pool(TEST_DB_URL, 5).await.unwrap();
        crate::db::run_pg_migrations(&pool).await.unwrap();
        pool
    }

    async fn insert_user_with_watermark<DB: DbType>(
        pool: &Pool<DB>,
        watermark: Option<serde_json::Value>,
    ) -> uuid::Uuid
    where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    Option<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
        let user_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, watermark_config) \
             VALUES ($1, $2, 'x', $3)",
        )
        .bind(user_id)
        .bind(format!("wm_test_{user_id}"))
        .bind(watermark)
        .execute(pool)
        .await
        .unwrap();
        user_id
    }

    async fn insert_image<DB: DbType>(pool: &Pool<DB>, user_id: uuid::Uuid) -> uuid::Uuid
    where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
        let image_id = uuid::Uuid::new_v4();
        let pk = format!("{:x}", image_id)[..16].to_string();
        sqlx::query(
            "INSERT INTO images (id, user_id, public_key, original_name, storage_key, \
             storage_backend, mime_type, file_size, sha256, url, status) \
             VALUES ($1, $2, $3, 'n', 'source.png', 'local', 'image/png', 1, $4, 'u', 'active')",
        )
        .bind(image_id)
        .bind(user_id)
        .bind(pk)
        .bind("a".repeat(64))
        .execute(pool)
        .await
        .unwrap();
        image_id
    }

    async fn cleanup_user<DB: DbType>(pool: &Pool<DB>, user_id: uuid::Uuid)
    where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .unwrap();
    }

    fn task_for(user_id: uuid::Uuid, image_id: uuid::Uuid) -> TaskPayload {
        let mut t = sample_task();
        t.user_id = user_id;
        t.image_id = image_id;
        t
    }

    fn canvas(w: u32, h: u32) -> image::DynamicImage {
        image::DynamicImage::new_rgba8(w, h)
    }

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("phw-pipe-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn local_router(dir: &TempDir) -> StorageRouter {
        let local: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(
            dir.path().to_path_buf(),
            "http://localhost:3000".into(),
        ));
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), local);
        StorageRouter::new(backends, "local".into())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_fetch_watermark_config_some() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(
            &pool,
            Some(serde_json::json!({
                "enabled": true, "text": "@test", "position": "bottom-right"
            })),
        )
        .await;
        let task = task_for(user_id, uuid::Uuid::new_v4());
        let cfg = fetch_watermark_config(&pool, &task).await.unwrap().unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.text, "@test");
        assert_eq!(
            cfg.position,
            pichost_core::models::WatermarkPosition::BottomRight
        );
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_fetch_watermark_config_none_and_invalid() {
        let pool = test_pg_pool().await;
        let plain = insert_user_with_watermark(&pool, None).await;
        let bad = insert_user_with_watermark(
            &pool,
            Some(serde_json::json!({"enabled": "not-a-bool"})),
        )
        .await;
        let t1 = task_for(plain, uuid::Uuid::new_v4());
        let t2 = task_for(bad, uuid::Uuid::new_v4());
        assert!(fetch_watermark_config(&pool, &t1).await.unwrap().is_none());
        assert!(fetch_watermark_config(&pool, &t2).await.unwrap().is_none());
        cleanup_user(&pool, plain).await;
        cleanup_user(&pool, bad).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_load_watermarked_image_enabled() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(
            &pool,
            Some(serde_json::json!({"enabled": true, "text": "@test"})),
        )
        .await;
        let task = task_for(user_id, uuid::Uuid::new_v4());
        let raw = canvas(200, 100);
        let out = load_watermarked_image(&pool, &task, raw.clone()).await.unwrap();
        assert_ne!(out.to_rgba8().into_raw(), raw.to_rgba8().into_raw());
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_load_watermarked_image_disabled_returns_clone() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(
            &pool,
            Some(serde_json::json!({"enabled": false, "text": "@test"})),
        )
        .await;
        let task = task_for(user_id, uuid::Uuid::new_v4());
        let raw = canvas(64, 64);
        let out = load_watermarked_image(&pool, &task, raw.clone()).await.unwrap();
        assert_eq!(out.to_rgba8().into_raw(), raw.to_rgba8().into_raw());
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_update_image_record_writes_variants() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(&pool, None).await;
        let image_id = insert_image(&pool, user_id).await;
        let task = task_for(user_id, image_id);
        update_image_record(
            &pool, &task, 200, 100, "t/thumb", "t/webp", true, true,
            "http://localhost",
        )
        .await
        .unwrap();
        let row: ImageRow = sqlx::query_as(
            "SELECT width, height, thumbnail_key, thumbnail_url, webp_key, webp_url, status \
             FROM images WHERE id = $1",
        )
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((row.0, row.1), (200, 100));
        assert_eq!(row.2.as_deref(), Some("t/thumb"));
        assert!(row.3.as_deref().unwrap().starts_with("http://localhost/u/thumb/"));
        assert_eq!(row.4.as_deref(), Some("t/webp"));
        assert!(row.5.as_deref().unwrap().starts_with("http://localhost/u/webp/"));
        assert_eq!(row.6, "ready");
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_update_image_record_no_variants_null_keys() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(&pool, None).await;
        let image_id = insert_image(&pool, user_id).await;
        let task = task_for(user_id, image_id);
        update_image_record(&pool, &task, 10, 20, "t", "w", false, false, "http://localhost")
            .await
            .unwrap();
        let row: (Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT thumbnail_key, webp_key, status FROM images WHERE id = $1",
        )
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(row.0.is_none());
        assert!(row.1.is_none());
        assert_eq!(row.2, "ready");
        cleanup_user(&pool, user_id).await;
    }

    async fn insert_storage_config<DB: DbType>(
        pool: &Pool<DB>,
        user_id: uuid::Uuid,
    ) -> uuid::Uuid
    where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
        let config_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO user_storage_configs (id, user_id, name, provider, config) \
             VALUES ($1, $2, 'cfg', 'local', '{}'::jsonb)",
        )
        .bind(config_id)
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
        config_id
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_fetch_storage_config_found_and_missing() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(&pool, None).await;
        let config_id = insert_storage_config(&pool, user_id).await;
        let cfg = fetch_storage_config(&pool, &config_id).await.unwrap();
        assert_eq!(cfg.id, config_id);
        assert_eq!(cfg.provider, "local");
        let err = fetch_storage_config(&pool, &uuid::Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, PipelineError::BackendResolution(_)));
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_resolve_backend_local_and_config() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(&pool, None).await;
        let config_id = insert_storage_config(&pool, user_id).await;
        let dir = TempDir::new();
        let router = local_router(&dir);
        let cfg = AppConfig::default();

        let plain = task_for(user_id, uuid::Uuid::new_v4());
        let backend = resolve_backend(&pool, &router, &cfg, &plain).await.unwrap();
        assert_eq!(backend.backend_name(), "local");

        let mut with_cfg = task_for(user_id, uuid::Uuid::new_v4());
        with_cfg.storage_config_id = Some(config_id);
        let backend = resolve_backend(&pool, &router, &cfg, &with_cfg).await.unwrap();
        assert_eq!(backend.backend_name(), "local");
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_process_task_end_to_end() {
        let pool = test_pg_pool().await;
        let user_id = insert_user_with_watermark(&pool, None).await;
        let image_id = insert_image(&pool, user_id).await;

        let dir = TempDir::new();
        let router = local_router(&dir);
        let img = canvas(200, 100);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(dir.path().join("source.png"), &buf).unwrap();

        let task = task_for(user_id, image_id);
        let cfg = AppConfig::default();
        process_task(&pool, &router, &cfg, &task).await.unwrap();

        let row: (i32, i32, Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT width, height, thumbnail_key, webp_key, status FROM images WHERE id = $1",
        )
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((row.0, row.1), (200, 100));
        assert_eq!(row.4, "ready");
        let thumb_key = row.2.unwrap();
        let webp_key = row.3.unwrap();
        assert!(dir.path().join(&thumb_key).exists());
        assert!(dir.path().join(&webp_key).exists());
        cleanup_user(&pool, user_id).await;
    }
}
