# P4-C: Gallery Categories Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable users to organize images into a 2-level category hierarchy with gallery filtering, single/batch move, and a dual-column sidebar+grid UI.

**Architecture:** A `categories` table (self-referencing `parent_id`, max depth 2) and a `category_id` FK on `images`. Backend follows the `storage_configs.rs` CRUD pattern for user-owned resource handlers. Frontend refactors Gallery into a dual-column layout with a `CategoryTree` sidebar using TanStack Query for server state.

**Tech Stack:** Rust (Axum, sqlx, serde), React 19 (TypeScript 7, Tailwind CSS 4, TanStack Query v5, ky)

## Global Constraints

- Rust: functions ≤50 lines, lines ≤120 chars
- sqlx: runtime-only queries (no `query!` macro)
- Migrations: auto-apply at startup via `sqlx::migrate!()` in `pichost-api/src/main.rs`
- API: JWT auth on all category endpoints (user-owned resources)
- Frontend: React 19, TypeScript 7, Tailwind CSS 4, TanStack Query v5
- No `as any`, `@ts-ignore`, or `@ts-expect-error`
- Version bump: `0.15.1` → `0.16.0` (minor — new feature)
- Verification gates: `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `npm run build`

---

## Task Dependency Graph

```
T0 (migration)
  └─ T1 (model + category_id on Image)
      └─ T2 (CRUD API endpoints)
          ├─ T3 (image move + gallery filter) ── T4 (frontend API)
          │                                         ├─ T5 (CategoryTree)
          │                                         ├─ T6 (Gallery layout)
          │                                         │    └─ T7 (CRUD UI in sidebar)
          │                                         └─ T8 (ImageDetail assignment)
