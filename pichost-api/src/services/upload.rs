use std::sync::Arc;

use axum::extract::Multipart;
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sqlx::PgPool;
use uuid::Uuid;

use crate::app::AppState;
use crate::cache::CachePool;
use crate::middleware::auth::AuthUser;
use crate::services::html_escape;
use deadpool_redis::redis::AsyncCommands;
use pichost_core::crypto::decode_key;
use pichost_core::models::UserStorageConfig;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageConfigInfo {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult {
    pub id: Uuid,
    pub public_key: String,
    pub original_name: String,
    pub url: String,
    pub markdown: String,
    pub html: String,
    pub bbcode: String,
    pub sha256: String,
    pub file_size: i64,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub status: String,
    pub thumbnail_url: Option<String>,
    pub webp_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub storage_config: Option<StorageConfigInfo>,
}

/// Full image-row tuple used by list_images / get_image queries.
/// Fields: id, public_key, original_name, url, mime_type, file_size,
///         sha256, width, height, status, thumbnail_url, webp_url, created_at,
///         storage_config_id, config_name, config_provider
pub(crate) type ImageRow = (
    Uuid,
    String,
    String,
    String,
    String,
    i64,
    String,
    Option<i32>,
    Option<i32>,
    String,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
    Option<Uuid>,
    Option<String>,
    Option<String>,
);

impl UploadResult {
    /// Build an UploadResult from a DB row tuple. Generates markdown, HTML and
    /// bbcode link formats from original_name + url.
    pub(crate) fn from_row(row: ImageRow) -> Self {
        let (id, pk, name, url, mime, size, sha256, w, h, status, thumb, webp,
             created, cfg_id, cfg_name, cfg_provider) = row;
        let storage_config = match (cfg_id, cfg_name, cfg_provider) {
            (Some(id), Some(name), Some(provider)) => Some(StorageConfigInfo {
                id,
                name,
                provider,
            }),
            _ => None,
        };
        Self {
            id,
            public_key: pk,
            original_name: name,
            url,
            markdown: String::new(),
            html: String::new(),
            bbcode: String::new(),
            sha256,
            file_size: size,
            mime_type: mime,
            width: w,
            height: h,
            status,
            thumbnail_url: thumb,
            webp_url: webp,
            created_at: created,
            storage_config,
        }
    }
}

/// Query parameters for GET /api/v1/images
#[derive(Debug, Deserialize)]
pub struct ImageListQuery {
    /// Page number (1-based, default 1)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (default 20, max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    /// Sort field: "created_at", "file_size", "original_name"
    #[serde(default = "default_sort")]
    pub sort: String,
    /// Sort order: "asc" or "desc"
    #[serde(default = "default_order")]
    pub order: String,
    /// Optional search term (ILIKE match against original_name)
    #[serde(default)]
    pub search: String,
    /// Optional storage config ID filter
    #[serde(default)]
    pub storage_config_id: Option<Uuid>,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }
fn default_sort() -> String { "created_at".to_string() }
fn default_order() -> String { "desc".to_string() }

/// Paginated response envelope
#[derive(Debug, Serialize)]
pub struct ImageListResponse {
    pub items: Vec<UploadResult>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

// ── Enqueue helper ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn enqueue_processing_task(
    redis_pool: &CachePool,
    image_id: Uuid,
    user_id: Uuid,
    storage_key: &str,
    mime_type: &str,
    storage_backend: &str,
    storage_config_id: Option<Uuid>,
    storage_backend_name: &str,
) {
    let task_id = Uuid::new_v4();
    let mut payload = serde_json::json!({
        "task_id": task_id.to_string(),
        "image_id": image_id.to_string(),
        "user_id": user_id.to_string(),
        "storage_backend": storage_backend,
        "source_key": storage_key,
        "source_mime": mime_type,
        "retry_count": 0,
        "max_retries": 3,
        "storage_backend_name": storage_backend_name,
    });
    if let Some(cid) = storage_config_id {
        payload["storage_config_id"] = serde_json::Value::String(cid.to_string());
    }

    let pool_err = |e: deadpool_redis::PoolError| {
        tracing::warn!("redis pool error during enqueue: {e}");
    };

    let mut conn = match redis_pool.get().await {
        Ok(c) => c,
        Err(e) => {
            pool_err(e);
            return;
        }
    };

    let payload_json = serde_json::to_string(&payload).unwrap_or_default();
    let now = chrono::Utc::now().to_rfc3339();
    let task_key = format!("pichost:task:{task_id}");

    // Store task data hash — field names must match queue.rs convention
    let _: Result<(), _> = conn.hset(&task_key, "data", &payload_json).await;
    let _: Result<(), _> = conn.hset(&task_key, "status", "pending").await;
    let _: Result<(), _> = conn.hset(&task_key, "created_at", &now).await;
    let _: Result<(), _> = conn.hset(&task_key, "updated_at", &now).await;
    // Push to pending queue
    let _: Result<(), _> = conn
        .lpush("pichost:tasks:pending", task_id.to_string())
        .await;

    tracing::info!(%task_id, %image_id, "enqueued processing task");
}

// ── Private helpers ────────────────────────────────────────────────────────

type ApiError = (StatusCode, Json<serde_json::Value>);

/// Extracts the first `file` field from a multipart body.
/// Returns raw bytes + original filename (defaults to "file").
pub async fn extract_file_from_multipart(
    mut multipart: Multipart,
) -> Result<(Vec<u8>, String), ApiError> {
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            let file_name = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "file".to_string());
            let data = field.bytes().await.map_err(|e| {
                let json = serde_json::json!({"error": format!("failed to read file: {e}")});
                (StatusCode::BAD_REQUEST, Json(json))
            })?;
            return Ok((data.to_vec(), file_name));
        }
    }
    Err((
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": "no file field found in upload"})),
    ))
}

