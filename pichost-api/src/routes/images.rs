use crate::db::DbQueryResult;
use pichost_core::DbType;
use std::sync::Arc;

use axum::{
    extract::{Extension, Multipart, Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use pichost_core::i18n::Language;
use pichost_core::StorageRouter;
use serde_json::json;
use uuid::Uuid;

use crate::app::AppState;
use crate::cache::{cached_meta, cached_thumb};
use crate::i18n_ext::{error_json, error_json_args, JsonBody, Locale};
use crate::middleware::auth::AuthUser;
use crate::services::upload::{self, ImageListQuery, ImageListResponse, ImageRow, UploadResult};
use crate::services::upload_url;
use sqlx::{Pool, Row};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn check_image_status(status: &str) -> bool {
    status == "active" || status == "ready"
}

fn validate_batch_ids(
    ids: &[Uuid],
    locale: Language,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if ids.is_empty() {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "image.batch_empty",
        ));
    }
    if ids.len() > 100 {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "image.batch_limit",
        ));
    }
    Ok(())
}

async fn cleanup_storage_files(
    router: &StorageRouter,
    backend: &str,
    storage_key: &str,
    thumb_key: &Option<String>,
    webp_key: &Option<String>,
) {
    let storage = router.for_backend(backend);
    let _ = storage.delete(storage_key).await;
    if let Some(ref tk) = thumb_key {
        let _ = storage.delete(tk).await;
    }
    if let Some(ref wk) = webp_key {
        let _ = storage.delete(wk).await;
    }
}

type RouteError = (StatusCode, Json<serde_json::Value>);

/// Request body for PATCH /api/v1/images/:id
#[derive(Debug, Deserialize)]
pub struct UpdateImageRequest {
    pub original_name: String,
}

/// Validate original_name: non-empty, ≤255 chars, no path separators or null bytes.
fn validate_original_name(name: &str, locale: Language) -> Result<(), RouteError> {
    if name.is_empty() {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "image.rename_empty",
        ));
    }
    if name.len() > 255 {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "image.rename_too_long",
        ));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "image.rename_invalid",
        ));
    }
    Ok(())
}

