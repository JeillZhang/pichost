use crate::db::DbQueryResult;
use pichost_core::DbType;
use sqlx::Pool;
use std::sync::Arc;

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    Extension,
};
use pichost_core::i18n::Language;
use pichost_core::models::Category;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::i18n_ext::{error_json, error_json_args, JsonBody, Locale};
use crate::middleware::auth::AuthUser;

type RouteError = (StatusCode, Json<serde_json::Value>);
const MAX_DEPTH: i32 = 2;

// ── Request types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCategoryRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CategoryTreeNode {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub children: Vec<CategoryTreeNode>,
}

// ── Tree building helpers ───────────────────────────────────────────────

fn build_tree(categories: Vec<Category>) -> Vec<CategoryTreeNode> {
    let mut roots = Vec::new();
    for cat in &categories {
        if cat.parent_id.is_none() {
            roots.push(CategoryTreeNode {
                id: cat.id,
                name: cat.name.clone(),
                parent_id: None,
                children: build_children(cat.id, &categories),
            });
        }
    }
    roots
}

fn build_children(parent_id: Uuid, all: &[Category]) -> Vec<CategoryTreeNode> {
    all.iter()
        .filter(|c| c.parent_id == Some(parent_id))
        .map(|c| CategoryTreeNode {
            id: c.id,
            name: c.name.clone(),
            parent_id: Some(parent_id),
            children: Vec::new(),
        })
        .collect()
}

// ── Depth validation ────────────────────────────────────────────────────

async fn validate_depth<DB: DbType>(
    pool: &Pool<DB>,
    user_id: Uuid,
    parent_id: Uuid,
    current: i32,
    locale: Language,
) -> Result<(), RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    if current >= MAX_DEPTH {
        return Err(error_json_args(
            locale,
            StatusCode::BAD_REQUEST,
            "category.depth_exceeded",
            &[MAX_DEPTH.to_string()],
        ));
    }
    let parent: Option<Category> = sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE id = $1 AND user_id = $2",
    )
    .bind(parent_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error_json_args(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error_detail",
            &[e.to_string()],
        )
    })?;
    let parent = parent
        .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "category.parent_not_found"))?;
    if let Some(gp) = parent.parent_id {
        Box::pin(validate_depth(pool, user_id, gp, current + 1, locale)).await?;
    }
    Ok(())
}

// ── Handlers ────────────────────────────────────────────────────────────

/// GET /api/v1/categories
pub async fn list_categories<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
) -> Result<Json<Vec<CategoryTreeNode>>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let rows: Vec<Category> = sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        error_json_args(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error_detail",
            &[e.to_string()],
        )
    })?;
    Ok(Json(build_tree(rows)))
}

/// POST /api/v1/categories
pub async fn create_category<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
    JsonBody(req): JsonBody<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<Category>), RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 128 {
        return Err(error_json(
            locale,
            StatusCode::BAD_REQUEST,
            "category.invalid_name",
        ));
    }
    if let Some(pid) = req.parent_id {
        validate_depth(&state.pool, user.id, pid, 1, locale).await?;
    }
    let category = sqlx::query_as::<_, Category>(
        "INSERT INTO categories (user_id, name, parent_id) \
         VALUES ($1, $2, $3) \
         RETURNING id, user_id, name, parent_id, created_at",
    )
    .bind(user.id)
    .bind(&name)
    .bind(req.parent_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if pichost_core::db::db_error_kind(&e) == pichost_core::db::DbErrorKind::UniqueViolation {
            return error_json(locale, StatusCode::CONFLICT, "category.name_exists");
        }
        error_json_args(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error_detail",
            &[e.to_string()],
        )
    })?;
    Ok((StatusCode::CREATED, Json(category)))
}

/// GET /api/v1/categories/{id}
pub async fn get_category<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
    Path(id): Path<Uuid>,
) -> Result<Json<Category>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        error_json_args(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error_detail",
            &[e.to_string()],
        )
    })?
    .map(Json)
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "category.not_found"))
}

/// PATCH /api/v1/categories/{id}
pub async fn update_category<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
    Path(id): Path<Uuid>,
    JsonBody(req): JsonBody<UpdateCategoryRequest>,
) -> Result<Json<Category>, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let existing = sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        error_json_args(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error_detail",
            &[e.to_string()],
        )
    })?
    .ok_or_else(|| error_json(locale, StatusCode::NOT_FOUND, "category.not_found"))?;
    let new_name = req.name.unwrap_or(existing.name);
    let category = sqlx::query_as::<_, Category>(
        "UPDATE categories SET name = $1 WHERE id = $2 AND user_id = $3 \
         RETURNING id, user_id, name, parent_id, created_at",
    )
    .bind(&new_name)
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        error_json_args(
            locale,
            StatusCode::INTERNAL_SERVER_ERROR,
            "common.internal_error_detail",
            &[e.to_string()],
        )
    })?;
    Ok(Json(category))
}

/// DELETE /api/v1/categories/{id}
pub async fn delete_category<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    Extension(user): Extension<AuthUser>,
    Locale(locale): Locale,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, RouteError>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    usize: sqlx::ColumnIndex<DB::Row>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let result = sqlx::query("DELETE FROM categories WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            error_json_args(
                locale,
                StatusCode::INTERNAL_SERVER_ERROR,
                "common.internal_error_detail",
                &[e.to_string()],
            )
        })?;
    if result.affected() == 0 {
        return Err(error_json(
            locale,
            StatusCode::NOT_FOUND,
            "category.not_found",
        ));
    }
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(id: Uuid, parent: Option<Uuid>) -> Category {
        Category {
            id,
            user_id: Uuid::new_v4(),
            name: format!("cat-{}", id),
            parent_id: parent,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_build_tree_nested() {
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let categories = vec![cat(child, Some(root)), cat(root, None)];
        let tree = build_tree(categories);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].id, root);
        assert_eq!(tree[0].parent_id, None);
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].id, child);
        assert_eq!(tree[0].children[0].parent_id, Some(root));
        assert!(tree[0].children[0].children.is_empty());
    }

    #[test]
    fn test_build_tree_no_roots() {
        let a = Uuid::new_v4();
        let categories = vec![cat(a, Some(Uuid::new_v4()))];
        assert!(build_tree(categories).is_empty());
    }

    #[test]
    fn test_build_children_filters_by_parent() {
        let parent = Uuid::new_v4();
        let child = Uuid::new_v4();
        let other = Uuid::new_v4();
        let categories = vec![
            cat(parent, None),
            cat(child, Some(parent)),
            cat(other, Some(Uuid::new_v4())),
        ];
        let children = build_children(parent, &categories);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child);
    }
}