/// Validates file against max-size limits (admin vs user) and per-user
/// storage quota.  file_size is in bytes.
async fn check_upload_quotas(
    state: &AppState,
    user: &AuthUser,
    file_size: u64,
) -> Result<(), ApiError> {
    let max_size = if user.is_admin {
        state.config.upload.max_file_size_admin
    } else {
        state.config.upload.max_file_size_user
    };
    if file_size > max_size {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "file exceeds maximum allowed size"})),
        ));
    }

    if let Some(quota) = user.storage_quota {
        let current_usage: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(file_size), 0) FROM images WHERE user_id = $1",
        )
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Quota usage query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

        let new_file_size = file_size as i64;
        if current_usage + new_file_size > quota {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(serde_json::json!({
                    "error": "storage quota exceeded",
                    "quota_bytes": quota,
                    "used_bytes": current_usage,
                    "file_bytes": new_file_size,
                })),
            ));
        }
    }
    Ok(())
}

/// Checks whether this user already uploaded the same content for a
/// specific storage config (per-user, per-sha256, per-config dedup).
/// Returns `Some(UploadResult)` if a duplicate exists, `None` otherwise.
async fn try_dedup(
    state: &AppState,
    user_id: Uuid,
    sha256: &str,
    storage_config_id: Option<Uuid>,
) -> Result<Option<UploadResult>, ApiError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM images \
         WHERE user_id=$1 AND sha256=$2 \
           AND storage_config_id IS NOT DISTINCT FROM $3)",
    )
    .bind(user_id)
    .bind(sha256)
    .bind(storage_config_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Dedup query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    if !exists {
        return Ok(None);
    }

    let row = sqlx::query_as::<_, ImageRow>(
        "SELECT id, public_key, original_name, url, mime_type, file_size, \
         sha256, width, height, status, thumbnail_url, webp_url, \
         created_at, storage_config_id \
         FROM images \
         WHERE user_id = $1 AND sha256 = $2 \
           AND storage_config_id IS NOT DISTINCT FROM $3",
    )
    .bind(user_id)
    .bind(sha256)
    .bind(storage_config_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to fetch existing image: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    Ok(Some(UploadResult::from_row(row)))
}

/// Generates a collision-free 6-char hex public key.
async fn generate_public_key(state: &AppState) -> Result<String, ApiError> {
    use rand::Rng;
    loop {
        let key = format!("{:06x}", rand::thread_rng().gen::<u32>() & 0xFFFFFF);
        let key_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM images WHERE public_key=$1)")
                .bind(&key)
                .fetch_one(&state.pool)
                .await
                .map_err(|e| {
                    tracing::warn!("Public key uniqueness check failed: {e}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "internal server error"})),
                    )
                })?;
        if !key_exists {
            return Ok(key);
        }
    }
}