async fn count_user_images<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    search_term: &str,
    config_id: Option<Uuid>,
    category_id: Option<Uuid>,
    locale: Language,
) -> Result<i64, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64,): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let sql = build_count_sql(search_term, config_id, category_id);
    let mut q = sqlx::query(&sql).bind(user_id);
    if !search_term.is_empty() {
        q = q.bind(format!("%{search_term}%"));
    }
    if config_id.is_some() {
        q = q.bind(config_id);
    }
    if category_id.is_some() {
        q = q.bind(category_id);
    }
    let row = q.fetch_one(pool).await.map_err(|e| {
        tracing::warn!("Image count query failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?;
    row.try_get(0usize).map_err(|e| {
        tracing::warn!("Image count decode failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })
}

#[allow(clippy::too_many_arguments)]
async fn fetch_user_images<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    sort_col: &str,
    order_dir: &str,
    search_term: &str,
    limit: i64,
    offset: i64,
    config_id: Option<Uuid>,
    category_id: Option<Uuid>,
    locale: Language,
) -> Result<Vec<ImageRow>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let sql = build_list_sql(search_term, config_id, category_id, sort_col, order_dir);
    let mut q = sqlx::query_as::<_, ImageRow>(&sql).bind(user_id);
    if !search_term.is_empty() {
        q = q.bind(format!("%{search_term}%"));
    }
    if config_id.is_some() {
        q = q.bind(config_id);
    }
    if category_id.is_some() {
        q = q.bind(category_id);
    }
    q = q.bind(limit).bind(offset);
    q.fetch_all(pool).await.map_err(|e| {
        tracing::warn!("Image list query failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })
}

fn build_count_sql(
    search_term: &str,
    config_id: Option<Uuid>,
    category_id: Option<Uuid>,
) -> String {
    let mut n = 1;
    let mut sql = format!("SELECT COUNT(*) FROM images WHERE user_id = ${n}");
    if !search_term.is_empty() {
        n += 1;
        sql.push_str(&format!(" AND LOWER(original_name) LIKE LOWER(${n})"));
    }
    if config_id.is_some() {
        n += 1;
        sql.push_str(&format!(" AND storage_config_id = ${n}"));
    }
    if category_id.is_some() {
        n += 1;
        sql.push_str(&format!(" AND category_id = ${n}"));
    }
    sql
}

/// Gallery list SQL with positional placeholders in bind order: user_id ($1),
/// optional search/config/category filters ($2..$4), then LIMIT/OFFSET.
#[allow(clippy::too_many_arguments)]
fn build_list_sql(
    search_term: &str,
    config_id: Option<Uuid>,
    category_id: Option<Uuid>,
    sort_col: &str,
    order_dir: &str,
) -> String {
    let mut n = 1;
    let mut sql = format!(
        "SELECT i.id,i.public_key,i.original_name,i.url,i.mime_type,i.file_size,\
         i.sha256,i.width,i.height,i.status,i.thumbnail_url,i.webp_url,\
         i.created_at,i.category_id,i.storage_config_id,\
         c.name,c.provider FROM images i \
         LEFT JOIN user_storage_configs c ON i.storage_config_id = c.id WHERE i.user_id = ${n}"
    );
    if !search_term.is_empty() {
        n += 1;
        sql.push_str(&format!(" AND LOWER(i.original_name) LIKE LOWER(${n})"));
    }
    if config_id.is_some() {
        n += 1;
        sql.push_str(&format!(" AND i.storage_config_id = ${n}"));
    }
    if category_id.is_some() {
        n += 1;
        sql.push_str(&format!(" AND i.category_id = ${n}"));
    }
    n += 1;
    let limit = n;
    n += 1;
    let offset = n;
    sql.push_str(&format!(
        " ORDER BY {sort_col} {order_dir} LIMIT ${limit} OFFSET ${offset}"
    ));
    sql
}

fn map_rows_to_results(rows: Vec<ImageRow>) -> Vec<UploadResult> {
    rows.into_iter().map(UploadResult::from_row).collect()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/images — upload an image (protected)
pub async fn upload_handler<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Vec<UploadResult>>), (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64,): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (bool,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i32>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> &'a [uuid::Uuid]: sqlx::Encode<'q, DB>,
    [uuid::Uuid]: sqlx::Type<DB>,
{
    let (bytes, file_name, storage_config_ids) =
        extract_upload_parts(&mut multipart, locale.0).await?;

    match upload::process_upload(
        &state,
        &user,
        bytes,
        file_name,
        storage_config_ids,
        locale.0,
    )
    .await
    {
        Ok(results) => {
            crate::metrics::UPLOADS_TOTAL.inc();
            Ok((StatusCode::CREATED, Json(results)))
        }
        Err(e) => {
            crate::metrics::UPLOAD_ERRORS_TOTAL.inc();
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// URL upload
// ---------------------------------------------------------------------------

/// Request body for URL-based image upload.
#[derive(Debug, serde::Deserialize)]
pub struct UrlUploadRequest {
    pub url: String,
    #[serde(default)]
    pub storage_config_ids: Option<Vec<Uuid>>,
}

fn validate_url_not_empty(url: &str, locale: Language) -> Result<(), RouteError> {
    if url.trim().is_empty() {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "url.field_required",
        ));
    }
    Ok(())
}

/// POST /api/v1/images/upload-url — upload from a remote URL (protected)
pub async fn url_upload_handler<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    JsonBody(payload): JsonBody<UrlUploadRequest>,
) -> Result<(StatusCode, Json<Vec<UploadResult>>), RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64,): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (bool,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i32>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> &'a [uuid::Uuid]: sqlx::Encode<'q, DB>,
    [uuid::Uuid]: sqlx::Type<DB>,
{
    validate_url_not_empty(&payload.url, locale.0)?;

    let (bytes, file_name) = upload_url::fetch_image_from_url(&payload.url, locale.0).await?;

    match upload::process_upload(
        &state,
        &user,
        bytes,
        file_name,
        payload.storage_config_ids,
        locale.0,
    )
    .await
    {
        Ok(results) => {
            crate::metrics::UPLOADS_TOTAL.inc();
            Ok((StatusCode::CREATED, Json(results)))
        }
        Err(e) => {
            crate::metrics::UPLOAD_ERRORS_TOTAL.inc();
            Err(e)
        }
    }
}

async fn parse_upload_field(
    field: axum::extract::multipart::Field<'_>,
    file_data: &mut Option<Vec<u8>>,
    file_name: &mut String,
    storage_config_ids: &mut Option<Vec<Uuid>>,
    locale: Language,
) -> Result<(), RouteError> {
    match field.name() {
        Some("file") => {
            *file_name = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "file".to_string());
            let data = field.bytes().await.map_err(|e| {
                error_json_args(
                    locale,
                    StatusCode::BAD_REQUEST,
                    "upload.field_read_failed",
                    &[e.to_string()],
                )
            })?;
            *file_data = Some(data.to_vec());
        }
        Some("storage_config_ids") => {
            let text = field.text().await.map_err(|e| {
                error_json_args(
                    locale,
                    StatusCode::BAD_REQUEST,
                    "upload.config_ids_read_failed",
                    &[e.to_string()],
                )
            })?;
            let ids: Result<Vec<Uuid>, _> =
                text.split(',').map(|s| Uuid::parse_str(s.trim())).collect();
            *storage_config_ids = Some(ids.map_err(|_| {
                error_json(
                    locale,
                    StatusCode::BAD_REQUEST,
                    "upload.invalid_config_uuid",
                )
            })?);
        }
        _ => {}
    }
    Ok(())
}

/// Extract file data and optional storage_config_ids from a multipart upload.
async fn extract_upload_parts(
    multipart: &mut Multipart,
    locale: Language,
) -> Result<(Vec<u8>, String, Option<Vec<Uuid>>), RouteError> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name = "file".to_string();
    let mut storage_config_ids: Option<Vec<Uuid>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        parse_upload_field(
            field,
            &mut file_data,
            &mut file_name,
            &mut storage_config_ids,
            locale,
        )
        .await?;
    }

    let bytes = file_data
        .ok_or_else(|| error_json(locale, StatusCode::BAD_REQUEST, "upload.file_missing"))?;

    Ok((bytes, file_name, storage_config_ids))
}

fn resolve_sort(params: &ImageListQuery) -> (&str, &str) {
    let sort_col = match params.sort.as_str() {
        "created_at" | "file_size" | "original_name" => params.sort.as_str(),
        _ => "created_at",
    };
    let order_dir = match params.order.as_str() {
        "asc" | "ASC" => "ASC",
        _ => "DESC",
    };
    (sort_col, order_dir)
}

/// GET /api/v1/images — list user's images with pagination, search, and sort (protected)
pub async fn list_images<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    axum::extract::Query(params): axum::extract::Query<ImageListQuery>,
) -> Result<Json<ImageListResponse>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (i64,): crate::db::DbRow<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let page = params.page.max(1);
    let per_page = params.per_page.clamp(1, 100);
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;
    let (sort_col, order_dir) = resolve_sort(&params);

    let search_term = params.search.trim();
    let total = count_user_images(
        &state.pool,
        user.id,
        search_term,
        params.storage_config_id,
        params.category_id,
        locale.0,
    )
    .await?;
    let rows = fetch_user_images(
        &state.pool,
        user.id,
        sort_col,
        order_dir,
        search_term,
        limit,
        offset,
        params.storage_config_id,
        params.category_id,
        locale.0,
    )
    .await?;
    let items = map_rows_to_results(rows);

    let total_pages = if total == 0 {
        1
    } else {
        ((total as f64) / (per_page as f64)).ceil() as u32
    };

    Ok(Json(ImageListResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
    }))
}

/// GET /api/v1/images/{id} — single image detail (protected, cached)
pub async fn get_image<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    Path(id): Path<Uuid>,
) -> Result<Json<UploadResult>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let result = cached_meta(state.cache.as_ref(), &id, 600, async {
        sqlx::query_as::<_, ImageRow>(
            "SELECT i.id, i.public_key, i.original_name, i.url, i.mime_type, i.file_size,\
                 i.sha256, i.width, i.height, i.status, i.thumbnail_url, i.webp_url, \
                 i.created_at, i.category_id, i.storage_config_id, \
                 c.name, c.provider \
                 FROM images i \
                 LEFT JOIN user_storage_configs c ON i.storage_config_id = c.id \
                 WHERE i.id = $1 AND i.user_id = $2",
        )
        .bind(id)
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Get image query failed: {e}");
            error_json(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "image.internal_error",
            )
        })?
        .ok_or_else(|| error_json(locale.0, StatusCode::NOT_FOUND, "image.not_found"))
        .map(UploadResult::from_row)
    })
    .await?;

    Ok(Json(result))
}

