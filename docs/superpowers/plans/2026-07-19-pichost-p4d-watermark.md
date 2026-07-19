# P4-D: 服务端图片水印 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply configurable text watermarks to images in the Worker pipeline, with JSONB config stored per-user and a Settings UI for configuration.

**Architecture:** Watermark is applied in `pichost-worker` after source image decode and before thumbnail/WebP variant generation. User watermark config (text, font, position, color, rotation, scale) is stored as JSONB on the `users` table. The `PATCH /users/me` endpoint is extended to accept `watermark_config`. A `WatermarkSettings` component is added to the Settings page. The `imageproc` + `rusttype` crates render TTF text onto `DynamicImage`.

**Tech Stack:** Rust (imageproc, rusttype, serde_json), PostgreSQL JSONB, React 19 + TypeScript, TanStack Query, Tailwind CSS 4

## Global Constraints

- Rust: functions ≤50 lines, lines ≤120 chars
- `cargo clippy --workspace -- -D warnings` — zero warnings
- `cargo test --workspace` — all tests pass
- `npm run build` — frontend builds clean
- Version bump: 0.16.0 → 0.16.1 (patch)
- No `query!` macro — all queries use `query_as` / `query_scalar`
- TDD: test code written and verified to fail BEFORE implementation code

---

## Task Dependency Graph

```
T0 (migration) ──→ T1 (models + queries) ──→ T2 (PATCH /users/me)
                       │                          │
                       ├──→ T3 (worker: config fetch)    │
                       │         │                       ↓
T4 (deps+fonts) ──→ T5 (watermark render) ──→ T6 (pipeline integration)
                                                       │
T2 ──→ T7 (frontend: API + component)
              │
              └──→ T8 (frontend: Settings integration)
```

---

### T0: Add `watermark_config` JSONB column to users table

- id: T0
- title: "Add watermark_config JSONB column to users table"
- files:
  - Create: `migrations/0010_add_watermark_config.sql`
- depends_on: []
- breaking: true
- ac:
  - given: "migration 0010 exists and is applied"
    when: "`sqlx::migrate!()` runs at API startup"
    then: "`users` table has `watermark_config JSONB` column defaulting to NULL"
  - given: "existing users with no watermark config"
    when: "SELECT watermark_config FROM users WHERE watermark_config IS NULL"
    then: "returns NULL rows (existing users unaffected)"

- regression:
  - "cargo test --workspace -- --skip ignored"
  - "Applied migrations count increases from 9 to 10 at API startup"

- migration_verify:
  - "Check column exists: `SELECT column_name, data_type FROM information_schema.columns WHERE table_name='users' AND column_name='watermark_config'` → returns `watermark_config | jsonb`"

- test_code: |
  ```rust
  // No unit test needed — migration is verified by sqlx::migrate!() at startup.
  // Integration verification: after migration runs, query the column.
  ```

- impl_code: |
  ```sql
  -- migrations/0010_add_watermark_config.sql
  ALTER TABLE users ADD COLUMN IF NOT EXISTS watermark_config JSONB;
  COMMENT ON COLUMN users.watermark_config IS 'Per-user watermark configuration. NULL = watermark disabled. JSON schema: {enabled, text, font, font_size, color, rotation, scale, position, margin_x, margin_y}';
  ```

- verify:
  - "cargo clippy --workspace -- -D warnings"
  - "cargo test --workspace -- --skip ignored"

---

### T1: Define WatermarkConfig type and update User model + all query tuples

- id: T1
- title: "Define WatermarkConfig type and add watermark_config field to User/UserProfile/AuthUser/UpdateProfileRequest"
- files:
  - Modify: `pichost-core/src/models.rs`
- depends_on: [T0]
- breaking: true
- ac:
  - given: "WatermarkConfig struct exists with all fields from spec §5.3"
    when: "serde_json::from_str::<WatermarkConfig>(json)"
    then: "deserializes all 10 fields with correct types"
  - given: "WatermarkConfig with enabled=false"
    when: "config.enabled is checked"
    then: "returns false"
  - given: "User struct includes watermark_config: Option<WatermarkConfig>"
    when: "User struct is compiled"
    then: "field exists and is Option<WatermarkConfig>"
  - given: "AuthUser struct includes watermark_config: Option<WatermarkConfig>"
    when: "JWT middleware extracts AuthUser claims"
    then: "watermark_config field is accessible for Worker consumption"
  - given: "UpdateProfileRequest includes watermark_config: Option<Option<WatermarkConfig>>"
    when: "JSON body `{\"watermark_config\": {\"enabled\": true, \"text\": \"test\"}}` is deserialized"
    then: "watermark_config is Some(Some(WatermarkConfig{...}))"
  - given: "UpdateProfileRequest with `\"watermark_config\": null`"
    when: "JSON body is deserialized"
    then: "watermark_config is Some(None) — signal to clear the config"

- regression:
  - "cargo test -p pichost-core -- --skip ignored"
  - "cargo test -p pichost-api test_auth_login -- --exact"

- migration_verify:
  - "Verify JSONB round-trip: after T0 migration, query `SELECT watermark_config FROM users LIMIT 1` — column exists and returns NULL for existing users"
  - "Verify WatermarkConfig deserialization from DB JSONB: `serde_json::from_value::<WatermarkConfig>(json_value)` succeeds for valid JSON and fails gracefully for invalid JSON"

