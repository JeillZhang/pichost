use std::sync::Arc;

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    Extension,
};
use pichost_core::models::Category;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
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

fn error_json(msg: &str) -> Json<serde_json::Value> {
    Json(serde_json::json!({"error": msg}))
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

async fn validate_depth(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    parent_id: Uuid,
    current: i32,
) -> Result<(), RouteError> {
    if current >= MAX_DEPTH {
        return Err((
            StatusCode::BAD_REQUEST,
            error_json(&format!("Maximum category depth is {}", MAX_DEPTH)),
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
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, error_json(&e.to_string())))?;
    let parent =
        parent.ok_or_else(|| (StatusCode::NOT_FOUND, error_json("Parent category not found")))?;
    if let Some(gp) = parent.parent_id {
        Box::pin(validate_depth(pool, user_id, gp, current + 1)).await?;
    }
    Ok(())
}

// ── Handlers ────────────────────────────────────────────────────────────

/// GET /api/v1/categories
pub async fn list_categories(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<CategoryTreeNode>>, RouteError> {
    let rows: Vec<Category> = sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, error_json(&e.to_string())))?;
    Ok(Json(build_tree(rows)))
}

/// POST /api/v1/categories
pub async fn create_category(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<Category>), RouteError> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            error_json("Name must be 1-128 characters"),
        ));
    }
    if let Some(pid) = req.parent_id {
        validate_depth(&state.pool, user.id, pid, 1).await?;
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
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("categories_user_id_name_parent_id_key") {
                return (StatusCode::CONFLICT, error_json("Category name already exists"));
            }
        }
        (StatusCode::INTERNAL_SERVER_ERROR, error_json(&e.to_string()))
    })?;
    Ok((StatusCode::CREATED, Json(category)))
}

/// GET /api/v1/categories/{id}
pub async fn get_category(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Category>, RouteError> {
    sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, error_json(&e.to_string())))?
    .map(Json)
    .ok_or_else(|| (StatusCode::NOT_FOUND, error_json("Category not found")))
}

/// PATCH /api/v1/categories/{id}
pub async fn update_category(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCategoryRequest>,
) -> Result<Json<Category>, RouteError> {
    let existing = sqlx::query_as::<_, Category>(
        "SELECT id, user_id, name, parent_id, created_at \
         FROM categories WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, error_json(&e.to_string())))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, error_json("Category not found")))?;
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
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, error_json(&e.to_string())))?;
    Ok(Json(category))
}

/// DELETE /api/v1/categories/{id}
pub async fn delete_category(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, RouteError> {
    let result = sqlx::query("DELETE FROM categories WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, error_json(&e.to_string())))?;
    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, error_json("Category not found")));
    }
    Ok(StatusCode::OK)
}