async fn rename_image_in_db<DB: DbType>(
    pool: &Pool<DB>,
    id: Uuid,
    user_id: Uuid,
    name: &str,
    locale: Language,
) -> Result<(), RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let updated_rows =
        sqlx::query("UPDATE images SET original_name = $1 WHERE id = $2 AND user_id = $3")
            .bind(name)
            .bind(id)
            .bind(user_id)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::warn!("Rename image query failed: {e}");
                error_json(
                    locale,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "common.internal_error",
                )
            })?
            .affected();
    if updated_rows == 0 {
        return Err(error_json(locale, StatusCode::NOT_FOUND, "image.not_found"));
    }
    Ok(())
}

async fn refetch_image_row<DB: DbType>(
    pool: &Pool<DB>,
    id: Uuid,
    user_id: Uuid,
    locale: Language,
) -> Result<ImageRow, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_as::<_, ImageRow>(
        "SELECT i.id, i.public_key, i.original_name, i.url, i.mime_type, i.file_size,\
         i.sha256, i.width, i.height, i.status, i.thumbnail_url, i.webp_url, \
         i.created_at, i.category_id, i.storage_config_id, \
         c.name, c.provider \
         FROM images i \
         LEFT JOIN user_storage_configs c ON i.storage_config_id = c.id \
         WHERE i.id = $1 AND i.user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Rename image re-fetch failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "image.not_found"))
}

