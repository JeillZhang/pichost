use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub storage_backend: String,
    pub storage_prefix: String,
    pub storage_quota: Option<i64>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub watermark_config: Option<WatermarkConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub user_id: Uuid,
    pub public_key: String,
    pub original_name: String,
    pub storage_key: String,
    pub storage_backend: String,
    pub mime_type: String,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sha256: String,
    pub url: String,
    pub thumbnail_key: Option<String>,
    pub thumbnail_url: Option<String>,
    pub webp_key: Option<String>,
    pub webp_url: Option<String>,
    pub status: ImageStatus,
    pub storage_config_id: Option<Uuid>,
    #[serde(default)]
    pub category_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageStatus {
    Pending,
    Active,
    Processing,
    Ready,
    Failed,
}

impl std::fmt::Display for ImageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Processing => write!(f, "processing"),
            Self::Ready => write!(f, "ready"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum WatermarkPosition {
    #[serde(alias = "top-left", alias = "top_left")]
    TopLeft,
    #[serde(alias = "top-right", alias = "top_right")]
    TopRight,
    #[serde(alias = "bottom-left", alias = "bottom_left")]
    BottomLeft,
    #[default]
    #[serde(alias = "bottom-right", alias = "bottom_right")]
    BottomRight,
    Center,
    Tile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatermarkConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
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

fn default_font() -> String {
    "NotoSansSC-Regular".into()
}
fn default_font_size() -> u32 {
    48
}
fn default_color() -> String {
    "rgba(255, 255, 255, 0.5)".into()
}
fn default_rotation() -> f64 {
    -30.0
}
fn default_scale() -> f64 {
    0.15
}
fn default_margin() -> u32 {
    20
}

/// 用户的存储后端配置
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserStorageConfig {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub provider: String,
    pub is_default: bool,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Git 后端 config JSON 的反序列化结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfigDetail {
    pub token_encrypted: String,
    pub repo: String,
    pub branch: String,
    pub path_prefix: Option<String>,
}

/// A user-created image category, supporting up to 2 levels of nesting.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Category {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// API 响应用于掩码 token 的配置视图
#[derive(Debug, Clone, Serialize)]
pub struct UserStorageConfigResponse {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
    pub repo: String,
    pub branch: String,
    pub path_prefix: Option<String>,
    pub is_default: bool,
    pub token_masked: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadTask {
    pub id: Uuid,
    pub image_id: Uuid,
    pub task_type: String,
    pub payload: Option<serde_json::Value>,
    pub status: String,
    pub error: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Response for GET /users/me — full user profile
#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub storage_backend: String,
    pub storage_prefix: String,
    pub storage_quota: Option<i64>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub watermark_config: Option<WatermarkConfig>,
}

/// Request body for PATCH /users/me
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub storage_backend: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_jsonb",
        skip_serializing_if = "Option::is_none"
    )]
    pub watermark_config: Option<Option<WatermarkConfig>>,
}

fn deserialize_optional_jsonb<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

/// Request body for POST /users/me/password
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Payload for async image processing tasks (worker queue).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskPayload {
    pub task_id: uuid::Uuid,
    pub image_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub storage_backend: String,
    pub storage_config_id: Option<uuid::Uuid>,
    pub storage_backend_name: String,
    pub source_key: String,
    pub source_mime: String,
    pub retry_count: i32,
    pub max_retries: i32,
}

#[cfg(test)]
mod watermark_tests {
    use super::*;

    #[test]
    fn test_watermark_config_deserialize_full() {
        let json = r#"{
            "enabled": true, "text": "@testuser", "font": "NotoSansSC-Regular",
            "font_size": 48, "color": "rgba(255, 255, 255, 0.5)",
            "rotation": -30.0, "scale": 0.15, "position": "bottom-right",
            "margin_x": 20, "margin_y": 20
        }"#;
        let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.text, "@testuser");
        assert_eq!(cfg.position, WatermarkPosition::BottomRight);
    }

    #[test]
    fn test_watermark_config_defaults_for_partial() {
        let json = r#"{"enabled": true, "text": "hello"}"#;
        let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.font, "NotoSansSC-Regular");
        assert_eq!(cfg.font_size, 48);
        assert_eq!(cfg.position, WatermarkPosition::BottomRight);
        assert!((cfg.rotation - (-30.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_watermark_config_disabled() {
        let json = r#"{"enabled": false, "text": ""}"#;
        let cfg: WatermarkConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.enabled);
    }

    #[test]
    fn test_watermark_position_serde() {
        let cfg: WatermarkConfig =
            serde_json::from_str(r#"{"enabled":true,"text":"x","position":"tile"}"#).unwrap();
        assert_eq!(cfg.position, WatermarkPosition::Tile);
        let cfg: WatermarkConfig =
            serde_json::from_str(r#"{"enabled":true,"text":"x","position":"center"}"#).unwrap();
        assert_eq!(cfg.position, WatermarkPosition::Center);
        let cfg: WatermarkConfig =
            serde_json::from_str(r#"{"enabled":true,"text":"x","position":"top-left"}"#).unwrap();
        assert_eq!(cfg.position, WatermarkPosition::TopLeft);
        // Test snake_case alias
        let cfg: WatermarkConfig =
            serde_json::from_str(r#"{"enabled":true,"text":"x","position":"top_left"}"#).unwrap();
        assert_eq!(cfg.position, WatermarkPosition::TopLeft);
    }

    #[test]
    fn test_update_profile_request_watermark_absent() {
        let req: UpdateProfileRequest = serde_json::from_str(r#"{"username": "bob"}"#).unwrap();
        assert_eq!(req.username, Some("bob".to_string()));
        assert_eq!(req.watermark_config, None); // absent → don't touch
    }

    #[test]
    fn test_update_profile_request_watermark_null_means_clear() {
        let req: UpdateProfileRequest =
            serde_json::from_str(r#"{"watermark_config": null}"#).unwrap();
        assert_eq!(req.watermark_config, Some(None)); // explicit null → clear
    }

    #[test]
    fn test_update_profile_request_watermark_set() {
        let req: UpdateProfileRequest =
            serde_json::from_str(r#"{"watermark_config": {"enabled": true, "text": "x"}}"#)
                .unwrap();
        assert!(req.watermark_config.is_some());
        let inner = req.watermark_config.unwrap();
        assert!(inner.is_some());
        assert!(inner.unwrap().enabled);
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;
    use chrono::Utc;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn image_status_display() {
        assert_eq!(ImageStatus::Pending.to_string(), "pending");
        assert_eq!(ImageStatus::Active.to_string(), "active");
        assert_eq!(ImageStatus::Processing.to_string(), "processing");
        assert_eq!(ImageStatus::Ready.to_string(), "ready");
        assert_eq!(ImageStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn image_status_serde_roundtrip() {
        for (status, name) in [
            (ImageStatus::Pending, "pending"),
            (ImageStatus::Active, "active"),
            (ImageStatus::Processing, "processing"),
            (ImageStatus::Ready, "ready"),
            (ImageStatus::Failed, "failed"),
        ] {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", name));
            let back: ImageStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
    }

    fn sample_user() -> User {
        User {
            id: Uuid::new_v4(),
            username: "alice".into(),
            email: Some("alice@example.com".into()),
            password_hash: "hash".into(),
            storage_backend: "local".into(),
            storage_prefix: "pfx".into(),
            storage_quota: Some(1_000_000),
            is_admin: true,
            created_at: now(),
            updated_at: now(),
            watermark_config: None,
        }
    }

    #[test]
    fn user_serde_roundtrip() {
        let user = sample_user();
        let json = serde_json::to_string(&user).unwrap();
        let back: User = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, user.id);
        assert_eq!(back.username, "alice");
        assert_eq!(back.email, user.email);
        assert_eq!(back.storage_quota, Some(1_000_000));
        assert!(back.is_admin);
    }

    fn sample_image(category_id: Option<Uuid>) -> Image {
        Image {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            public_key: "abc123".into(),
            original_name: "photo.png".into(),
            storage_key: "u/1/2026/01/01/a.png".into(),
            storage_backend: "local".into(),
            mime_type: "image/png".into(),
            file_size: 1024,
            width: Some(100),
            height: Some(50),
            sha256: "deadbeef".into(),
            url: "http://x/u/abc123".into(),
            thumbnail_key: None,
            thumbnail_url: None,
            webp_key: None,
            webp_url: None,
            status: ImageStatus::Active,
            storage_config_id: None,
            category_id,
            created_at: now(),
        }
    }

    #[test]
    fn image_serde_roundtrip_with_category() {
        let cat = Uuid::new_v4();
        let img = sample_image(Some(cat));
        let json = serde_json::to_string(&img).unwrap();
        let back: Image = serde_json::from_str(&json).unwrap();
        assert_eq!(back.category_id, Some(cat));
        assert_eq!(back.status, ImageStatus::Active);
    }

    #[test]
    fn image_serde_roundtrip_without_category() {
        let img = sample_image(None);
        let json = serde_json::to_string(&img).unwrap();
        let back: Image = serde_json::from_str(&json).unwrap();
        assert_eq!(back.category_id, None);
    }

    #[test]
    fn user_storage_config_serde_roundtrip() {
        let cfg = UserStorageConfig {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "my git".into(),
            provider: "github".into(),
            is_default: true,
            config: serde_json::json!({"repo": "a/b", "branch": "main"}),
            created_at: now(),
            updated_at: now(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: UserStorageConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "my git");
        assert_eq!(back.provider, "github");
        assert!(back.is_default);
        assert_eq!(back.config["branch"], "main");
    }

    #[test]
    fn git_config_detail_deserialize() {
        let detail: GitConfigDetail = serde_json::from_str(
            r#"{"token_encrypted":"enc","repo":"owner/repo","branch":"main","path_prefix":"pic"}"#,
        )
        .unwrap();
        assert_eq!(detail.token_encrypted, "enc");
        assert_eq!(detail.repo, "owner/repo");
        assert_eq!(detail.branch, "main");
        assert_eq!(detail.path_prefix, Some("pic".into()));

        let minimal: GitConfigDetail =
            serde_json::from_str(r#"{"token_encrypted":"e","repo":"o/r","branch":"b"}"#).unwrap();
        assert!(minimal.path_prefix.is_none());
    }

    #[test]
    fn category_serde_roundtrip() {
        let cat = Category {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "travel".into(),
            parent_id: None,
            created_at: now(),
        };
        let json = serde_json::to_string(&cat).unwrap();
        let back: Category = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "travel");
        assert_eq!(back.parent_id, None);
    }

    #[test]
    fn user_storage_config_response_serialize() {
        let resp = UserStorageConfigResponse {
            id: Uuid::new_v4(),
            name: "git".into(),
            provider: "github".into(),
            repo: "owner/repo".into(),
            branch: "main".into(),
            path_prefix: None,
            is_default: false,
            token_masked: "ghp_****5678".into(),
            created_at: now(),
            updated_at: now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["token_masked"], "ghp_****5678");
        assert_eq!(v["provider"], "github");
        assert!(v["path_prefix"].is_null());
    }

    #[test]
    fn upload_task_serde_roundtrip() {
        let task = UploadTask {
            id: Uuid::new_v4(),
            image_id: Uuid::new_v4(),
            task_type: "thumbnail".into(),
            payload: Some(serde_json::json!({"size": 300})),
            status: "pending".into(),
            error: None,
            retry_count: 0,
            max_retries: 3,
            created_at: now(),
            completed_at: None,
        };
        let json = serde_json::to_string(&task).unwrap();
        let back: UploadTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_type, "thumbnail");
        assert_eq!(back.payload.unwrap()["size"], 300);
        assert_eq!(back.retry_count, 0);
    }

    #[test]
    fn change_password_request_deserialize() {
        let req: ChangePasswordRequest =
            serde_json::from_str(r#"{"current_password":"old","new_password":"new"}"#).unwrap();
        assert_eq!(req.current_password, "old");
        assert_eq!(req.new_password, "new");
    }
}
