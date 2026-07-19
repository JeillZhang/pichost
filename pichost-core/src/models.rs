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

fn default_font() -> String { "NotoSansSC-Regular".into() }
fn default_font_size() -> u32 { 48 }
fn default_color() -> String { "rgba(255, 255, 255, 0.5)".into() }
fn default_rotation() -> f64 { -30.0 }
fn default_scale() -> f64 { 0.15 }
fn default_margin() -> u32 { 20 }

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
        let req: UpdateProfileRequest =
            serde_json::from_str(r#"{"username": "bob"}"#).unwrap();
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