/// PATCH /api/v1/images/{id} — rename an image's display name
pub async fn rename_image<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    locale: Locale,
    JsonBody(req): JsonBody<UpdateImageRequest>,
) -> Result<Json<UploadResult>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    validate_original_name(&req.original_name, locale.0)?;
    rename_image_in_db(&state.pool, id, user.id, &req.original_name, locale.0).await?;
    let updated = refetch_image_row(&state.pool, id, user.id, locale.0).await?;

    let _: Result<(), _> = state.cache.del(&format!("pichost:meta:{}", id)).await;

    Ok(Json(UploadResult::from_row(updated)))
}

/// GET /u/{public_key} — serve image publicly (unauthenticated)
pub async fn public_get<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Path(public_key): Path<String>,
) -> Result<Response, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (
        std::string::String,
        std::string::String,
        std::string::String,
        std::string::String,
    ): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT storage_key, mime_type, status, storage_backend FROM images WHERE public_key = $1",
    )
    .bind(&public_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Public image query failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?
    .ok_or_else(|| error_json(locale.0, StatusCode::NOT_FOUND, "image.not_found"))?;

    let (storage_key, mime_type, status, storage_backend) = row;
    if !check_image_status(&status) {
        return Err(error_json(
            locale.0,
            StatusCode::NOT_FOUND,
            "image.not_found",
        ));
    }

    let storage = state.router.for_backend(&storage_backend);
    let bytes = storage.get(&storage_key).await.map_err(|e| {
        tracing::warn!("Storage read failed on {}: {e}", storage.backend_name());
        error_json(locale.0, StatusCode::NOT_FOUND, "image.not_found")
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &mime_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// Detect MIME type of a generated thumbnail from its bytes — the storage
/// key carries no extension (e.g. `{user}/thumb.{image_id}`), so key-based
/// guessing returns image/jpeg even for PNG thumbs, which browsers reject
/// under `X-Content-Type-Options: nosniff`.
fn mime_for_thumb_bytes(bytes: &[u8]) -> &'static str {
    infer::get(bytes)
        .map(|t| t.mime_type())
        .unwrap_or("application/octet-stream")
}

/// GET /u/thumb/{image_id} — serve generated thumbnail (unauthenticated)
pub async fn public_get_thumb<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Path(image_id): Path<Uuid>,
) -> Result<Response, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let row = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT thumbnail_key, storage_backend FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Thumb query failed: {e}");
        error_json(locale.0, StatusCode::INTERNAL_SERVER_ERROR, "image.internal_error")
    })?
    .ok_or_else(|| error_json(locale.0, StatusCode::NOT_FOUND, "image.not_found"))?;

    let (thumb_key, storage_backend) = row;
    let thumb_key = thumb_key
        .ok_or_else(|| error_json(locale.0, StatusCode::NOT_FOUND, "image.thumbnail_not_ready"))?;

    let bytes = cached_thumb(
        state.cache.as_ref(),
        &format!("thumb:{}", image_id),
        3600,
        async {
            let backend = state.router.for_backend(&storage_backend);
            backend.get(&thumb_key).await.map_err(|e| {
                tracing::warn!("Thumb storage read failed: {e}");
                error_json(locale.0, StatusCode::NOT_FOUND, "image.thumbnail_not_found")
            })
        },
    )
    .await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for_thumb_bytes(&bytes))
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