- test_code: |
  ```rust
  // In pichost-core/src/models.rs, add a test module (or use existing #[cfg(test)])
  #[cfg(test)]
  mod watermark_tests {
      use super::*;

      #[test]
      fn test_watermark_config_deserialize_full() {
          let json = r#"{
              "enabled": true,
              "text": "@testuser",
              "font": "NotoSansSC-Regular",
              "font_size": 48,
              "color": "rgba(255, 255, 255, 0.5)",
              "rotation": -30.0,
              "scale": 0.15,
              "position": "bottom-right",
              "margin_x": 20,
              "margin_y": 20
          }"#;
          let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
          assert!(cfg.enabled);
          assert_eq!(cfg.text, "@testuser");
          assert_eq!(cfg.font, "NotoSansSC-Regular");
          assert_eq!(cfg.font_size, 48);
          assert_eq!(cfg.color, "rgba(255, 255, 255, 0.5)");
          assert!((cfg.rotation - (-30.0)).abs() < f64::EPSILON);
          assert!((cfg.scale - 0.15).abs() < f64::EPSILON);
          assert_eq!(cfg.position, WatermarkPosition::BottomRight);
          assert_eq!(cfg.margin_x, 20);
          assert_eq!(cfg.margin_y, 20);
      }

      #[test]
      fn test_watermark_config_defaults() {
          let json = r#"{"enabled": true, "text": "hello"}"#;
          let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
          assert_eq!(cfg.font, "NotoSansSC-Regular");  // default
          assert_eq!(cfg.font_size, 48);
          assert_eq!(cfg.color, "rgba(255, 255, 255, 0.5)");
          assert!((cfg.rotation - (-30.0)).abs() < f64::EPSILON);
          assert!((cfg.scale - 0.15).abs() < f64::EPSILON);
          assert_eq!(cfg.position, WatermarkPosition::BottomRight);
          assert_eq!(cfg.margin_x, 20);
          assert_eq!(cfg.margin_y, 20);
      }

      #[test]
      fn test_watermark_config_disabled() {
          let json = r#"{"enabled": false, "text": ""}"#;
          let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
          assert!(!cfg.enabled);
          assert_eq!(cfg.text, "");
      }

      #[test]
      fn test_watermark_config_position_enum() {
          let json = r#"{"enabled": true, "text": "x", "position": "tile"}"#;
          let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
          assert_eq!(cfg.position, WatermarkPosition::Tile);

          let json = r#"{"enabled": true, "text": "x", "position": "center"}"#;
          let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
          assert_eq!(cfg.position, WatermarkPosition::Center);

          let json = r#"{"enabled": true, "text": "x", "position": "top-left"}"#;
          let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
          assert_eq!(cfg.position, WatermarkPosition::TopLeft);
      }

      #[test]
      fn test_update_profile_request_watermark_null_means_clear() {
          let json = r#"{"watermark_config": null}"#;
          let req: UpdateProfileRequest = serde_json::from_str(json).unwrap();
          assert_eq!(req.username, None);
          assert_eq!(req.email, None);
          assert_eq!(req.watermark_config, Some(None)); // explicit null → clear
      }

      #[test]
      fn test_update_profile_request_watermark_absent() {
          let json = r#"{"username": "bob"}"#;
          let req: UpdateProfileRequest = serde_json::from_str(json).unwrap();
          assert_eq!(req.username, Some("bob".to_string()));
          assert_eq!(req.watermark_config, None); // absent → don't touch
      }
  }
  ```

- impl_code: |
  ```rust
  // In pichost-core/src/models.rs

  use serde::{Deserialize, Serialize};

  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  #[serde(rename_all = "lowercase")]
  pub enum WatermarkPosition {
      TopLeft,
      TopRight,
      BottomLeft,
      BottomRight,
      Center,
      Tile,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct WatermarkConfig {
      #[serde(default)]                    // false if omitted
      pub enabled: bool,
      #[serde(default)]                    // "" if omitted
      pub text: String,
      #[serde(default = "default_font")]
      pub font: String,
      #[serde(default = "default_font_size")]
      pub font_size: u32,
      #[serde(default = "default_color")]
      pub color: String,
      #[serde(default = "default_rotation")]
      pub rotation: f64,
      #[serde(default = "default_scale")]
      pub scale: f64,
      #[serde(default)]
      pub position: WatermarkPosition,
      #[serde(default = "default_margin")]
      pub margin_x: u32,
      #[serde(default = "default_margin")]
      pub margin_y: u32,
  }

  fn default_font() -> String { "NotoSansSC-Regular".into() }
  fn default_font_size() -> u32 { 48 }
  fn default_color() -> String { "rgba(255, 255, 255, 0.5)".into() }
  fn default_rotation() -> f64 { -30.0 }
  fn default_scale() -> f64 { 0.15 }
  fn default_margin() -> u32 { 20 }

  impl Default for WatermarkPosition {
      fn default() -> Self { WatermarkPosition::BottomRight }
  }

  // --- Add field to User struct (line 6) ---
  pub struct User {
      // ... existing fields ...
      pub watermark_config: Option<WatermarkConfig>,
  }

  // --- Add field to UserProfile (line 128) ---
  pub struct UserProfile {
      // ... existing fields ...
      pub watermark_config: Option<WatermarkConfig>,
  }

  // --- Add field to UpdateProfileRequest (line 142) ---
  pub struct UpdateProfileRequest {
      pub username: Option<String>,
      pub email: Option<String>,
      pub storage_backend: Option<String>,
      #[serde(default, deserialize_with = "deserialize_optional_optional")]
      pub watermark_config: Option<Option<WatermarkConfig>>,
  }

  // Custom deserializer: absent → None, null → Some(None), value → Some(Some(value))
  fn deserialize_optional_optional<'de, D, T>(
      deserializer: D,
  ) -> Result<Option<Option<T>>, D::Error>
  where
      D: serde::Deserializer<'de>,
      T: serde::Deserialize<'de>,
  {
      Ok(Some(Option::deserialize(deserializer)?))
  }

  // --- Update AuthUser struct in pichost-api/src/routes/auth.rs:73 ---
  // Add: pub watermark_config: Option<WatermarkConfig>,

  // --- Update ALL query tuples that read users table ---
  // Every sqlx::query_as that SELECTs from users must add watermark_config as 10th/11th column.
  // Affected locations (verified by grep for "FROM users" in query_as):
  // - pichost-api/src/routes/users.rs:135,264 (get_my_profile, update_my_profile)
  // - pichost-api/src/routes/admin.rs:177 (fetch_and_merge_user_fields)
  // - pichost-api/src/routes/admin.rs:80,104,131,147,270 (various admin queries)
  // - pichost-api/src/routes/auth.rs:* (login, refresh, register — may use UserInfo not User)
  // - pichost-worker/src/pipeline.rs (new: fetch watermark_config per task)
  ```

- verify:
  - "cargo test -p pichost-core -- test_watermark_config -- --exact"
  - "cargo clippy --workspace -- -D warnings"