```

---

### T0: Database migration 0009 — categories table

- id: T0
  title: "Create categories table migration 0009"
  files:
    - migrations/0009_create_categories.sql
  depends_on: []
  breaking: true
  ac:
    - given: "fresh database with images table"
      when: "migration 0009 runs"
      then: "categories table exists with columns (id UUID PK, user_id FK→users, name VARCHAR(128), parent_id FK→categories, created_at TIMESTAMPTZ), unique constraint on (user_id, name, parent_id), and images table has category_id UUID column referencing categories(id) ON DELETE SET NULL"
  regression:
    - "cargo test --workspace"
  migration_verify:
    - "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'categories') → true"
    - "SELECT column_name FROM information_schema.columns WHERE table_name = 'images' AND column_name = 'category_id' → 1 row"
  test_code: |
    -- Migration files are verified by applying them and checking schema.
    -- No unit test needed; migration_verify steps above serve as the test.
  impl_code: |
    -- migrations/0009_create_categories.sql
    CREATE TABLE categories (
        id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name        VARCHAR(128) NOT NULL,
        parent_id   UUID REFERENCES categories(id) ON DELETE CASCADE,
        created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        UNIQUE(user_id, name, parent_id)
    );

    ALTER TABLE images
        ADD COLUMN category_id UUID REFERENCES categories(id) ON DELETE SET NULL;
  verify:
    - "cargo test --workspace"
    - "cargo clippy --workspace -- -D warnings"

---

### T1: Category model + category_id on Image struct

- id: T1
  title: "Add Category model and category_id field to Image"
  files:
    - pichost-core/src/models.rs
    - pichost-api/tests/category_test.rs
  depends_on: [T0]
  breaking: false
  ac:
    - given: "Category struct with serde derives"
      when: "serialized to JSON and deserialized back"
      then: "round-trip preserves name, parent_id=null, and all fields"
    - given: "Image struct with optional category_id"
      when: "deserialized from JSON containing category_id"
      then: "category_id is Some(uuid)"
    - given: "Image struct with optional category_id"
      when: "deserialized from JSON without category_id key"
      then: "category_id is None (serde default)"
  regression:
    - "cargo test -p pichost-core"
    - "cargo test -p pichost-api test_image_list"
  test_code: |
    // pichost-api/tests/category_test.rs
    use pichost_core::models::Category;
    use uuid::Uuid;

    #[test]
    fn test_category_serde_roundtrip() {
        let cat = Category {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            name: "Travel Photos".into(),
            parent_id: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&cat).unwrap();
        let parsed: Category = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Travel Photos");
        assert_eq!(parsed.parent_id, None);
    }

    #[test]
    fn test_image_category_id_optional() {
        let json = r#"{"id":"00000000-0000-0000-0000-000000000001","user_id":"00000000-0000-0000-0000-000000000000","public_key":"abc123","original_name":"test.png","storage_key":"k","storage_backend":"local","mime_type":"image/png","file_size":100,"width":null,"height":null,"sha256":"abc","url":"http://x","status":"active","storage_config_id":null,"created_at":"2026-01-01T00:00:00Z","category_id":"00000000-0000-0000-0000-000000000002"}"#;
        let img: pichost_core::models::Image = serde_json::from_str(json).unwrap();
        assert_eq!(img.category_id, Some(Uuid::nil()));
        let json_no_cat = r#"{"id":"00000000-0000-0000-0000-000000000001","user_id":"00000000-0000-0000-0000-000000000000","public_key":"abc123","original_name":"test.png","storage_key":"k","storage_backend":"local","mime_type":"image/png","file_size":100,"width":null,"height":null,"sha256":"abc","url":"http://x","status":"active","storage_config_id":null,"created_at":"2026-01-01T00:00:00Z"}"#;
        let img2: pichost_core::models::Image = serde_json::from_str(json_no_cat).unwrap();
        assert_eq!(img2.category_id, None);
    }
  impl_code: |
    // pichost-core/src/models.rs
    // Add after UserStorageConfig struct (around line 75):

    /// A user-created image category, supporting up to 2 levels of nesting.
    #[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
    pub struct Category {
        pub id: Uuid,
        pub user_id: Uuid,
        pub name: String,
        pub parent_id: Option<Uuid>,
        pub created_at: DateTime<Utc>,
    }

    // In the Image struct (around line 37, before `pub created_at`):
    /// Optional category assignment for organizing images.
    #[serde(default)]
    pub category_id: Option<Uuid>,
  verify:
    - "cargo test -p pichost-api test_category_serde_roundtrip -- --exact"
    - "cargo test -p pichost-api test_image_category_id_optional -- --exact"
    - "cargo clippy --workspace -- -D warnings"

---

### T2: Category CRUD API endpoints + route registration

- id: T2
  title: "Add category CRUD API handlers and register routes"
  files:
    - pichost-api/src/routes/categories.rs
    - pichost-api/src/main.rs
    - pichost-api/tests/category_test.rs
  depends_on: [T1]
  breaking: false
  ac:
    - given: "authenticated user with no categories"
      when: "POST /api/v1/categories with {name: 'Blog', parent_id: null}"
      then: "returns 201 with Category JSON, category persisted in DB"
    - given: "authenticated user with a category"
      when: "GET /api/v1/categories"
      then: "returns 200 with array of categories belonging to that user only"
    - given: "authenticated user with a category"
      when: "PATCH /api/v1/categories/:id with {name: 'Updated'}"
      then: "returns 200 with updated name, other user's categories unaffected"
    - given: "authenticated user with a category that has sub-categories"
      when: "DELETE /api/v1/categories/:id"
      then: "returns 200, category and sub-categories deleted, associated images have category_id set to NULL"
    - given: "authenticated user"
      when: "POST /api/v1/categories with depth > 2 (parent of a child)"
      then: "returns 400 with error message about max depth"
  regression:
    - "cargo test -p pichost-api test_image_list"
  test_code: |
    // Add to pichost-api/tests/category_test.rs
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize)]
    struct CreateCategoryRequest {
        name: String,
        parent_id: Option<Uuid>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct UpdateCategoryRequest {
        name: Option<String>,
    }

    #[test]
    fn test_create_category_request_serde() {
        let json = r#"{"name":"Blog","parent_id":null}"#;
        let req: CreateCategoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Blog");
        assert_eq!(req.parent_id, None);
    }

    #[test]
    fn test_create_category_request_with_parent() {
        let json = r#"{"name":"Rust","parent_id":"00000000-0000-0000-0000-000000000001"}"#;
        let req: CreateCategoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Rust");
        assert!(req.parent_id.is_some());
    }

    #[test]
    fn test_update_category_request_partial() {
        let json = r#"{"name":"New Name"}"#;
        let req: UpdateCategoryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, Some("New Name".into()));
    }
  impl_code: |
    // pichost-api/src/routes/categories.rs (NEW FILE)
    use axum::{
        extract::{Json, Path, State},
        http::StatusCode,
        response::IntoResponse,
        Extension, Router,
    };
    use pichost_core::models::Category;
    use serde::{Deserialize, Serialize};
    use sqlx::PgPool;
    use uuid::Uuid;
    use crate::middleware::auth::AuthUser;

    type RouteError = (StatusCode, Json<serde_json::Value>);
    const MAX_DEPTH: i32 = 2;

    #[derive(Debug, Deserialize)]
    pub struct CreateCategoryRequest {
        pub name: String,
        pub parent_id: Option<Uuid>,
    }

    #[derive(Debug, Deserialize)]
    pub struct UpdateCategoryRequest {
        pub name: Option<String>,
        pub parent_id: Option<Option<Uuid>>,
    }

    #[derive(Debug, Serialize, sqlx::FromRow)]
    pub struct CategoryTreeNode {
        pub id: Uuid,
        pub name: String,
        pub parent_id: Option<Uuid>,
        pub children: Vec<CategoryTreeNode>,
    }

    // ── list_categories ──
    pub async fn list_categories(
        State(pool): State<PgPool>,
        Extension(user): Extension<AuthUser>,
    ) -> Result<Json<Vec<CategoryTreeNode>>, RouteError> {
        let rows: Vec<Category> = sqlx::query_as::<_, Category>(
            "SELECT id, user_id, name, parent_id, created_at
             FROM categories WHERE user_id = $1 ORDER BY created_at",
        )
        .bind(user.id)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

        Ok(Json(build_tree(rows)))
    }

    fn build_tree(categories: Vec<Category>) -> Vec<CategoryTreeNode> {
        let mut roots: Vec<CategoryTreeNode> = Vec::new();
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

    // ── create_category ──
    pub async fn create_category(
        State(pool): State<PgPool>,
        Extension(user): Extension<AuthUser>,
        Json(req): Json<CreateCategoryRequest>,
    ) -> Result<(StatusCode, Json<Category>), RouteError> {
        // Validate name
        let name = req.name.trim().to_string();
        if name.is_empty() || name.len() > 128 {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Name must be 1-128 characters"}))));
        }

        // Validate max depth
        if let Some(pid) = req.parent_id {
            validate_depth(&pool, user.id, pid, 1).await?;
        }

        let category = sqlx::query_as::<_, Category>(
            "INSERT INTO categories (user_id, name, parent_id) VALUES ($1, $2, $3)
             RETURNING id, user_id, name, parent_id, created_at",
        )
        .bind(user.id)
        .bind(&name)
        .bind(req.parent_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint() == Some("categories_user_id_name_parent_id_key") {
                    return (StatusCode::CONFLICT, Json(serde_json::json!({"error": "Category name already exists"})));
                }
            }
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))
        })?;

        Ok((StatusCode::CREATED, Json(category)))
    }

    async fn validate_depth(pool: &PgPool, user_id: Uuid, parent_id: Uuid, current: i32) -> Result<(), RouteError> {
        if current >= MAX_DEPTH {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("Maximum category depth is {}", MAX_DEPTH)}))));
        }
        let parent: Option<Category> = sqlx::query_as::<_, Category>(
            "SELECT id, user_id, name, parent_id, created_at
             FROM categories WHERE id = $1 AND user_id = $2",
        )
        .bind(parent_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

        if parent.is_none() {
            return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Parent category not found"}))));
        }
        if let Some(grandparent_id) = parent.unwrap().parent_id {
            Box::pin(validate_depth(pool, user_id, grandparent_id, current + 1)).await?;
        }
        Ok(())
    }

    // ── get_category ──
    pub async fn get_category(
        State(pool): State<PgPool>,
        Extension(user): Extension<AuthUser>,
        Path(id): Path<Uuid>,
    ) -> Result<Json<Category>, RouteError> {
        sqlx::query_as::<_, Category>(
            "SELECT id, user_id, name, parent_id, created_at
             FROM categories WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user.id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Category not found"}))))
    }

    // ── update_category ──
    pub async fn update_category(
        State(pool): State<PgPool>,
        Extension(user): Extension<AuthUser>,
        Path(id): Path<Uuid>,
        Json(req): Json<UpdateCategoryRequest>,
    ) -> Result<Json<Category>, RouteError> {
        let existing = sqlx::query_as::<_, Category>(
            "SELECT id, user_id, name, parent_id, created_at
             FROM categories WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user.id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Category not found"}))))?;

        let new_name = req.name.unwrap_or(existing.name);
        let category = sqlx::query_as::<_, Category>(
            "UPDATE categories SET name = $1
             WHERE id = $2 AND user_id = $3
             RETURNING id, user_id, name, parent_id, created_at",
        )
        .bind(&new_name)
        .bind(id)
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

        Ok(Json(category))
    }

    // ── delete_category ──
    pub async fn delete_category(
        State(pool): State<PgPool>,
        Extension(user): Extension<AuthUser>,
        Path(id): Path<Uuid>,
    ) -> Result<StatusCode, RouteError> {
        let result = sqlx::query(
            "DELETE FROM categories WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user.id)
        .execute(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

        if result.rows_affected() == 0 {
            return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Category not found"}))));
        }

        Ok(StatusCode::OK)
    }

    // ── pichost-api/src/routes/mod.rs (trivial — add one line) ──
    pub mod categories;

    // ── pichost-api/src/main.rs (add category_routes function + nest) ──
    // Add after the existing route group registrations (inside build_router):
    fn category_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        use pichost_api::routes::categories;
        let pool = state.pool.clone();
        let protected = middleware::from_fn_with_state(
            state.clone(),
            pichost_api::middleware::auth::require_auth,
        );
        Router::new()
            .route("/", get(categories::list_categories).post(categories::create_category))
            .route("/{id}", get(categories::get_category)
                .patch(categories::update_category)
                .delete(categories::delete_category))
            .with_state(pool)
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                pichost_api::middleware::rate_limit::rate_limit_general,
            ))
            .route_layer(protected)
    }

    // In build_router(), add:
    .nest("/api/v1/categories", category_routes(state.clone()))
  verify:
    - "cargo test -p pichost-api test_create_category_request_serde -- --exact"
    - "cargo test -p pichost-api test_create_category_request_with_parent -- --exact"
    - "cargo test -p pichost-api test_update_category_request_partial -- --exact"
    - "cargo clippy --workspace -- -D warnings"

---

### T3: Image category operations — move, batch-move, gallery filter

- id: T3
  title: "Add image move/batch-move endpoints and category_id gallery filter"
  files:
    - pichost-api/src/routes/images.rs
    - pichost-api/src/services/upload.rs
    - pichost-api/tests/gallery_test.rs
  depends_on: [T2]
  breaking: false
  ac:
    - given: "authenticated user with an image and a category"
      when: "POST /api/v1/images/:id/move with {category_id: uuid}"
      then: "returns 200, image.category_id updated, other user's images unaffected"
    - given: "authenticated user with images in a category"
      when: "GET /api/v1/images?category_id=uuid"
      then: "returns only images assigned to that category"
    - given: "authenticated user"
      when: "POST /api/v1/images/batch-move with {image_ids: [...], category_id: uuid}"
      then: "returns 200 with moved count, only user's own images moved"
  regression:
    - "cargo test -p pichost-api test_image_list_query_defaults"
    - "cargo test -p pichost-api test_image_list_query_full"
    - "cargo test -p pichost-api test_image_list_response_total_pages"
  test_code: |
    // Add to pichost-api/tests/gallery_test.rs
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct MoveImageRequest {
        category_id: Uuid,
    }

    #[derive(Debug, Deserialize)]
    struct BatchMoveRequest {
        image_ids: Vec<Uuid>,
        category_id: Uuid,
    }

    #[test]
    fn test_move_image_request_serde() {
        let json = r#"{"category_id":"00000000-0000-0000-0000-000000000001"}"#;
        let req: MoveImageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.category_id, Uuid::nil());
    }

    #[test]
    fn test_batch_move_request_serde() {
        let json = r#"{"image_ids":["00000000-0000-0000-0000-000000000001","00000000-0000-0000-0000-000000000002"],"category_id":"00000000-0000-0000-0000-000000000003"}"#;
        let req: BatchMoveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image_ids.len(), 2);
    }

    #[test]
    fn test_image_list_query_with_category_id() {
        let query = "page=1&per_page=20&category_id=00000000-0000-0000-0000-000000000001";
        let params: pichost_api::services::upload::ImageListQuery =
            serde_urlencoded::from_str(query).unwrap();
        assert_eq!(params.category_id, Some(Uuid::nil()));
    }

    #[test]
    fn test_image_list_query_without_category_id() {
        let query = "page=1&per_page=20";
        let params: pichost_api::services::upload::ImageListQuery =
            serde_urlencoded::from_str(query).unwrap();
        assert_eq!(params.category_id, None);
    }
  impl_code: |
    // ── pichost-api/src/services/upload.rs ──

    // 1) Add category_id to ImageListQuery (around line 117):
    #[derive(Debug, Deserialize)]
    pub struct ImageListQuery {
        #[serde(default = "default_page")]
        pub page: u32,
        #[serde(default = "default_per_page")]
        pub per_page: u32,
        #[serde(default = "default_sort")]
        pub sort: String,
        #[serde(default = "default_order")]
        pub order: String,
        #[serde(default)]
        pub search: String,
        #[serde(default)]
        pub storage_config_id: Option<Uuid>,
        #[serde(default)]
        pub category_id: Option<Uuid>,  // NEW
    }

    // 2) Add category_id to ImageRow tuple type (around line 54-71):
    // Add Option<Uuid> as the 15th element (before storage_config_id/config_name/provider):
    pub(crate) type ImageRow = (
        Uuid,            // id
        String,          // public_key
        String,          // original_name
        String,          // url
        String,          // mime_type
        i64,             // file_size
        String,          // sha256
        Option<i32>,     // width
        Option<i32>,     // height
        String,          // status
        Option<String>,  // thumbnail_url
        Option<String>,  // webp_url
        DateTime<Utc>,   // created_at
        Option<Uuid>,    // category_id     ← NEW
        Option<Uuid>,    // storage_config_id
        Option<String>,  // config_name
        Option<String>,  // config_provider
    );

    // 3) Update from_row() to destructure the new field (around line 76-106):
    // Add `category_id` to the destructuring, add to UploadResult struct creation.

    // 4) Update count_user_images to add category_id WHERE clause:
    //   if let Some(cat_id) = query.category_id { sql.push_str(" AND i.category_id = "); sql.push_str(&format!("${}", param_count)); param_count += 1; }

    // 5) Update fetch_user_images — same pattern as count_user_images but also update SELECT to include category_id.

    // 6) Update list_user_images to pass category_id through.

    // ── pichost-api/src/routes/images.rs ──

    // 7) Add route handlers:

    // POST /api/v1/images/{id}/move
    pub async fn move_image(
        State(state): State<Arc<AppState>>,
        Extension(user): Extension<AuthUser>,
        Path(id): Path<Uuid>,
        Json(body): Json<MoveImageRequest>,
    ) -> Result<Json<serde_json::Value>, RouteError> {
        // Verify category belongs to user
        let cat = sqlx::query_as::<_, Category>(
            "SELECT id, user_id, name, parent_id, created_at
             FROM categories WHERE id = $1 AND user_id = $2",
        )
        .bind(body.category_id)
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Category not found"}))))?;

        let result = sqlx::query(
            "UPDATE images SET category_id = $1 WHERE id = $2 AND user_id = $3",
        )
        .bind(body.category_id)
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

        if result.rows_affected() == 0 {
            return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Image not found"}))));
        }

        Ok(Json(serde_json::json!({"message": "Image moved to category"})))
    }

    // POST /api/v1/images/batch-move
    pub async fn batch_move_images(
        State(state): State<Arc<AppState>>,
        Extension(user): Extension<AuthUser>,
        Json(body): Json<BatchMoveRequest>,
    ) -> Result<Json<serde_json::Value>, RouteError> {
        if body.image_ids.is_empty() {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "image_ids cannot be empty"}))));
        }
        if body.image_ids.len() > 100 {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Maximum 100 images per batch move"}))));
        }

        // Verify category belongs to user
        let cat = sqlx::query_as::<_, Category>(
            "SELECT id, user_id, name, parent_id, created_at
             FROM categories WHERE id = $1 AND user_id = $2",
        )
        .bind(body.category_id)
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Category not found"}))))?;

        let result = sqlx::query(
            "UPDATE images SET category_id = $1
             WHERE user_id = $2 AND id = ANY($3)",
        )
        .bind(body.category_id)
        .bind(user.id)
        .bind(&body.image_ids)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

        Ok(Json(serde_json::json!({
            "message": "Images moved to category",
            "moved": result.rows_affected()
        })))
    }

    // 8) In image_routes(), add:
    .route("/{id}/move", post(routes::images::move_image))
    .route("/batch-move", post(routes::images::batch_move_images))

    // 9) In count_user_images (lines 64-115 of images.rs), add category_id filter:
    // After the storage_config_id branch, add a similar branch for category_id.

    // 10) In fetch_user_images (lines 118-148 of images.rs), add category_id to SELECT and WHERE.
  verify:
    - "cargo test -p pichost-api test_move_image_request_serde -- --exact"
    - "cargo test -p pichost-api test_batch_move_request_serde -- --exact"
    - "cargo test -p pichost-api test_image_list_query_with_category_id -- --exact"
    - "cargo test -p pichost-api test_image_list_query_without_category_id -- --exact"
    - "cargo clippy --workspace -- -D warnings"

---

### T4: Frontend API types and functions for categories

- id: T4
  title: "Add Category types and CRUD API functions to frontend client"
  files:
    - web-ui/src/api/client.ts
  depends_on: [T3]
  breaking: false
  ac:
    - given: "frontend calling listCategories()"
      when: "API returns category tree"
      then: "returns typed CategoryTreeNode[] with children arrays"
    - given: "frontend calling createCategory({name: 'Blog'})"
      when: "API returns 201"
      then: "returns typed Category object"
  regression:
    - "npm run build"
  test_code: |
    // No frontend unit test infrastructure exists. TypeScript compilation
    // via `npx tsc --noEmit` and `npm run build` serves as the type-check.
  impl_code: |
    // web-ui/src/api/client.ts — add after existing exports

    export interface CategoryTreeNode {
      id: string
      name: string
      parent_id: string | null
      children: CategoryTreeNode[]
    }

    export interface Category {
      id: string
      user_id: string
      name: string
      parent_id: string | null
      created_at: string
    }

    export async function listCategories(): Promise<CategoryTreeNode[]> {
      return api.get('categories').json()
    }

    export async function createCategory(data: {
      name: string
      parent_id?: string | null
    }): Promise<Category> {
      return api.post('categories', { json: data }).json()
    }

    export async function getCategory(id: string): Promise<Category> {
      return api.get(`categories/${id}`).json()
    }

    export async function updateCategory(
      id: string,
      data: { name?: string; parent_id?: string | null },
    ): Promise<Category> {
      return api.patch(`categories/${id}`, { json: data }).json()
    }

    export async function deleteCategory(id: string): Promise<void> {
      await api.delete(`categories/${id}`)
    }

    export async function moveImageToCategory(
      imageId: string,
      categoryId: string,
    ): Promise<{ message: string }> {
      return api.post(`images/${imageId}/move`, {
        json: { category_id: categoryId },
      }).json()
    }

    export async function batchMoveImages(
      imageIds: string[],
      categoryId: string,
    ): Promise<{ message: string; moved: number }> {
      return api.post('images/batch-move', {
        json: { image_ids: imageIds, category_id: categoryId },
      }).json()
    }

    // Add category_id to PaginatedListParams if needed for gallery filter:
    export interface PaginatedListParams {
      page?: number
      per_page?: number
      sort?: 'created_at' | 'file_size' | 'original_name'
      order?: 'asc' | 'desc'
      search?: string
      storage_config_id?: string
      category_id?: string    // NEW
    }

    // Add category_id to ImageInfo:
    export interface ImageInfo {
      id: string
      public_key: string
      original_name: string
      url: string
      mime_type: string
      file_size: number
      sha256: string
      width: number | null
      height: number | null
      status: string
      thumbnail_url: string | null
      webp_url: string | null
      created_at: string
      storage_config?: { id: string; name: string; provider: string } | null
      category_id: string | null   // NEW
    }
  verify:
    - "npx tsc --noEmit"
    - "npm run build"

---

### T5: CategoryTree sidebar component

- id: T5
  title: "Build CategoryTree sidebar component with expand/collapse and selection"
  files:
    - web-ui/src/components/CategoryTree.tsx
  depends_on: [T4]
  breaking: false
  ac:
    - given: "user with categories"
      when: "CategoryTree renders"
      then: "displays root categories as expandable nodes, no selection highlighted"
    - given: "user clicks a category name"
      when: "onSelect callback fires with category id"
      then: "that category is highlighted, URL updates with ?category_id=..."
    - given: "user clicks expand arrow on parent category"
      when: "parent has children"
      then: "child categories appear indented, arrow rotates"
    - given: "user with no categories"
      when: "CategoryTree renders"
      then: "shows empty state with 'No categories yet' message and 'Create' button"
  regression:
    - "npm run build"
  test_code: |
    // No frontend unit test infrastructure. TypeScript compilation serves as test.
  impl_code: |
    // web-ui/src/components/CategoryTree.tsx (NEW FILE)
    import { useState } from 'react'
    import { useQuery } from '@tanstack/react-query'
    import { ChevronRight, Folder, FolderOpen, Plus } from 'lucide-react'
    import { listCategories, type CategoryTreeNode } from '../api/client'

    interface CategoryTreeProps {
      selectedId: string | null
      onSelect: (id: string | null) => void
      onAddCategory: (parentId: string | null) => void
      onEditCategory: (id: string, name: string) => void
      onDeleteCategory: (id: string) => void
    }

    const TreeNode = ({
      node,
      depth,
      selectedId,
      onSelect,
      onAddCategory,
      onEditCategory,
      onDeleteCategory,
    }: {
      node: CategoryTreeNode
      depth: number
      selectedId: string | null
      onSelect: (id: string | null) => void
      onAddCategory: (parentId: string | null) => void
      onEditCategory: (id: string, name: string) => void
      onDeleteCategory: (id: string) => void
    }) => {
      const [expanded, setExpanded] = useState(false)
      const hasChildren = node.children.length > 0
      const isSelected = selectedId === node.id

      return (
        <div>
          <div
            className={`group flex cursor-pointer items-center gap-1 rounded-md px-2 py-1 text-sm transition-colors ${
              isSelected
                ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
                : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]'
            }`}
            style={{ paddingLeft: `${depth * 16 + 8}px` }}
            onClick={() => onSelect(isSelected ? null : node.id)}
          >
            {hasChildren && (
              <button
                onClick={(e) => {
                  e.stopPropagation()
                  setExpanded(!expanded)
                }}
                className="flex h-4 w-4 items-center justify-center"
              >
                <ChevronRight
                  size={14}
                  className={`transition-transform ${expanded ? 'rotate-90' : ''}`}
                />
              </button>
            )}
            {!hasChildren && <span className="w-4" />}
            {expanded ? <FolderOpen size={16} /> : <Folder size={16} />}
            <span className="flex-1 truncate">{node.name}</span>
          </div>
          {expanded && hasChildren && (
            <div>
              {node.children.map((child) => (
                <TreeNode
                  key={child.id}
                  node={child}
                  depth={depth + 1}
                  selectedId={selectedId}
                  onSelect={onSelect}
                  onAddCategory={onAddCategory}
                  onEditCategory={onEditCategory}
                  onDeleteCategory={onDeleteCategory}
                />
              ))}
            </div>
          )}
        </div>
      )
    }

    export default function CategoryTree({
      selectedId,
      onSelect,
      onAddCategory,
      onEditCategory,
      onDeleteCategory,
    }: CategoryTreeProps) {
      const { data: categories, isLoading } = useQuery({
        queryKey: ['categories'],
        queryFn: listCategories,
      })

      return (
        <div className="flex flex-col">
          <div className="flex items-center justify-between px-2 py-2">
            <span className="text-xs font-medium uppercase tracking-wider text-[var(--color-text-muted)]">
              Categories
            </span>
            <button
              onClick={() => onAddCategory(null)}
              className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
              title="New category"
            >
              <Plus size={16} />
            </button>
          </div>

          {/* "All" option */}
          <div
            className={`cursor-pointer rounded-md px-2 py-1.5 text-sm transition-colors ${
              selectedId === null
                ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)] font-medium'
                : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)]'
            }`}
            style={{ paddingLeft: '8px' }}
            onClick={() => onSelect(null)}
          >
            All Images
          </div>

          {isLoading ? (
            <div className="px-4 py-2 text-xs text-[var(--color-text-muted)]">
              Loading...
            </div>
          ) : categories && categories.length > 0 ? (
            <div className="mt-1">
              {categories.map((cat) => (
                <TreeNode
                  key={cat.id}
                  node={cat}
                  depth={0}
                  selectedId={selectedId}
                  onSelect={onSelect}
                  onAddCategory={onAddCategory}
                  onEditCategory={onEditCategory}
                  onDeleteCategory={onDeleteCategory}
                />
              ))}
            </div>
          ) : (
            <div className="px-4 py-4 text-center text-xs text-[var(--color-text-muted)]">
              No categories yet
              <br />
              <button
                onClick={() => onAddCategory(null)}
                className="mt-1 text-[var(--color-accent)] hover:underline"
              >
                Create one
              </button>
            </div>
          )}
        </div>
      )
    }
  verify:
    - "npx tsc --noEmit"
    - "npm run build"

---

### T6: Gallery dual-column layout with category sidebar

- id: T6
  title: "Refactor Gallery to dual-column layout with CategoryTree sidebar"
  files:
    - web-ui/src/pages/Gallery.tsx
  depends_on: [T5]
  breaking: false
  ac:
    - given: "Gallery page with categories"
      when: "user clicks a category in sidebar"
      then: "grid filters to show only images in that category, URL updates with ?category_id="
    - given: "Gallery page with category selected"
      when: "user clicks 'All Images' in sidebar"
      then: "grid shows all images, category_id removed from query key and URL"
    - given: "narrow viewport (< 768px)"
      when: "Gallery renders"
      then: "sidebar collapses to a toggle button, grid takes full width"
  regression:
    - "npm run build"
  test_code: |
    // No frontend unit test infrastructure. TypeScript compilation serves as test.
  impl_code: |
    // web-ui/src/pages/Gallery.tsx — key changes:

    // 1) Import CategoryTree
    import CategoryTree from '../components/CategoryTree'

    // 2) Add category filter state
    const [categoryFilter, setCategoryFilter] = useState<string | null>(null)

    // 3) Add category_id to query key
    const { data, isLoading, isError, fetchNextPage, hasNextPage, isFetchingNextPage } =
      useInfiniteQuery({
        queryKey: ['images', { search, sort, order, storageConfigFilter, categoryFilter }],
        queryFn: ({ pageParam }) => listImages({
          page: pageParam,
          per_page: 20,
          sort,
          order,
          search: search || undefined,
          storage_config_id: storageConfigFilter || undefined,
          category_id: categoryFilter || undefined,
        }),
        initialPageParam: 1,
        getNextPageParam: (lastPage) =>
          lastPage.page < lastPage.total_pages ? lastPage.page + 1 : undefined,
        placeholderData: keepPreviousData,
      })

    // 4) Sync category_id to URL search params
    useEffect(() => {
      const params = new URLSearchParams(searchParams)
      if (categoryFilter) params.set('category_id', categoryFilter)
      else params.delete('category_id')
      setSearchParams(params, { replace: true })
    }, [categoryFilter])

    // 5) Read category_id from URL on mount
    const [searchParams, setSearchParams] = useSearchParams()
    useEffect(() => {
      const catFromUrl = searchParams.get('category_id')
      if (catFromUrl) setCategoryFilter(catFromUrl)
    }, [])

    // 6) Category management callbacks (UI triggers that T7 will hook into)
    const [showCategoryModal, setShowCategoryModal] = useState(false)
    const [categoryModalParentId, setCategoryModalParentId] = useState<string | null>(null)
    const handleAddCategory = (parentId: string | null) => {
      setCategoryModalParentId(parentId)
      setShowCategoryModal(true)
    }
    const handleEditCategory = (_id: string, _name: string) => {
      // Rename flow handled by CategoryTree's internal state (T7)
    }
    const handleDeleteCategory = (_id: string) => {
      // Delete confirmation handled by CategoryTree's internal state (T7)
    }
    // Note: the modal rendering + useMutation hooks are added in T7

    // 7) New layout structure:
    return (
      <div className="mx-auto flex max-w-7xl gap-4 p-4">
        {/* Sidebar */}
        <aside className="hidden w-56 shrink-0 md:block">
          <div className="sticky top-16 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-2 backdrop-blur-sm">
            <CategoryTree
              selectedId={categoryFilter}
              onSelect={setCategoryFilter}
              onAddCategory={handleAddCategory}
              onEditCategory={handleEditCategory}
              onDeleteCategory={handleDeleteCategory}
            />
          </div>
        </aside>

        {/* Main content area */}
        <div className="min-w-0 flex-1">
          {/* Filter bar: search, sort, order, batch actions */}
          <div className="mb-4 flex flex-wrap items-center gap-2">
            <SearchBar value={search} onChange={setSearch} />
            <SortDropdown sort={sort} order={order} onSortChange={setSort} onOrderChange={setOrder} />
            {/* Batch action bar when images selected */}
            {selected.size > 0 && (
              <div className="flex items-center gap-2">
                <span className="text-sm text-[var(--color-text-muted)]">
                  {selected.size} selected
                </span>
                <button onClick={handleBatchDelete} className="...">Delete</button>
                <button onClick={handleBatchMove} className="...">Move to...</button>
              </div>
            )}
          </div>

          {/* Grid — unchanged from current Gallery */}
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-3 lg:grid-cols-4">
            {/* ... existing image card rendering ... */}
          </div>
        </div>
      </div>
    )
  verify:
    - "npx tsc --noEmit"
    - "npm run build"

---

### T7: Category CRUD UI — create/rename/delete modals in sidebar

- id: T7
  title: "Add category create, rename, and delete UI to sidebar"
  files:
    - web-ui/src/components/CategoryTree.tsx
  depends_on: [T6]
  breaking: false
  ac:
    - given: "user clicks '+' button in sidebar"
      when: "create modal opens with name input and optional parent selector"
      then: "submitting creates category, tree refreshes, modal closes"
    - given: "user right-clicks a category"
      when: "context menu shows Rename / Delete options"
      then: "selecting Rename opens inline edit, selecting Delete shows confirmation"
    - given: "user confirms delete on a category with sub-categories"
      when: "API call succeeds"
      then: "category and children removed from tree"
  regression:
    - "npm run build"
  test_code: |
    // No frontend unit test infrastructure. TypeScript compilation serves as test.
  impl_code: |
    // web-ui/src/components/CategoryTree.tsx — extend with CRUD dialogs:
    // Add these imports:
    import { useMutation, useQueryClient } from '@tanstack/react-query'
    import { createCategory, updateCategory, deleteCategory, type CategoryTreeNode } from '../api/client'

    // Add to CategoryTree component (inside the default export function body):
    const queryClient = useQueryClient()
    const [showCreate, setShowCreate] = useState(false)
    const [createName, setCreateName] = useState('')
    const [createParentId, setCreateParentId] = useState<string | null>(null)
    const [renameId, setRenameId] = useState<string | null>(null)
    const [renameValue, setRenameValue] = useState('')
    const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null)

    const createMutation = useMutation({
      mutationFn: createCategory,
      onSuccess: () => {
        queryClient.invalidateQueries({ queryKey: ['categories'] })
        setShowCreate(false)
        setCreateName('')
      },
    })

    const updateMutation = useMutation({
      mutationFn: ({ id, ...data }: { id: string; name: string }) =>
        updateCategory(id, data),
      onSuccess: () => {
        queryClient.invalidateQueries({ queryKey: ['categories'] })
        setRenameId(null)
      },
    })

    const deleteMutation = useMutation({
      mutationFn: (id: string) => deleteCategory(id),
      onSuccess: () => {
        queryClient.invalidateQueries({ queryKey: ['categories'] })
        setDeleteConfirmId(null)
      },
    })

    // Handle create from external trigger (Gallery's handleAddCategory → modal)
    useEffect(() => {
      if (showCreate) {
        // Gallery passes parentId via a shared state — adapt the '+' button
        // click to open the create modal with the right parent
      }
    }, [showCreate])

    // Add create dialog (rendered inside CategoryTree's return, before closing </div>):
    {showCreate && (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={() => setShowCreate(false)}>
        <div className="w-80 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface-elevated)] p-4 shadow-xl" onClick={e => e.stopPropagation()}>
          <h3 className="mb-3 text-sm font-medium">New Category</h3>
          <input
            type="text"
            value={createName}
            onChange={e => setCreateName(e.target.value)}
            placeholder="Category name"
            className="mb-3 w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
            autoFocus
            onKeyDown={e => {
              if (e.key === 'Enter' && createName.trim()) {
                createMutation.mutate({ name: createName.trim(), parent_id: createParentId })
              }
              if (e.key === 'Escape') setShowCreate(false)
            }}
          />
          <div className="flex justify-end gap-2">
            <button onClick={() => setShowCreate(false)} className="rounded-lg px-3 py-1.5 text-sm text-[var(--color-text-muted)] hover:bg-[var(--color-surface)]">Cancel</button>
            <button onClick={() => { if (createName.trim()) createMutation.mutate({ name: createName.trim(), parent_id: createParentId }) }} disabled={!createName.trim()} className="rounded-lg bg-[var(--color-accent)] px-3 py-1.5 text-sm text-white disabled:opacity-50">Create</button>
          </div>
        </div>
      </div>
    )}

    // Add delete confirmation dialog:
    {deleteConfirmId && (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={() => setDeleteConfirmId(null)}>
        <div className="w-72 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface-elevated)] p-4 shadow-xl" onClick={e => e.stopPropagation()}>
          <h3 className="mb-2 text-sm font-medium">Delete Category</h3>
          <p className="mb-4 text-xs text-[var(--color-text-muted)]">This will also delete all sub-categories. Images will be unassigned.</p>
          <div className="flex justify-end gap-2">
            <button onClick={() => setDeleteConfirmId(null)} className="rounded-lg px-3 py-1.5 text-sm text-[var(--color-text-muted)] hover:bg-[var(--color-surface)]">Cancel</button>
            <button onClick={() => deleteMutation.mutate(deleteConfirmId)} className="rounded-lg bg-red-600 px-3 py-1.5 text-sm text-white">Delete</button>
          </div>
        </div>
      </div>
    )}

    // Add inline rename input in TreeNode:
    // When renameId === node.id, replace the <span>{node.name}</span> with:
    //   <input value={renameValue} onChange={...} onKeyDown={Enter→save, Escape→cancel}
    //          className="flex-1 rounded border px-1 py-0 text-sm" autoFocus />

    // Update TreeNode to accept callbacks that use these state setters:
    // Pass: onRename(id, currentName) → setRenameId(id); setRenameValue(currentName)
    // Pass: onDelete(id) → setDeleteConfirmId(id)
    // The '+' button at top calls: onAddCategory(null) → shows create modal via Gallery's handleAddCategory

    // Wire the external '+' button to trigger create modal:
    // Replace the existing '+' button's onClick:
    //   onClick={() => { setCreateParentId(null); setShowCreate(true) }}
    // And for child nodes, right-click "New sub-category" → setCreateParentId(node.id); setShowCreate(true)
  verify:
    - "npx tsc --noEmit"
    - "npm run build"

---

### T8: ImageDetail category assignment dropdown

- id: T8
  title: "Add category assignment dropdown to ImageDetail page"
  files:
    - web-ui/src/pages/ImageDetail.tsx
  depends_on: [T4, T2]
  breaking: false
  ac:
    - given: "ImageDetail page with an image"
      when: "user selects a category from dropdown"
      then: "image.category_id updates, gallery query invalidated"
    - given: "ImageDetail page with image already in a category"
      when: "page loads"
      then: "dropdown shows current category as selected"
    - given: "ImageDetail page"
      when: "user selects 'None' from category dropdown"
      then: "image removed from category (category_id = null)"
  regression:
    - "npm run build"
  test_code: |
    // No frontend unit test infrastructure. TypeScript compilation serves as test.
  impl_code: |
    // web-ui/src/pages/ImageDetail.tsx — add after existing image display section:

    // 1) Import hooks
    import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
    import { listCategories, moveImageToCategory } from '../api/client'

    // 2) Fetch categories for dropdown
    const { data: categories } = useQuery({
      queryKey: ['categories'],
      queryFn: listCategories,
    })

    // 3) Move mutation
    const queryClient = useQueryClient()
    const moveMutation = useMutation({
      mutationFn: ({ imageId, categoryId }: { imageId: string; categoryId: string }) =>
        moveImageToCategory(imageId, categoryId),
      onSuccess: () => {
        queryClient.invalidateQueries({ queryKey: ['image', id] })
        queryClient.invalidateQueries({ queryKey: ['images'] })
      },
    })

    // 4) Flatten category tree for dropdown
    function flattenCategories(
      nodes: CategoryTreeNode[],
    ): { id: string; name: string; depth: number }[] {
      const result: { id: string; name: string; depth: number }[] = []
      function walk(items: CategoryTreeNode[], depth: number) {
        for (const item of items) {
          result.push({ id: item.id, name: item.name, depth })
          if (item.children.length > 0) walk(item.children, depth + 1)
        }
      }
      walk(nodes, 0)
      return result
    }

    // 5) Render dropdown in the image info card:
    <div className="mt-3">
      <label className="block text-xs font-medium text-[var(--color-text-muted)] mb-1">
        Category
      </label>
      <select
        value={img?.category_id ?? ''}
        onChange={(e) => {
          const categoryId = e.target.value
          if (categoryId) {
            moveMutation.mutate({ imageId: id!, categoryId })
          }
        }}
        className="w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm text-[var(--color-text-primary)]"
      >
        <option value="">None</option>
        {categories && flattenCategories(categories).map((cat) => (
          <option key={cat.id} value={cat.id}>
            {'\u00A0\u00A0'.repeat(cat.depth)}{cat.name}
          </option>
        ))}
      </select>
    </div>
  verify:
    - "npx tsc --noEmit"
    - "npm run build"

---

## Verification Summary

After all tasks complete, run the full verification suite:

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
cd web-ui && npm run build
```

Expected: All pass. Version bump: `0.15.1` → `0.16.0` in `Cargo.toml` files and `package.json`.