/// Detect MIME type from raw bytes. Falls back to application/octet-stream.
fn detect_mime(bytes: &[u8]) -> String {
    infer::get(bytes)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

/// Detects image width × height from raw bytes.
/// Returns `(None, None)` on failure — callers degrade gracefully.
fn image_dimensions(bytes: &[u8]) -> (Option<i32>, Option<i32>) {
    let cursor = std::io::Cursor::new(bytes);
    match image::ImageReader::new(cursor).with_guessed_format() {
        Ok(reader) => match reader.into_dimensions() {
            Ok((w, h)) => (Some(w as i32), Some(h as i32)),
            Err(e) => {
                tracing::warn!("Failed to decode image dimensions: {e}");
                (None, None)
            }
        },
        Err(e) => {
            tracing::warn!("Failed to create image reader: {e}");
            (None, None)
        }
    }
}

// ── Config resolution helpers ──────────────────────────────────────────────

/// Decode the token encryption key from config, returning a zeroed key if
/// none is configured (local-only uploads still work).
fn resolve_encryption_key(config: &pichost_core::config::AppConfig) -> Result<[u8; 32], ApiError> {
    match &config.token_encryption_key {
        Some(encoded) => decode_key(encoded).map_err(|e| {
            tracing::warn!("Invalid token encryption key: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "server misconfiguration: invalid encryption key"
                })),
            )
        }),
        None => Ok([0u8; 32]),
    }
}

/// Resolve which storage configs to use for this upload.
/// - `Some(ids)`: lookup specific configs (must belong to user).
/// - `None`: fall back to user's default config, or local if none exists.
async fn resolve_upload_configs(
    pool: &PgPool,
    user_id: Uuid,
    storage_config_ids: Option<Vec<Uuid>>,
) -> Result<Vec<UserStorageConfig>, ApiError> {
    if let Some(ids) = storage_config_ids {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let configs = sqlx::query_as::<_, UserStorageConfig>(
            "SELECT id, user_id, name, provider, is_default, \
             config, created_at, updated_at \
             FROM user_storage_configs \
             WHERE id = ANY($1) AND user_id = $2 \
             ORDER BY created_at",
        )
        .bind(&ids)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to resolve upload configs: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

        if configs.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "storage_config_ids: no matching configs found"
                })),
            ));
        }
        return Ok(configs);
    }

    // Backward compat: use user's default config, or local fallback
    let config = sqlx::query_as::<_, UserStorageConfig>(
        "SELECT id, user_id, name, provider, is_default, \
         config, created_at, updated_at \
         FROM user_storage_configs \
         WHERE user_id = $1 AND is_default = true",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to fetch default config: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    if let Some(c) = config {
        return Ok(vec![c]);
    }

    // No configs exist — synthesize a local pseudo-config
    Ok(vec![UserStorageConfig {
        id: Uuid::nil(),
        user_id,
        name: "Local Storage".into(),
        provider: "local".into(),
        is_default: false,
        config: serde_json::json!({}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }])
}

/// Validate upload configs: max 2 configs, at least one must be "local".
fn validate_upload_configs(configs: &[UserStorageConfig]) -> Result<(), ApiError> {
    if configs.len() > 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "at most 2 storage_config_ids are allowed"
            })),
        ));
    }
    if !configs.iter().any(|c| c.provider == "local") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "at least one storage config must use 'local' provider"
            })),
        ));
    }
    Ok(())
}

// ── Storage / DB persistence helpers ───────────────────────────────────────