---

### T2: Extend PATCH /users/me and admin PATCH /users/:id for watermark_config

- id: T2
- title: "Update user profile and admin update handlers to accept watermark_config"
- files:
  - Modify: `pichost-api/src/routes/users.rs`
  - Modify: `pichost-api/src/routes/admin.rs`
- depends_on: [T1]
- breaking: false
- ac:
  - given: "authenticated user sends PATCH /users/me with `{\"watermark_config\": {\"enabled\": true, \"text\": \"@me\"}}`"
    when: "the handler processes the request"
    then: "response includes `watermark_config.enabled: true, text: '@me'`"
  - given: "authenticated user sends PATCH /users/me with `{\"watermark_config\": null}`"
    when: "the handler processes the request"
    then: "response includes `watermark_config: null` (config cleared)"
  - given: "authenticated user sends PATCH /users/me with `{\"username\": \"newname\"}` (no watermark_config)"
    when: "the handler processes the request"
    then: "watermark_config in response is unchanged from DB"
  - given: "admin sends PATCH /admin/users/:id with `{\"watermark_config\": {\"enabled\": true, \"text\": \"@user\"}}`"
    when: "the handler processes the request"
    then: "user's watermark_config is updated and returned in response"
  - given: "admin sends PATCH /admin/users/:id with empty body"
    when: "the handler processes the request"
    then: "user's watermark_config is unchanged"

- regression:
  - "cargo test -p pichost-api test_update_profile -- --exact"
  - "cargo test -p pichost-api test_admin_update_user -- --exact"

- test_code: |
  ```rust
  // In pichost-api/tests/watermark_api_test.rs (create new file)
  //
  // Note: These tests require PostgreSQL + Redis (ignored by default like other integration tests).
  // Run with: cargo test -p pichost-api test_watermark -- --ignored

  // Test 1: Update watermark config via PATCH /users/me
  #[tokio::test]
  #[ignore] // needs DB
  async fn test_update_watermark_config_via_profile() {
      let app = spawn_test_app().await;
      let token = register_and_login(&app, "wmuser1").await;

      // Set watermark config
      let resp = app
          .patch("/api/v1/users/me")
          .header("Authorization", format!("Bearer {}", token))
          .json(&serde_json::json!({
              "watermark_config": {
                  "enabled": true,
                  "text": "@wmuser1",
                  "font_size": 36,
                  "position": "top-left"
              }
          }))
          .send()
          .await;
      assert_eq!(resp.status(), 200);
      let body: serde_json::Value = resp.json().await;
      let wm = &body["watermark_config"];
      assert_eq!(wm["enabled"], true);
      assert_eq!(wm["text"], "@wmuser1");
      assert_eq!(wm["font_size"], 36);
      assert_eq!(wm["position"], "top-left");
      // Defaults should be filled in
      assert_eq!(wm["font"], "NotoSansSC-Regular");
      assert_eq!(wm["color"], "rgba(255, 255, 255, 0.5)");
  }

  // Test 2: Clear watermark config by sending null
  #[tokio::test]
  #[ignore] // needs DB
  async fn test_clear_watermark_config() {
      let app = spawn_test_app().await;
      let token = register_and_login(&app, "wmuser2").await;

      // First set it
      app.patch("/api/v1/users/me")
          .header("Authorization", format!("Bearer {}", token))
          .json(&serde_json::json!({"watermark_config": {"enabled": true, "text": "temp"}}))
          .send().await;

      // Then clear it
      let resp = app
          .patch("/api/v1/users/me")
          .header("Authorization", format!("Bearer {}", token))
          .json(&serde_json::json!({"watermark_config": null}))
          .send()
          .await;
      assert_eq!(resp.status(), 200);
      let body: serde_json::Value = resp.json().await;
      assert!(body["watermark_config"].is_null());
  }

  // Test 3: Update watermark without changing other fields
  #[tokio::test]
  #[ignore] // needs DB
  async fn test_partial_update_does_not_clear_watermark() {
      let app = spawn_test_app().await;
      let token = register_and_login(&app, "wmuser3").await;

      // Set watermark
      app.patch("/api/v1/users/me")
          .header("Authorization", format!("Bearer {}", token))
          .json(&serde_json::json!({"watermark_config": {"enabled": true, "text": "keepme"}}))
          .send().await;

      // Update only username
      let resp = app
          .patch("/api/v1/users/me")
          .header("Authorization", format!("Bearer {}", token))
          .json(&serde_json::json!({"username": "wmuser3_new"}))
          .send()
          .await;
      assert_eq!(resp.status(), 200);
      let body: serde_json::Value = resp.json().await;
      assert_eq!(body["username"], "wmuser3_new");
      assert_eq!(body["watermark_config"]["text"], "keepme");
  }
  ```

- impl_code: |
  ```rust
  // --- pichost-api/src/routes/users.rs: update_my_profile() ---
  // Add to the UPDATE SQL (around line 231-262):

  // Currently the SQL is:
  // UPDATE users SET
  //     username = COALESCE($1, username),
  //     email = CASE WHEN $2::boolean THEN $3 ELSE email END,
  //     storage_backend = COALESCE($4, storage_backend),
  //     updated_at = now()
  // WHERE id = $5

  // Extend with watermark_config:
  let watermark_json: Option<Option<serde_json::Value>> = body.watermark_config
      .map(|cfg| cfg.map(|c| serde_json::to_value(c).unwrap_or_default()));

  let update_sql = format!(
      "UPDATE users SET \
       username = COALESCE($1, username), \
       email = CASE WHEN $2::boolean THEN $3 ELSE email END, \
       storage_backend = COALESCE($4, storage_backend), \
       watermark_config = CASE \
           WHEN $6::boolean THEN $7::jsonb \
           ELSE watermark_config \
       END, \
       updated_at = now() \
       WHERE id = $5",
  );

  let watermark_flag = watermark_json.is_some();
  let watermark_value = watermark_json.unwrap_or_default();

  sqlx::query(&update_sql)
      .bind(&body.username)
      .bind(has_email)
      .bind(&body.email)
      .bind(&body.storage_backend)
      .bind(user_id)
      .bind(watermark_flag)
      .bind(&watermark_value)
      .execute(&*pool)
      .await?;

  // The re-fetch query (line 264) also needs watermark_config added
  // as a new column in the tuple. Add Option<serde_json::Value> position.

  // --- pichost-api/src/routes/admin.rs ---
  // 1. Add watermark_config field to AdminUpdateUserBody struct
  //    watermark_config: Option<Option<serde_json::Value>>

  // 2. Add watermark_config to UserUpdateParams struct (line 64)
  //    watermark_config: Option<serde_json::Value>

  // 3. Update fetch_and_merge_user_fields() to include watermark_config
  //    in the SELECT tuple (add Option<serde_json::Value>)

  // 4. Update execute_user_update() to SET watermark_config = ...
  //    using the same CASE WHEN pattern
  ```

