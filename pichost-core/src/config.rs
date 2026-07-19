use figment::{Figment, providers::{Env, Format, Serialized, Toml}};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub upload: UploadConfig,
    pub logging: LoggingConfig,
    pub worker: WorkerConfig,
    /// AES-256-GCM 密钥，用于加密用户 Git PAT
    /// 须 32 字节（base64 或 hex 编码），与 JWT secret 独立
    #[serde(default)]
    pub token_encryption_key: Option<String>,
    /// 每用户最多可创建的存储配置数。（None = 默认 5）
    #[serde(default)]
    pub storage_max_user_configs: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub public_url: String,
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub access_token_ttl: u64,
    pub refresh_token_ttl: u64,
    #[serde(default)]
    pub oauth_github_client_id: Option<String>,
    #[serde(default)]
    pub oauth_github_client_secret: Option<String>,
    #[serde(default)]
    pub oauth_google_client_id: Option<String>,
    #[serde(default)]
    pub oauth_google_client_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub default_backend: String,
    pub local_base_path: PathBuf,
    #[serde(default)]
    pub rustfs: Option<RustfsStorageConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RustfsStorageConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    #[serde(default = "default_rustfs_region")]
    pub region: String,
    #[serde(default)]
    pub use_ssl: bool,
    #[serde(default)]
    pub public_endpoint: Option<String>,
}

fn default_rustfs_region() -> String {
    "us-east-1".to_string()
}

fn default_storage_quota() -> u64 {
    1_073_741_824 // 1 GB
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UploadConfig {
    pub max_file_size_admin: u64,
    pub max_file_size_user: u64,
    pub allowed_mimes: Vec<String>,
    #[serde(default = "default_storage_quota")]
    pub storage_quota_default: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerProcessingConfig {
    pub thumbnail_size: u32,
    pub thumbnail_quality: u8,
    pub webp_quality: f32,
    pub compress_threshold_kb: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerConfig {
    pub concurrency: usize,
    pub queue_poll_timeout: u64,
    pub task_timeout: u64,
    pub recovery_scan_interval: u64,
    #[serde(default)]
    pub processing: WorkerProcessingConfig,
}

impl Default for WorkerProcessingConfig {
    fn default() -> Self {
        Self {
            thumbnail_size: 300,
            thumbnail_quality: 85,
            webp_quality: 82.0,
            compress_threshold_kb: 500,
        }
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            queue_poll_timeout: 5,
            task_timeout: 300,
            recovery_scan_interval: 60,
            processing: WorkerProcessingConfig::default(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 3000,
                public_url: "http://localhost:3000".into(),
                cors_origins: vec!["http://localhost:5173".into()],
            },
            auth: AuthConfig {
                jwt_secret: String::new(),
                access_token_ttl: 900,
                refresh_token_ttl: 2_592_000,
                oauth_github_client_id: None,
                oauth_github_client_secret: None,
                oauth_google_client_id: None,
                oauth_google_client_secret: None,
            },
            storage: StorageConfig {
                default_backend: "local".into(),
                local_base_path: PathBuf::from("./storage-local"),
                rustfs: None,
            },
            database: DatabaseConfig {
                url: "postgres://pichost:pichost@localhost:5432/pichost".into(),
                max_connections: 10,
            },
            redis: RedisConfig { url: "redis://localhost:6379".into(), pool_size: 20 },
            upload: UploadConfig {
                max_file_size_admin: 52_428_800,
                max_file_size_user: 10_485_760,
                allowed_mimes: vec![
                    "image/png".into(),
                    "image/jpeg".into(),
                    "image/gif".into(),
                    "image/webp".into(),
                    "image/svg+xml".into(),
                    "image/avif".into(),
                    "image/bmp".into(),
                ],
                storage_quota_default: 1_073_741_824,
            },
            logging: LoggingConfig { level: "info".into(), format: "json".into() },
            worker: WorkerConfig::default(),
            token_encryption_key: None,
            storage_max_user_configs: None,
        }
    }
}

#[allow(clippy::result_large_err)]
pub fn load_config() -> Result<AppConfig, figment::Error> {
    let figment = Figment::new()
        .merge(Serialized::defaults(AppConfig::default()))
        .merge(Toml::file("config.toml").nested())
        .merge(Env::prefixed("PICHOST_").split("_"));

    figment.extract()
}