/// Writes bytes to the given storage backend and builds the public URL.
/// Returns `(storage_key, url, backend_name)`.
async fn write_to_storage(
    storage: &Arc<dyn pichost_core::storage::StorageBackend>,
    public_url: &str,
    user_id: Uuid,
    public_key: &str,
    bytes: &[u8],
    mime_type: &str,
) -> Result<(String, String, String), ApiError> {
    let storage_key = format!("{}/{}", user_id, public_key);
    storage
        .put(&storage_key, bytes, mime_type)
        .await
        .map_err(|e| {
            tracing::warn!("Storage write failed on {}: {e}", storage.backend_name());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage write failed"})),
            )
        })?;
    let backend_name = storage.backend_name().to_string();
    let url = if backend_name == "local" {
        format!("{}/u/{}", public_url.trim_end_matches('/'), public_key)
    } else {
        storage.public_url(&storage_key)
    };
    Ok((storage_key, url, backend_name))
}

/// Inserts a new image record into the database, optionally linking a config.
#[allow(clippy::too_many_arguments)]
async fn insert_image_record(
    pool: &PgPool,
    user_id: Uuid,
    public_key: &str,
    original_name: &str,
    storage_key: &str,
    storage_backend: &str,
    mime_type: &str,
    file_size: i64,
    width: Option<i32>,
    height: Option<i32>,
    sha256: &str,
    url: &str,
    storage_config_id: Option<Uuid>,
) -> Result<Uuid, ApiError> {
    sqlx::query_scalar(
        r#"INSERT INTO images
           (user_id, public_key, original_name, storage_key, storage_backend,
            mime_type, file_size, width, height, sha256, url, status,
            storage_config_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'active', $12)
           RETURNING id"#,
    )
    .bind(user_id)
    .bind(public_key)
    .bind(original_name)
    .bind(storage_key)
    .bind(storage_backend)
    .bind(mime_type)
    .bind(file_size)
    .bind(width)
    .bind(height)
    .bind(sha256)
    .bind(url)
    .bind(storage_config_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Image insert failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "failed to save image metadata"})),
        )
    })
}

/// Orchestrates storage write + DB insert for one backend config.
/// Returns `(image_id, storage_key, backend_key, public_key, url)`.
#[allow(clippy::too_many_arguments)]
async fn persist_image_for_config(
    state: &AppState,
    user: &AuthUser,
    config: &UserStorageConfig,
    encryption_key: &[u8; 32],
    original_name: &str,
    bytes: &[u8],
    mime_type: &str,
    width: Option<i32>,
    height: Option<i32>,
    sha256: &str,
) -> Result<(Uuid, String, String, String, String), ApiError> {
    let storage = state.router.for_config(config, encryption_key).map_err(|e| {
        tracing::warn!("Failed to resolve backend for config {}: {e}", config.id);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "failed to resolve storage backend"})),
        )
    })?;

    let public_key = generate_public_key(state).await?;

    let (storage_key, url, backend_name) = write_to_storage(
        &storage,
        &state.config.server.public_url,
        user.id,
        &public_key,
        bytes,
        mime_type,
    )
    .await?;

    // For non-local backends, store the config UUID as storage_backend
    // so for_backend() can find the cached dynamic backend.
    let db_backend = if config.provider == "local" {
        "local".to_string()
    } else {
        config.id.to_string()
    };
    let db_config_id = if config.id.is_nil() { None } else { Some(config.id) };

    let image_id = insert_image_record(
        &state.pool,
        user.id,
        &public_key,
        original_name,
        &storage_key,
        &db_backend,
        mime_type,
        bytes.len() as i64,
        width,
        height,
        sha256,
        &url,
        db_config_id,
    )
    .await?;

    Ok((image_id, storage_key, backend_name, public_key, url))
}

/// Constructs a fully-populated UploadResult for a freshly-inserted image.
#[allow(clippy::too_many_arguments)]
fn build_result(
    id: Uuid,
    public_key: String,
    original_name: String,
    url: String,
    bytes: &[u8],
    mime_type: String,
    width: Option<i32>,
    height: Option<i32>,
    sha256: String,
    storage_config: Option<StorageConfigInfo>,
) -> UploadResult {
    let file_size = bytes.len() as i64;
    let markdown = format!("![{}]({})", original_name, url);
    let html = format!(
        "<img src=\"{}\" alt=\"{}\" />",
        url,
        html_escape(&original_name)
    );
    let bbcode = format!("[img]{}[/img]", url);
    UploadResult {
        id,
        public_key,
        original_name,
        url,
        markdown,
        html,
        bbcode,
        sha256,
        file_size,
        mime_type,
        width,
        height,
        status: "active".to_string(),
        thumbnail_url: None,
        webp_url: None,
        created_at: Utc::now(),
        storage_config,
    }
}