- verify:
  - "cargo test -p pichost-api test_watermark -- --ignored"
  - "cargo clippy --workspace -- -D warnings"

---

### T3: Fetch watermark_config in Worker process_task

- id: T3
- title: "Fetch watermark_config from DB in Worker process_task and pass to pipeline"
- files:
  - Modify: `pichost-worker/src/pipeline.rs`
  - Modify: `pichost-core/src/config.rs`
- depends_on: [T1]
- breaking: false
- ac:
  - given: "TaskPayload with user_id and watermark_config.enabled=true in DB"
    when: "process_task() fetches user's watermark_config"
    then: "watermark_config is Some(WatermarkConfig{enabled: true, ...})"
  - given: "TaskPayload with user_id and watermark_config IS NULL in DB"
    when: "process_task() fetches user's watermark_config"
    then: "watermark_config is None"
  - given: "TaskPayload with user_id and watermark_config.enabled=false in DB"
    when: "process_task() fetches user's watermark_config"
    then: "watermark_config is Some(WatermarkConfig{enabled: false, ...})"

- regression:
  - "cargo test -p pichost-worker -- --skip ignored"
  - "Existing worker test for process_image_variants keeps passing"

- test_code: |
  ```rust
  // In pichost-worker/src/pipeline.rs, add inline test:

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_watermark_config_parse_from_jsonb() {
          let json = serde_json::json!({
              "enabled": true,
              "text": "@test",
              "font": "Arial",
              "font_size": 24,
              "color": "rgba(0,0,0,0.5)",
              "rotation": 0.0,
              "scale": 0.1,
              "position": "center",
              "margin_x": 10,
              "margin_y": 10
          });
          let cfg: WatermarkConfig = serde_json::from_value(json).unwrap();
          assert!(cfg.enabled);
          assert_eq!(cfg.text, "@test");
          assert_eq!(cfg.font, "Arial");
          assert_eq!(cfg.position, WatermarkPosition::Center);
      }

      #[test]
      fn test_watermark_config_null_is_none() {
          let cfg: Option<WatermarkConfig> =
              serde_json::from_value(serde_json::Value::Null).unwrap_or(None);
          assert!(cfg.is_none());
      }
  }
  ```

- impl_code: |
  ```rust
  // In pichost-worker/src/pipeline.rs, inside process_task():

  // After reading source image (line 41), before process_image_variants (line 46):

  // Fetch watermark config from DB
  let watermark_config: Option<WatermarkConfig> = sqlx::query_scalar::<_, Option<serde_json::Value>>(
      "SELECT watermark_config FROM users WHERE id = $1"
  )
  .bind(task.user_id)
  .fetch_optional(pool)
  .await
  .map_err(|e| PipelineError::Database(format!("Failed to fetch watermark config: {}", e)))?
  .flatten()
  .and_then(|v| serde_json::from_value(v).ok());

  // Pass watermark_config to process_image_variants
  let (thumb_written, webp_written) = process_image_variants(
      &img, fmt, backend.as_ref(), &task.source_key,
      &config.worker.processing,
      watermark_config.as_ref(),  // NEW parameter
  )
  .await
  .map_err(|e| PipelineError::Pipeline(format!("Variant generation failed: {}", e)))?;
  ```

- verify:
  - "cargo check -p pichost-worker"
  - "cargo clippy --workspace -- -D warnings"

---

### T4: Add imageproc + rusttype dependencies and embed TTF fonts

- id: T4
- title: "Add imageproc/rusttype crates and embed 5 built-in TTF fonts"
- files:
  - Modify: `pichost-worker/Cargo.toml`
  - Create: `pichost-worker/src/fonts.rs`
- depends_on: []
- breaking: true
- ac:
  - given: "imageproc and rusttype are in Cargo.toml dependencies"
    when: "cargo build -p pichost-worker"
    then: "compiles without errors"
  - given: "fonts module exports load_font(name: &str) -> Result<Font<'static>, String>"
    when: "load_font(\"NotoSansSC-Regular\") is called"
    then: "returns Ok(Font) loaded from embedded bytes"
  - given: "load_font(\"unknown-font\") is called"
    when: "the font name is not in the built-in list"
    then: "returns Err with descriptive message listing available fonts"

- regression:
  - "cargo test -p pichost-worker -- --skip ignored"
  - "cargo clippy --workspace -- -D warnings"

- test_code: |
  ```rust
  // In pichost-worker/src/fonts.rs, add inline tests:

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_load_builtin_fonts() {
          for name in builtin_font_names() {
              let font = load_font(name);
              assert!(font.is_ok(), "Failed to load font: {}", name);
          }
      }

      #[test]
      fn test_load_nonexistent_font() {
          let result = load_font("ComicSans");
          assert!(result.is_err());
          let err = result.unwrap_err();
          assert!(err.contains("Available fonts"));
      }

      #[test]
      fn test_load_noto_sc_regular() {
          let font = load_font("NotoSansSC-Regular").unwrap();
          // Verify font renders something — measure glyph dimensions
          let scale = rusttype::Scale::uniform(48.0);
          let glyph: Vec<_> = font.layout("测试", scale, rusttype::point(0.0, 0.0)).collect();
          assert!(!glyph.is_empty(), "Font should have glyphs for Chinese text");
      }
  }
  ```