async fn resolve_thumb_key_by_public_key<DB: DbType>(
    pool: &Pool<DB>,
    public_key: &str,
    locale: Language,
) -> Result<(String, String), RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
{
    let row: (Option<String>, String) = sqlx::query_as(
        "SELECT thumbnail_key, storage_backend FROM images \
         WHERE public_key = $1 AND status IN ('active', 'ready')",
    )
    .bind(public_key)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Thumbnail-by-key query failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "image.not_found"))?;

    let (thumb_key, storage_backend) = row;
    let thumb_key = thumb_key
        .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "image.thumbnail_not_ready"))?;
    Ok((thumb_key, storage_backend))
}

/// GET /t/{public_key} — serve thumbnail by public_key (alias)
pub async fn public_get_thumb_by_key<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Path(public_key): Path<String>,
) -> Result<Response, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
{
    let (thumb_key, storage_backend) =
        resolve_thumb_key_by_public_key(&state.pool, &public_key, locale.0).await?;

    let backend = state.router.for_backend(&storage_backend);
    let bytes = cached_thumb(
        state.cache.as_ref(),
        &format!("thumb:pk:{}", public_key),
        3600,
        async {
            backend.get(&thumb_key).await.map_err(|e| {
                tracing::warn!("Thumb storage read by key failed: {e}");
                error_json(
                    locale.0,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "image.thumbnail_not_found",
                )
            })
        },
    )
    .await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for_thumb_bytes(&bytes))
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// GET /u/webp/{image_id} — serve generated WebP (unauthenticated)
pub async fn public_get_webp<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    locale: Locale,
    Path(image_id): Path<Uuid>,
) -> Result<Response, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let row = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT webp_key, storage_backend FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("WebP query failed: {e}");
        error_json(locale.0, StatusCode::INTERNAL_SERVER_ERROR, "image.internal_error")
    })?
    .ok_or_else(|| error_json(locale.0, StatusCode::NOT_FOUND, "image.not_found"))?;

    let (webp_key, storage_backend) = row;
    let webp_key = webp_key
        .ok_or_else(|| error_json(locale.0, StatusCode::NOT_FOUND, "image.webp_not_ready"))?;

    let bytes = cached_thumb(
        state.cache.as_ref(),
        &format!("webp:{}", image_id),
        3600,
        async {
            let backend = state.router.for_backend(&storage_backend);
            backend.get(&webp_key).await.map_err(|e| {
                tracing::warn!("WebP storage read failed: {e}");
                error_json(locale.0, StatusCode::NOT_FOUND, "image.webp_not_found")
            })
        },
    )
    .await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/webp")
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

async fn fetch_delete_target<DB: DbType>(
    pool: &Pool<DB>,
    id: Uuid,
    user: &AuthUser,
    locale: Language,
) -> Result<(String, String, Option<String>, Option<String>), RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        r#"SELECT storage_key, storage_backend, thumbnail_key, webp_key
           FROM images WHERE id = $1 AND (user_id = $2 OR $3)"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(user.is_admin)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Delete image query failed: {e}");
        error_json(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "image.not_found"))
}

/// DELETE /api/v1/images/{id} — delete image + storage files (protected)
pub async fn delete_image<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let (storage_key, storage_backend, thumb_key, webp_key) =
        fetch_delete_target(&state.pool, id, &user, locale.0).await?;
    cleanup_storage_files(
        &state.router,
        &storage_backend,
        &storage_key,
        &thumb_key,
        &webp_key,
    )
    .await;

    sqlx::query("DELETE FROM images WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Image delete db failed: {e}");
            error_json(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "image.delete_failed",
            )
        })?;

    let _: Result<(), _> = state.cache.del(&format!("pichost:meta:{}", id)).await;

    tracing::info!(image_id = %id, user_id = %user.id, "image deleted");
    Ok(Json(json!({"message": "image deleted", "id": id})))
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BatchDeleteRequest {
    pub ids: Vec<Uuid>,
}