/// Build a StorageConfigInfo from a UserStorageConfig row.
fn config_info(config: &UserStorageConfig) -> StorageConfigInfo {
    StorageConfigInfo {
        id: config.id,
        name: config.name.clone(),
        provider: config.provider.clone(),
    }
}

// ── Upload orchestrators ───────────────────────────────────────────────────

/// Upload the image to a single backend config. Handles dedup, storage,
/// DB insert, and worker enqueue.
#[allow(clippy::too_many_arguments)]
async fn upload_to_single_backend(
    state: &AppState,
    user: &AuthUser,
    config: &UserStorageConfig,
    encryption_key: &[u8; 32],
    bytes: &[u8],
    file_name: &str,
    sha256: &str,
    mime_type: &str,
    width: Option<i32>,
    height: Option<i32>,
) -> Result<UploadResult, ApiError> {
    let config_id = if config.id.is_nil() { None } else { Some(config.id) };

    // Deduplicate per config
    if let Some(existing) = try_dedup(state, user.id, sha256, config_id).await? {
        return Ok(existing);
    }

    let (image_id, storage_key, backend_name, public_key, url) =
        persist_image_for_config(
            state, user, config, encryption_key,
            file_name, bytes, mime_type, width, height, sha256,
        )
        .await?;

    enqueue_processing_task(
        &state.cache.get_pool(),
        image_id,
        user.id,
        &storage_key,
        mime_type,
        &backend_name,
        config_id,
        &backend_name,
    )
    .await;

    Ok(build_result(
        image_id,
        public_key,
        file_name.to_string(),
        url,
        bytes,
        mime_type.to_string(),
        width,
        height,
        sha256.to_string(),
        Some(config_info(config)),
    ))
}

/// Public entry point: process an image upload, potentially to multiple
/// storage backends. Validates the image, resolves configs, then delegates
/// to `upload_to_single_backend` for each config.
///
/// Returns one `UploadResult` per backend.
pub async fn process_upload(
    state: &AppState,
    user: &AuthUser,
    bytes: Vec<u8>,
    file_name: String,
    storage_config_ids: Option<Vec<Uuid>>,
) -> Result<Vec<UploadResult>, ApiError> {
    if !infer::is_image(&bytes) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "file is not a valid image"})),
        ));
    }

    check_upload_quotas(state, user, bytes.len() as u64).await?;

    let sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));
    let configs = resolve_upload_configs(&state.pool, user.id, storage_config_ids).await?;
    validate_upload_configs(&configs)?;

    let encryption_key = resolve_encryption_key(&state.config)?;
    let mime_type = detect_mime(&bytes);
    let (width, height) = image_dimensions(&bytes);

    let mut results = Vec::with_capacity(configs.len());
    for config in &configs {
        let result = upload_to_single_backend(
            state, user, config, &encryption_key,
            &bytes, &file_name, &sha256, &mime_type, width, height,
        )
        .await?;
        results.push(result);
    }

    Ok(results)
}

// ── Gallery queries ────────────────────────────────────────────────────────

/// Query one page of images for a user. Builds sorted, search-filtered SQL.
pub async fn list_user_images(
    pool: &PgPool,
    user_id: Uuid,
    query: &ImageListQuery,
) -> Result<ImageListResponse, ApiError> {
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    let sort_col = match query.sort.as_str() {
        "created_at" | "file_size" | "original_name" => query.sort.as_str(),
        _ => "created_at",
    };
    let order_dir = match query.order.as_str() {
        "asc" | "ASC" => "ASC",
        _ => "DESC",
    };

    let search_term = query.search.trim();
    let total = count_user_images(pool, user_id, search_term, query.storage_config_id).await?;
    let rows = fetch_user_images(
        pool, user_id, sort_col, order_dir, search_term, limit, offset,
        query.storage_config_id,
    )
    .await?;
    let items: Vec<UploadResult> = rows.into_iter().map(UploadResult::from_row).collect();

    let total_pages = if total == 0 {
        1
    } else {
        ((total as f64) / (per_page as f64)).ceil() as u32
    };

    Ok(ImageListResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
    })
}