- impl_code: |
  ```toml
  # In pichost-worker/Cargo.toml, add dependencies:
  imageproc = "0.25"
  rusttype = "0.9"
  ```

  ```rust
  // pichost-worker/src/fonts.rs (new file)
  use rusttype::{Font, Scale, point};

  /// Load a built-in font from embedded bytes.
  pub fn load_font(name: &str) -> Result<Font<'static>, String> {
      let bytes: &[u8] = match name {
          "NotoSansSC-Regular" => include_bytes!("../fonts/NotoSansSC-Regular.ttf"),
          "NotoSans-Regular"   => include_bytes!("../fonts/NotoSans-Regular.ttf"),
          "Arial"              => include_bytes!("../fonts/Arial.ttf"),
          "DejaVuSans"         => include_bytes!("../fonts/DejaVuSans.ttf"),
          "FiraCode-Regular"   => include_bytes!("../fonts/FiraCode-Regular.ttf"),
          other => return Err(format!(
              "Unknown font: '{}'. Available fonts: NotoSansSC-Regular, NotoSans-Regular, Arial, DejaVuSans, FiraCode-Regular",
              other
          )),
      };
      Font::try_from_bytes(bytes)
          .ok_or_else(|| format!("Failed to parse font: {}", name))
  }

  /// List all built-in font names.
  pub fn builtin_font_names() -> Vec<&'static str> {
      vec![
          "NotoSansSC-Regular",
          "NotoSans-Regular",
          "Arial",
          "DejaVuSans",
          "FiraCode-Regular",
      ]
  }

  /// Calculate font size scaled relative to image diagonal
  pub fn scaled_font_size(img_diagonal: f32, base_size: u32, scale: f64) -> f32 {
      (base_size as f64 * scale * img_diagonal as f64 / 1000.0) as f32
  }
  ```

- verify:
  - "cargo build -p pichost-worker"
  - "cargo test -p pichost-worker -- test_load -- --exact"
  - "cargo clippy --workspace -- -D warnings"

---

### T5: Implement apply_watermark() rendering function

- id: T5
- title: "Implement apply_watermark() function with text overlay, positioning, rotation, and tiling"
- files:
  - Create: `pichost-worker/src/watermark.rs`
- depends_on: [T4]
- breaking: false
- ac:
  - given: "WatermarkConfig with enabled=false"
    when: "apply_watermark(img, config) is called"
    then: "returns clone of the original image (no-op)"
  - given: "WatermarkConfig with enabled=true and position=BottomRight"
    when: "apply_watermark(img, config) is called"
    then: "output image has text rendered near bottom-right corner with margins"
  - given: "WatermarkConfig with position=Tile"
    when: "apply_watermark(img, config) is called"
    then: "output image has tiled pattern of watermark text across the image"
  - given: "WatermarkConfig with rotation=-30"
    when: "apply_watermark(img, config) is called"
    then: "text is rendered at -30 degree rotation"
  - given: "WatermarkConfig with color='rgba(255, 0, 0, 0.3)'"
    when: "apply_watermark(img, config) is called"
    then: "text uses semi-transparent red color"
  - given: "Empty text in watermark config"
    when: "apply_watermark(img, config) is called"
    then: "returns clone of the original image (no text to render)"

- regression:
  - "cargo test -p pichost-worker -- --skip ignored"

- test_code: |
  ```rust
  // In pichost-worker/src/watermark.rs, inline tests:

  #[cfg(test)]
  mod tests {
      use super::*;
      use image::{DynamicImage, RgbaImage, Rgba};

      fn test_image() -> DynamicImage {
          let mut img = RgbaImage::new(800, 600);
          for pixel in img.pixels_mut() {
              *pixel = Rgba([128, 128, 128, 255]);
          }
          DynamicImage::ImageRgba8(img)
      }

      fn test_config() -> WatermarkConfig {
          WatermarkConfig {
              enabled: true,
              text: "@test".into(),
              font: "DejaVuSans".into(),
              font_size: 48,
              color: "rgba(255, 0, 0, 0.8)".into(),
              rotation: 0.0,
              scale: 0.15,
              position: WatermarkPosition::BottomRight,
              margin_x: 20,
              margin_y: 20,
          }
      }

      #[test]
      fn test_disabled_watermark_returns_clone() {
          let img = test_image();
          let mut config = test_config();
          config.enabled = false;
          let result = apply_watermark(&img, &config);
          assert!(result.is_ok());
          // Dimensions unchanged
          assert_eq!(result.unwrap().width(), 800);
          assert_eq!(result.unwrap().height(), 600);
      }

      #[test]
      fn test_empty_text_returns_clone() {
          let img = test_image();
          let mut config = test_config();
          config.text = "".into();
          let result = apply_watermark(&img, &config);
          assert!(result.is_ok());
      }

      #[test]
      fn test_watermark_changes_image() {
          let img = test_image();
          let result = apply_watermark(&img, &test_config());
          assert!(result.is_ok());
          let watermarked = result.unwrap();
          // Dimensions preserved
          assert_eq!(watermarked.width(), 800);
          assert_eq!(watermarked.height(), 600);
          // Pixels should differ (text overlay changed some pixels)
          let orig_pixel = img.to_rgba8().get_pixel(400, 560); // near bottom-right
          let new_pixel = watermarked.to_rgba8().get_pixel(400, 560);
          // At least some pixels should differ
          let mut diff_count = 0;
          for y in 0..600 {
              for x in 0..800 {
                  if img.to_rgba8().get_pixel(x, y) != watermarked.to_rgba8().get_pixel(x, y) {
                      diff_count += 1;
                  }
              }
          }
          assert!(diff_count > 0, "Watermark should change at least some pixels");
      }

      #[test]
      fn test_invalid_font_returns_error() {
          let img = test_image();
          let mut config = test_config();
          config.font = "NoSuchFont".into();
          let result = apply_watermark(&img, &config);
          assert!(result.is_err());
      }

      #[test]
      fn test_all_positions() {
          let img = test_image();
          let positions = vec![
              WatermarkPosition::TopLeft,
              WatermarkPosition::TopRight,
              WatermarkPosition::BottomLeft,
              WatermarkPosition::BottomRight,
              WatermarkPosition::Center,
              WatermarkPosition::Tile,
          ];
          for pos in positions {
              let mut config = test_config();
              config.position = pos;
              let result = apply_watermark(&img, &config);
              assert!(result.is_ok(), "Position {:?} failed", pos);
              assert_eq!(result.unwrap().width(), 800);
          }
      }
  }
  ```