async fn fetch_batch_delete_targets<DB: DbType>(
    pool: &Pool<DB>,
    ids: &[Uuid],
    user: &AuthUser,
    locale: Language,
) -> Result<Vec<(String, String, Option<String>, Option<String>)>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${i}")).collect();
    let user_param = ids.len() + 1;
    let admin_param = ids.len() + 2;
    let sql = format!(
        "SELECT storage_key, storage_backend, thumbnail_key, webp_key FROM images \
         WHERE id IN ({}) AND (user_id = ${user_param} OR ${admin_param})",
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(&sql);
    for id in ids {
        q = q.bind(id);
    }
    q.bind(user.id)
        .bind(user.is_admin)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::warn!("Batch delete query failed: {e}");
            error_json(
                locale,
                StatusCode::INTERNAL_SERVER_ERROR,
                "common.internal_error",
            )
        })
}

/// POST /api/v1/images/batch-delete — delete multiple images (protected)
pub async fn batch_delete<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    JsonBody(body): JsonBody<BatchDeleteRequest>,
) -> Result<Json<serde_json::Value>, RouteError>
where
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
{
    validate_batch_ids(&body.ids, locale.0)?;

    let rows = fetch_batch_delete_targets(&state.pool, &body.ids, &user, locale.0).await?;

    for (sk, sb, tk, wk) in &rows {
        cleanup_storage_files(&state.router, sb, sk, tk, wk).await;
    }

    let placeholders: Vec<String> = (1..=body.ids.len()).map(|i| format!("${i}")).collect();
    let sql = format!(
        "DELETE FROM images WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut q = sqlx::query(&sql);
    for image_id in &body.ids {
        q = q.bind(image_id);
    }
    let deleted = q
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Batch delete DB failed: {e}");
            error_json(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "image.batch_delete_failed",
            )
        })?
        .affected() as usize;

    for image_id in &body.ids {
        let _: Result<(), _> = state.cache.del(&format!("pichost:meta:{}", image_id)).await;
    }

    let failed = body.ids.len().saturating_sub(deleted);
    tracing::info!(user_id = %user.id, requested = body.ids.len(), deleted, failed, "batch delete");
    Ok(Json(
        json!({"message": "batch delete completed", "deleted": deleted, "failed": failed}),
    ))
}

#[derive(Debug, Deserialize)]
pub struct MoveImageRequest {
    /// None removes the image from its category.
    pub category_id: Option<Uuid>,
}

async fn ensure_category_owned<DB: DbType>(
    pool: &Pool<DB>,
    category_id: Uuid,
    user_id: Uuid,
    locale: Language,
) -> Result<(), RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    use pichost_core::models::Category;

    sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE id = $1 AND user_id = $2",
    )
    .bind(category_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error_json_args(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.db_error",
            &[e.to_string()],
        )
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "category.not_found"))?;
    Ok(())
}

/// POST /api/v1/images/{id}/move — move an image to a category
pub async fn move_image<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    locale: Locale,
    JsonBody(body): JsonBody<MoveImageRequest>,
) -> Result<Json<serde_json::Value>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    if let Some(category_id) = body.category_id {
        ensure_category_owned(&state.pool, category_id, user.id, locale.0).await?;
    }

    let result = sqlx::query("UPDATE images SET category_id = $1 WHERE id = $2 AND user_id = $3")
        .bind(body.category_id)
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            error_json_args(
                locale.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "common.db_error",
                &[e.to_string()],
            )
        })?;

    if result.affected() == 0 {
        return Err(error_json(
            locale.0,
            StatusCode::NOT_FOUND,
            "image.move_not_found",
        ));
    }

    let _: Result<(), _> = state.cache.del(&format!("pichost:meta:{}", id)).await;

    Ok(Json(json!({"message": "Image moved to category"})))
}

#[derive(Debug, Deserialize)]
pub struct BatchMoveRequest {
    pub image_ids: Vec<Uuid>,
    pub category_id: Uuid,
}