async fn count_user_images(
    pool: &PgPool,
    user_id: Uuid,
    search_term: &str,
    config_id: Option<Uuid>,
) -> Result<i64, ApiError> {
    let log_err = |e: sqlx::Error| {
        tracing::warn!("Image count query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    };
    if let Some(cid) = config_id {
        if search_term.is_empty() {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM images WHERE user_id = $1 AND storage_config_id = $2",
            )
            .bind(user_id)
            .bind(cid)
            .fetch_one(pool)
            .await
            .map_err(log_err)
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM images WHERE user_id = $1 \
                 AND original_name ILIKE $2 AND storage_config_id = $3",
            )
            .bind(user_id)
            .bind(format!("%{}%", search_term))
            .bind(cid)
            .fetch_one(pool)
            .await
            .map_err(log_err)
        }
    } else if search_term.is_empty() {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM images WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(log_err)
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM images WHERE user_id = $1 AND original_name ILIKE $2",
        )
        .bind(user_id)
        .bind(format!("%{}%", search_term))
        .fetch_one(pool)
        .await
        .map_err(log_err)
    }
}

#[allow(clippy::too_many_arguments)]
async fn fetch_user_images(
    pool: &PgPool, user_id: Uuid, sort_col: &str, order_dir: &str,
    search_term: &str, limit: i64, offset: i64,
    config_id: Option<Uuid>,
) -> Result<Vec<ImageRow>, ApiError> {
    let map_err = |e: sqlx::Error| {
        tracing::warn!("Image list query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    };
    let base = "SELECT id,public_key,original_name,url,mime_type,file_size,\
                sha256,width,height,status,thumbnail_url,webp_url,\
                created_at,storage_config_id FROM images";
    if let Some(cid) = config_id {
        if search_term.is_empty() {
            let sql = format!(
                "{base} WHERE user_id = $1 AND storage_config_id = $2 \
                 ORDER BY {sort_col} {order_dir} LIMIT $3 OFFSET $4"
            );
            sqlx::query_as::<_, ImageRow>(&sql)
                .bind(user_id).bind(cid).bind(limit).bind(offset)
                .fetch_all(pool).await.map_err(map_err)
        } else {
            let sql = format!(
                "{base} WHERE user_id = $1 AND original_name ILIKE $2 \
                 AND storage_config_id = $3 \
                 ORDER BY {sort_col} {order_dir} LIMIT $4 OFFSET $5"
            );
            sqlx::query_as::<_, ImageRow>(&sql)
                .bind(user_id).bind(format!("%{}%", search_term))
                .bind(cid).bind(limit).bind(offset)
                .fetch_all(pool).await.map_err(map_err)
        }
    } else if search_term.is_empty() {
        let sql = format!(
            "{base} WHERE user_id = $1 ORDER BY {sort_col} {order_dir} \
             LIMIT $2 OFFSET $3"
        );
        sqlx::query_as::<_, ImageRow>(&sql)
            .bind(user_id).bind(limit).bind(offset)
            .fetch_all(pool).await.map_err(map_err)
    } else {
        let sql = format!(
            "{base} WHERE user_id = $1 AND original_name ILIKE $2 \
             ORDER BY {sort_col} {order_dir} LIMIT $3 OFFSET $4"
        );
        sqlx::query_as::<_, ImageRow>(&sql)
            .bind(user_id).bind(format!("%{}%", search_term))
            .bind(limit).bind(offset)
            .fetch_all(pool).await.map_err(map_err)
    }
}

/// Fetch a single image by ID (owned by user).
pub async fn get_user_image(
    pool: &PgPool,
    user_id: Uuid,
    image_id: Uuid,
) -> Result<Option<UploadResult>, ApiError> {
    sqlx::query_as::<_, ImageRow>(
        "SELECT id, public_key, original_name, url, mime_type, file_size, \
         sha256, width, height, status, thumbnail_url, webp_url, \
         created_at, storage_config_id \
         FROM images WHERE id = $1 AND user_id = $2",
    )
    .bind(image_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Get image query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })
    .map(|opt| opt.map(UploadResult::from_row))
}