- impl_code: |
  ```rust
  // pichost-worker/src/watermark.rs
  use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
  use imageproc::drawing::draw_text_mut;
  use rusttype::{Font, Scale};
  use pichost_core::models::{WatermarkConfig, WatermarkPosition};
  use crate::fonts;

  /// Apply watermark text overlay to a DynamicImage.
  /// Returns a new DynamicImage with watermark applied, or the original if disabled.
  pub fn apply_watermark(
      img: &DynamicImage,
      config: &WatermarkConfig,
  ) -> Result<DynamicImage, String> {
      if !config.enabled || config.text.is_empty() {
          return Ok(img.clone());
      }

      let font = fonts::load_font(&config.font)?;
      let (w, h) = (img.width() as f32, img.height() as f32);
      let diagonal = (w * w + h * h).sqrt();
      let font_size = fonts::scaled_font_size(diagonal, config.font_size, config.scale);
      let scale = Scale::uniform(font_size);

      // Parse RGBA color
      let color = parse_rgba(&config.color)?;

      let mut canvas = img.to_rgba8();

      match config.position {
          WatermarkPosition::Tile => {
              draw_tiled_watermark(&mut canvas, &font, &config.text, scale, color, config.rotation);
          }
          _ => {
              let (x, y) = calculate_position(
                  w as u32, h as u32,
                  &config.text, scale, &font,
                  &config.position,
                  config.margin_x, config.margin_y,
              );
              draw_single_watermark(&mut canvas, &font, &config.text, scale, color, x, y, config.rotation);
          }
      }

      Ok(DynamicImage::ImageRgba8(canvas))
  }

  fn parse_rgba(color_str: &str) -> Result<Rgba<u8>, String> {
      // Supports "rgba(r, g, b, a)" and "#RRGGBB" and "#RRGGBBAA"
      // ... implementation
  }

  fn calculate_position(w: u32, h: u32, text: &str, scale: Scale, font: &Font,
      pos: &WatermarkPosition, mx: u32, my: u32) -> (i32, i32) {
      // Measure text bounds, return (x, y) based on position enum
      // ...
  }

  fn draw_single_watermark(canvas: &mut RgbaImage, font: &Font, text: &str,
      scale: Scale, color: Rgba<u8>, x: i32, y: i32, rotation_deg: f64) {
      draw_text_mut(canvas, color, x, y, scale, font, text);
      // If rotation != 0, use imageproc::geometric::rotation to rotate
  }

  fn draw_tiled_watermark(canvas: &mut RgbaImage, font: &Font, text: &str,
      scale: Scale, color: Rgba<u8>, rotation_deg: f64) {
      // Calculate tile spacing (3x text width, 5x text height)
      // Loop over image, draw text at each tile position
      // ...
  }
  ```

- verify:
  - "cargo test -p pichost-worker -- test_watermark -- --exact"
  - "cargo clippy --workspace -- -D warnings"

---

### T6: Integrate watermark into Worker pipeline

- id: T6
- title: "Call apply_watermark() in process_task between read and variant generation"
- files:
  - Modify: `pichost-worker/src/pipeline.rs`
- depends_on: [T3, T5]
- breaking: false
- ac:
  - given: "Task with user who has watermark_config.enabled=true"
    when: "process_task() processes the image"
    then: "the watermark is applied to source image before thumbnail/WebP generation"
  - given: "Task with user who has watermark_config IS NULL"
    when: "process_task() processes the image"
    then: "no watermark is applied (pass-through)"
  - given: "Task with user who has watermark_config.enabled=false"
    when: "process_task() processes the image"
    then: "no watermark is applied"

- regression:
  - "cargo test -p pichost-worker -- --skip ignored"
  - "Existing thumbnail and WebP generation tests pass"

- test_code: |
  ```rust
  // In pichost-worker/src/pipeline.rs, extend existing test or add new one:

  #[cfg(test)]
  mod pipeline_tests {
      use super::*;

      #[test]
      fn test_process_task_with_watermark_disabled() {
          // Test that when watermark_config is None or enabled=false,
          // process_image_variants still works correctly (no-op path)
          // ...
      }

      #[test]
      fn test_pipeline_no_crash_with_null_config() {
          // Verify that a null watermark_config doesn't crash the pipeline
          let config: Option<WatermarkConfig> = None;
          // Should not affect flow
      }
  }
  ```

- impl_code: |
  ```rust
  // In pichost-worker/src/pipeline.rs, modify process_task():

  // After reading source image (around line 41):
  let (img, fmt, _bytes) = read_source_image(backend.as_ref(), task).await?;

  // Fetch watermark config from user record
  let watermark_config: Option<WatermarkConfig> = sqlx::query_scalar::<_, Option<serde_json::Value>>(
      "SELECT watermark_config FROM users WHERE id = $1"
  )
  .bind(task.user_id)
  .fetch_optional(pool)
  .await
  .map_err(|e| PipelineError::Database(format!("watermark config fetch: {}", e)))?
  .flatten()
  .and_then(|v| serde_json::from_value(v).ok());

  // Apply watermark if enabled
  let img = if let Some(ref wm_cfg) = watermark_config {
      if wm_cfg.enabled && !wm_cfg.text.is_empty() {
          crate::watermark::apply_watermark(&img, wm_cfg)
              .map_err(|e| PipelineError::Watermark(e))?
      } else {
          img
      }
  } else {
      img
  };

  // Continue with existing flow: extract dimensions, generate variants
  let (width, height) = (img.width() as i32, img.height() as i32);
  let (thumb_written, webp_written) = process_image_variants(
      &img, fmt, backend.as_ref(), &task.source_key, &config.worker.processing,
  )
  .await
  .map_err(|e| PipelineError::Pipeline(e))?;
  ```

  ```rust
  // Add Watermark variant to PipelineError enum (line 12):
  pub enum PipelineError {
      StorageRead(String),
      StorageWrite(String),
      Decode(String),
      Thumbnail(String),
      Webp(String),
      Watermark(String),     // NEW
      Database(String),
      Pipeline(String),
      BackendResolution(String),
  }
  ```

  ```rust
  // Add watermark module declaration to pichost-worker/src/main.rs or lib.rs:
  mod watermark;
  ```