fn validate_batch_move(ids: &[Uuid], locale: Language) -> Result<(), RouteError> {
    if ids.is_empty() {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "image.batch_move_empty",
        ));
    }
    if ids.len() > 100 {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "image.batch_move_limit",
        ));
    }
    Ok(())
}

/// POST /api/v1/images/batch-move — move multiple images to a category
pub async fn batch_move_images<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    JsonBody(body): JsonBody<BatchMoveRequest>,
) -> Result<Json<serde_json::Value>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    validate_batch_move(&body.image_ids, locale.0)?;
    ensure_category_owned(&state.pool, body.category_id, user.id, locale.0).await?;

    let placeholders: Vec<String> = (3..3 + body.image_ids.len())
        .map(|i| format!("${i}"))
        .collect();
    let sql = format!(
        "UPDATE images SET category_id = $1 WHERE user_id = $2 AND id IN ({})",
        placeholders.join(", ")
    );
    let mut q = sqlx::query(&sql).bind(body.category_id).bind(user.id);
    for image_id in &body.image_ids {
        q = q.bind(image_id);
    }
    let result = q.execute(&state.pool).await.map_err(|e| {
        error_json_args(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.db_error",
            &[e.to_string()],
        )
    })?;

    for image_id in &body.image_ids {
        let _: Result<(), _> = state.cache.del(&format!("pichost:meta:{}", image_id)).await;
    }

    Ok(Json(json!({
        "message": "Images moved to category",
        "moved": result.affected()
    })))
}

#[derive(serde::Serialize)]
pub struct ImageLinks {
    pub url: String,
    pub markdown: String,
    pub html: String,
    pub bbcode: String,
}

/// GET /api/v1/images/{id}/links — get share link formats only
pub async fn get_image_links<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    locale: Locale,
    Path(image_id): Path<Uuid>,
) -> Result<Json<ImageLinks>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (
        std::string::String,
        std::string::String,
        std::string::String,
    ): crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    use crate::services::html_escape;

    let row: (String, String, String) = sqlx::query_as(
        "SELECT public_key, original_name, url FROM images WHERE id = $1 AND user_id = $2",
    )
    .bind(image_id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Image links query failed: {e}");
        error_json(
            locale.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error",
        )
    })?
    .ok_or_else(|| error_json(locale.0, StatusCode::NOT_FOUND, "image.not_found"))?;

    let (_public_key, original_name, url) = row;
    let markdown = format!("![{}]({})", original_name, url);
    let html = format!(
        "<img src=\"{}\" alt=\"{}\" />",
        url,
        html_escape(&original_name)
    );
    let bbcode = format!("[img]{}[/img]", url);

    Ok(Json(ImageLinks {
        url,
        markdown,
        html,
        bbcode,
    }))
}