- verify:
  - "cargo build -p pichost-worker"
  - "cargo test -p pichost-worker -- --skip ignored"
  - "cargo clippy --workspace -- -D warnings"

---

### T7: Frontend — API types and WatermarkSettings component

- id: T7
- title: "Add WatermarkConfig TypeScript types, API function, and WatermarkSettings component"
- files:
  - Modify: `web-ui/src/api/client.ts`
  - Create: `web-ui/src/components/WatermarkSettings.tsx`
- depends_on: [T2]
- breaking: false
- ac:
  - given: "WatermarkSettings component is rendered"
    when: "user toggles the 'Enable' checkbox"
    then: "watermark settings form fields become interactive"
  - given: "WatermarkSettings form is filled and saved"
    when: "user clicks Save"
    then: "PATCH /users/me is called with watermark_config and success toast is shown"
  - given: "WatermarkSettings component loads"
    when: "GET /users/me returns existing watermark_config"
    then: "form is pre-filled with existing values"
  - given: "WatermarkSettings with enabled=false"
    when: "component renders"
    then: "all watermark fields are greyed out/disabled except the enable toggle"
  - given: "User changes font, font_size, color, position, rotation, or scale"
    when: "form fields are modified"
    then: "each field updates local state independently"
  - given: "User clicks 'Clear Watermark'"
    when: "confirmation is accepted"
    then: "PATCH /users/me is called with watermark_config: null"

- regression:
  - "npm run build"
  - "npx tsc --noEmit"

- test_code: |
  N/A — no frontend test framework configured. Manual verification:
  - Navigate to Settings → Watermark section
  - Toggle enable/disable
  - Change text, font, color, position
  - Save → refresh → verify settings persist
  - Clear watermark → verify cleared

- impl_code: |
  ```typescript
  // In web-ui/src/api/client.ts — add types and update interfaces:

  export interface WatermarkConfig {
    enabled: boolean;
    text: string;
    font: string;
    font_size: number;
    color: string;
    rotation: number;
    scale: number;
    position: 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right' | 'center' | 'tile';
    margin_x: number;
    margin_y: number;
  }

  // Update UserProfile to include watermark_config
  // Update UpdateProfileRequest to include watermark_config?: WatermarkConfig | null

  export async function updateWatermarkConfig(
    config: WatermarkConfig | null
  ): Promise<UserProfile> {
    return api.patch('users/me', {
      json: { watermark_config: config },
    }).json<UserProfile>();
  }
  ```

  ```tsx
  // web-ui/src/components/WatermarkSettings.tsx
  import { useState, useEffect } from 'react';
  import { Button } from './ui/Button';
  import { Input } from './ui/Input';
  import type { WatermarkConfig, UserProfile } from '../api/client';

  const FONTS = [
    'NotoSansSC-Regular',
    'NotoSans-Regular',
    'Arial',
    'DejaVuSans',
    'FiraCode-Regular',
  ] as const;

  const POSITIONS = [
    { value: 'top-left', label: 'Top Left' },
    { value: 'top-right', label: 'Top Right' },
    { value: 'bottom-left', label: 'Bottom Left' },
    { value: 'bottom-right', label: 'Bottom Right' },
    { value: 'center', label: 'Center' },
    { value: 'tile', label: 'Tile' },
  ] as const;

  const DEFAULT_CONFIG: WatermarkConfig = {
    enabled: false,
    text: '',
    font: 'NotoSansSC-Regular',
    font_size: 48,
    color: 'rgba(255, 255, 255, 0.5)',
    rotation: -30,
    scale: 0.15,
    position: 'bottom-right',
    margin_x: 20,
    margin_y: 20,
  };

  interface Props {
    profile: UserProfile | null;
    onUpdate: (profile: UserProfile) => void;
  }

  export function WatermarkSettings({ profile, onUpdate }: Props) {
    const [config, setConfig] = useState<WatermarkConfig>(
      profile?.watermark_config ?? DEFAULT_CONFIG
    );
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Sync when profile changes
    useEffect(() => {
      setConfig(profile?.watermark_config ?? DEFAULT_CONFIG);
    }, [profile]);

    async function handleSave() {
      setSaving(true);
      setError(null);
      try {
        const updated = await updateWatermarkConfig(config);
        onUpdate(updated);
        // Show toast: "Watermark settings saved"
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Failed to save');
      } finally {
        setSaving(false);
      }
    }

    async function handleClear() {
      if (!confirm('Clear watermark configuration? This cannot be undone.')) return;
      setSaving(true);
      try {
        const updated = await updateWatermarkConfig(null);
        setConfig(DEFAULT_CONFIG);
        onUpdate(updated);
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Failed to clear');
      } finally {
        setSaving(false);
      }
    }

    return (
      <div className="rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm space-y-3">
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
          Default Watermark
        </h3>

        {/* Enable toggle */}
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={config.enabled}
            onChange={(e) => setConfig({ ...config, enabled: e.target.checked })}
            className="rounded"
          />
          <span className="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Enable watermark
          </span>
        </label>

        {/* Config fields — disabled when !enabled */}
        <fieldset disabled={!config.enabled} className="space-y-3">
          {/* Text input */}
          <Input
            label="Text"
            value={config.text}
            onChange={(e) => setConfig({ ...config, text: e.target.value })}
            placeholder="@username or custom text"
            maxLength={128}
          />

          {/* Font select */}
          <div className="space-y-1">
            <label className="text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Font
            </label>
            <select
              value={config.font}
              onChange={(e) => setConfig({ ...config, font: e.target.value })}
              className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-2 text-sm"
            >
              {FONTS.map((f) => (
                <option key={f} value={f}>{f}</option>
              ))}
            </select>
          </div>

          {/* Font size + Color inline */}
          <div className="flex gap-3">
            <div className="flex-1">
              <Input
                label="Font Size"
                type="number"
                min={8}
                max={200}
                value={config.font_size}
                onChange={(e) => setConfig({ ...config, font_size: Number(e.target.value) })}
              />
            </div>
            <div className="flex-1">
              <Input
                label="Color"
                type="text"
                value={config.color}
                onChange={(e) => setConfig({ ...config, color: e.target.value })}
                placeholder="rgba(255,255,255,0.5)"
              />
            </div>
          </div>

          {/* Rotation + Scale inline */}
          <div className="flex gap-3">
            <div className="flex-1">
              <Input
                label="Rotation (°)"
                type="number"
                min={-180}
                max={180}
                step={1}
                value={config.rotation}
                onChange={(e) => setConfig({ ...config, rotation: Number(e.target.value) })}
              />
            </div>
            <div className="flex-1">
              <Input
                label="Scale"
                type="number"
                min={0.01}
                max={1.0}
                step={0.01}
                value={config.scale}
                onChange={(e) => setConfig({ ...config, scale: Number(e.target.value) })}
              />
            </div>
          </div>

          {/* Position select */}
          <div className="space-y-1">
            <label className="text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Position
            </label>
            <select
              value={config.position}
              onChange={(e) => setConfig({ ...config, position: e.target.value as WatermarkConfig['position'] })}
              className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-2 text-sm"
            >
              {POSITIONS.map((p) => (
                <option key={p.value} value={p.value}>{p.label}</option>
              ))}
            </select>
          </div>

          {/* Margins inline */}
          <div className="flex gap-3">
            <div className="flex-1">
              <Input
                label="Margin X (px)"
                type="number"
                min={0}
                max={500}
                value={config.margin_x}
                onChange={(e) => setConfig({ ...config, margin_x: Number(e.target.value) })}
              />
            </div>
            <div className="flex-1">
              <Input
                label="Margin Y (px)"
                type="number"
                min={0}
                max={500}
                value={config.margin_y}
                onChange={(e) => setConfig({ ...config, margin_y: Number(e.target.value) })}
              />
            </div>
          </div>
        </fieldset>

        {error && (
          <p className="text-xs" style={{ color: 'var(--color-danger)' }}>{error}</p>
        )}

        <div className="flex gap-2 pt-2">
          <Button variant="primary" size="sm" onClick={handleSave} disabled={saving}>
            {saving ? 'Saving...' : 'Save Watermark'}
          </Button>
          <Button variant="ghost" size="sm" onClick={handleClear} disabled={saving}>
            Clear Watermark
          </Button>
        </div>
      </div>
    );
  }
  ```