#[cfg(test)]
mod rename_tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn validate_name_rejects_empty() {
        let result = validate_original_name("", Language::En);
        assert!(result.is_err());
        let (code, _) = result.unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_name_rejects_too_long() {
        let long_name = "a".repeat(256);
        let result = validate_original_name(&long_name, Language::En);
        assert!(result.is_err());
        let (code, _) = result.unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_name_rejects_path_separators() {
        for ch in &['/', '\\', '\0'] {
            let name = format!("bad{}name.txt", ch);
            let result = validate_original_name(&name, Language::En);
            assert!(result.is_err(), "should reject char '{}'", ch);
        }
    }

    #[test]
    fn validate_name_accepts_valid() {
        let result = validate_original_name("valid-file_name (1).jpg", Language::En);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_name_accepts_max_length() {
        let name = "a".repeat(255);
        let result = validate_original_name(&name, Language::En);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_count_sql_no_filters() {
        assert_eq!(
            build_count_sql("", None, None),
            "SELECT COUNT(*) FROM images WHERE user_id = $1"
        );
    }

    #[test]
    fn test_build_count_sql_with_filters() {
        let uid = Uuid::new_v4();
        let sql = build_count_sql("cat", Some(uid), Some(uid));
        assert!(sql.starts_with("SELECT COUNT(*) FROM images WHERE user_id = $1"));
        assert!(sql.contains(" AND LOWER(original_name) LIKE LOWER($2)"));
        assert!(sql.contains(" AND storage_config_id = $3"));
        assert!(sql.contains(" AND category_id = $4"));
        assert_eq!(sql.matches('$').count(), 4);
    }

    #[test]
    fn test_build_list_sql_no_filters() {
        let sql = build_list_sql("", None, None, "created_at", "DESC");
        assert!(sql.starts_with("SELECT i.id,i.public_key,"));
        assert!(sql.ends_with(" ORDER BY created_at DESC LIMIT $2 OFFSET $3"));
        assert!(!sql.contains("ILIKE"));
    }

    #[test]
    fn test_build_list_sql_with_filters() {
        let uid = Uuid::new_v4();
        let sql = build_list_sql("cat", Some(uid), Some(uid), "file_size", "ASC");
        assert!(sql.contains("WHERE i.user_id = $1"));
        assert!(sql.contains(" AND LOWER(i.original_name) LIKE LOWER($2)"));
        assert!(sql.contains(" AND i.storage_config_id = $3"));
        assert!(sql.contains(" AND i.category_id = $4"));
        assert!(sql.ends_with(" ORDER BY file_size ASC LIMIT $5 OFFSET $6"));
        assert_eq!(sql.matches('$').count(), 6);
    }

    fn sample_query(sort: &str, order: &str) -> ImageListQuery {
        ImageListQuery {
            page: 1,
            per_page: 20,
            sort: sort.to_string(),
            order: order.to_string(),
            search: String::new(),
            storage_config_id: None,
            category_id: None,
        }
    }

    #[test]
    fn test_check_image_status() {
        assert!(check_image_status("active"));
        assert!(check_image_status("ready"));
        assert!(!check_image_status("pending"));
        assert!(!check_image_status("failed"));
        assert!(!check_image_status("processing"));
    }

    #[test]
    fn test_validate_batch_ids() {
        let err = validate_batch_ids(&[], Language::En).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        let many: Vec<Uuid> = (0..101).map(|_| Uuid::new_v4()).collect();
        let err = validate_batch_ids(&many, Language::En).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        let ok: Vec<Uuid> = (0..100).map(|_| Uuid::new_v4()).collect();
        assert!(validate_batch_ids(&ok, Language::En).is_ok());
        assert!(validate_batch_ids(&[Uuid::new_v4()], Language::En).is_ok());
    }

    #[test]
    fn test_validate_batch_move() {
        let err = validate_batch_move(&[], Language::En).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        let many: Vec<Uuid> = (0..101).map(|_| Uuid::new_v4()).collect();
        assert!(validate_batch_move(&many, Language::En).is_err());
        let ok: Vec<Uuid> = (0..100).map(|_| Uuid::new_v4()).collect();
        assert!(validate_batch_move(&ok, Language::En).is_ok());
    }

    #[test]
    fn test_validate_url_not_empty() {
        assert!(validate_url_not_empty("", Language::En).is_err());
        assert!(validate_url_not_empty("   ", Language::En).is_err());
        assert!(validate_url_not_empty("http://x/y.png", Language::En).is_ok());
    }

    #[test]
    fn test_resolve_sort() {
        assert_eq!(
            resolve_sort(&sample_query("file_size", "asc")),
            ("file_size", "ASC")
        );
        assert_eq!(
            resolve_sort(&sample_query("original_name", "ASC")),
            ("original_name", "ASC")
        );
        assert_eq!(
            resolve_sort(&sample_query("created_at", "desc")),
            ("created_at", "DESC")
        );
        assert_eq!(
            resolve_sort(&sample_query("bogus", "bogus")),
            ("created_at", "DESC")
        );
    }

    #[test]
    fn test_mime_for_thumb_bytes() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let jpeg = [0xff, 0xd8, 0xff, 0xe0];
        assert_eq!(mime_for_thumb_bytes(&png), "image/png");
        assert_eq!(mime_for_thumb_bytes(&jpeg), "image/jpeg");
        assert_eq!(
            mime_for_thumb_bytes(b"not an image"),
            "application/octet-stream"
        );
        assert_eq!(mime_for_thumb_bytes(&[]), "application/octet-stream");
    }

    #[test]
    fn test_map_rows_to_results_empty() {
        assert!(map_rows_to_results(vec![]).is_empty());
    }
}