- verify:
  - "npx tsc --noEmit"
  - "npm run build"

---

### T8: Integrate WatermarkSettings into Settings page

- id: T8
- title: "Wire WatermarkSettings component into Settings.tsx and pass profile data"
- files:
  - Modify: `web-ui/src/pages/Settings.tsx`
- depends_on: [T7]
- breaking: false
- ac:
  - given: "Settings page renders"
    when: "user scrolls to the Watermark section"
    then: "WatermarkSettings card is visible between Storage Configs and OAuth Accounts"
  - given: "WatermarkSettings is saved successfully"
    when: "component calls onUpdate with new profile"
    then: "Settings page local profile state is updated"
  - given: "Settings page loads"
    when: "GET /users/me returns profile with watermark_config"
    then: "WatermarkSettings receives the watermark_config from profile"

- regression:
  - "npx tsc --noEmit"
  - "npm run build"
  - "Existing Settings page sections (Profile, Password, Storage, OAuth) unchanged"

- test_code: |
  N/A — no frontend test framework. Manual verification:
  - Login → Settings → scroll to Watermark card
  - Configure watermark → Save → refresh page → verify persistence
  - Verify other Settings sections still work (Profile, Password, Storage Configs, OAuth)

- impl_code: |
  ```tsx
  // In web-ui/src/pages/Settings.tsx:
  // 1. Import WatermarkSettings
  import { WatermarkSettings } from '../components/WatermarkSettings';

  // 2. Add between StorageConfigSection (line 155) and OAuth (line 158):
  // After:
  //   <StorageConfigSection />
  // Add:
        <WatermarkSettings
          profile={profile}
          onUpdate={(updatedProfile) => setProfile(updatedProfile)}
        />
  ```

- verify:
  - "npx tsc --noEmit"
  - "npm run build"
  - "cargo clippy --workspace -- -D warnings"

---

## Agent Worker Instructions

### Required Sub-Skills
- `superpowers:test-driven-development` — TDD required for T1 (model tests), T5 (watermark rendering tests)
- `superpowers:subagent-driven-development` (preferred execution mode)
- `rust-refactor-fns` — ensure new functions stay ≤50 lines

### Recommended Execution Mode
`subagent-driven-development` — dispatch per-task subagents, review between tasks

### Required Verification
- `cargo test --workspace` — all tests pass (including new watermark tests)
- `cargo clippy --workspace -- -D warnings` — zero warnings
- `npm run build` — frontend builds clean

### Version Bump
0.16.0 → **0.16.1** (patch)

### Post-Phase Requirements
After all tasks complete AND verification passes:
- Update `AGENTS.md`: add migration 0010, new PATCH /users/me field, `watermark_config` JSONB column
- Update `README.md`: add watermark to Features checklist (conditionally), new migration count (10)
- Update `.omo/summary/summary_and_next.md`: mark P4-D as ✅ complete
- Bump version in `Cargo.toml` workspace files to 0.16.1
- Commit as: `docs: auto-sync AGENTS.md, README.md, summary after P4-D completion`

### Font File Acquisition
Manual step: Download the 5 TTF font files and place them in `pichost-worker/fonts/`:
- `NotoSansSC-Regular.ttf` — from Google Fonts (Noto Sans SC)
- `NotoSans-Regular.ttf` — from Google Fonts (Noto Sans)
- `Arial.ttf` — system font or Arial Unicode MS
- `DejaVuSans.ttf` — from DejaVu fonts
- `FiraCode-Regular.ttf` — from Google Fonts (Fira Code)

These are embedded via `include_bytes!()` and compiled into the binary.
