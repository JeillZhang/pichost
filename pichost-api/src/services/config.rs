use std::path::Path;

use pichost_core::config::AppConfig;

/// Runtime `config.toml` view. Every field is optional so a missing file
/// (or missing key) falls back to `pichost-core` figment defaults.
#[derive(Debug, Clone, Default)]
pub struct SystemConfig {
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
    pub jwt_secret: Option<String>,
    pub token_encryption_key: Option<String>,
    pub public_url: Option<String>,
    pub default_backend: Option<String>,
    pub local_base_path: Option<String>,
}

impl SystemConfig {
    /// Build a config.toml view from the runtime-effective config.
    /// Sensitive fields (`jwt_secret`, `token_encryption_key`) are intentionally omitted.
    pub fn from_effective(cfg: &AppConfig) -> Self {
        Self {
            database_url: Some(cfg.database.url.clone()),
            redis_url: Some(cfg.redis.url.clone()),
            jwt_secret: None,
            token_encryption_key: None,
            public_url: Some(cfg.server.public_url.clone()),
            default_backend: Some(cfg.storage.default_backend.clone()),
            local_base_path: Some(cfg.storage.local_base_path.display().to_string()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Connection error: {0}")]
    Connection(String),
}

/// Read config.toml matching figment's nested key structure.
/// Keys: database.url, redis.url, server.public_url,
///       storage.default_backend, storage.local_base_path, auth.jwt_secret.
pub fn read_config_toml(path: &Path) -> Result<SystemConfig, ConfigError> {
    if !path.exists() {
        return Ok(SystemConfig::default());
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::Io(e.to_string()))?;
    let doc: toml_edit::DocumentMut = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ConfigError::Parse(e.to_string()))?;

    fn get_str(doc: &toml_edit::DocumentMut, section: &str, key: &str) -> Option<String> {
        doc.get(section)
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    Ok(SystemConfig {
        database_url: get_str(&doc, "database", "url"),
        redis_url: get_str(&doc, "redis", "url"),
        jwt_secret: get_str(&doc, "auth", "jwt_secret"),
        token_encryption_key: None,
        public_url: get_str(&doc, "server", "public_url"),
        default_backend: get_str(&doc, "storage", "default_backend"),
        local_base_path: get_str(&doc, "storage", "local_base_path"),
    })
}

/// Write SystemConfig to config.toml using figment-compatible nested keys.
/// Preserves all existing sections/keys.
pub fn write_config_toml(path: &Path, config: &SystemConfig) -> Result<(), ConfigError> {
    let mut doc: toml_edit::DocumentMut = if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| ConfigError::Parse(e.to_string()))?
    } else {
        toml_edit::DocumentMut::new()
    };

    fn set_nested(
        doc: &mut toml_edit::DocumentMut,
        section: &str,
        key: &str,
        val: &Option<String>,
    ) {
        match val {
            Some(v) => {
                doc[section][key] = toml_edit::value(v.as_str());
            }
            None => {
                if let Some(table) = doc.get_mut(section) {
                    table.as_table_mut().map(|t| t.remove(key));
                }
            }
        }
    }

    set_nested(&mut doc, "database", "url", &config.database_url);
    set_nested(&mut doc, "redis", "url", &config.redis_url);
    set_nested(&mut doc, "server", "public_url", &config.public_url);
    set_nested(&mut doc, "storage", "default_backend", &config.default_backend);
    set_nested(&mut doc, "storage", "local_base_path", &config.local_base_path);

    std::fs::write(path, doc.to_string()).map_err(|e| ConfigError::Io(e.to_string()))
}

/// Backup current config.toml to config.toml.{timestamp}.bak
pub fn backup_config(path: &Path) -> Result<String, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::Io("config.toml not found".into()));
    }
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let backup_name = format!("config.toml.{}.bak", ts);
    let backup_path = path.parent().unwrap_or(Path::new(".")).join(&backup_name);
    std::fs::copy(path, &backup_path).map_err(|e| ConfigError::Io(e.to_string()))?;
    Ok(backup_name)
}

/// List all backup files (matches config.toml.*.bak), newest first.
pub fn list_backups(dir: &Path) -> Result<Vec<String>, ConfigError> {
    let mut backups = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| ConfigError::Io(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| ConfigError::Io(e.to_string()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("config.toml.") && name.ends_with(".bak") {
            backups.push(name);
        }
    }
    backups.sort_by(|a, b| b.cmp(a));
    Ok(backups)
}

/// Restore config.toml from a backup file.
pub fn restore_config(path: &Path, backup_file: &str) -> Result<(), ConfigError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let backup_path = dir.join(backup_file);
    if !backup_path.exists() {
        return Err(ConfigError::Io(format!("Backup not found: {}", backup_file)));
    }
    std::fs::copy(&backup_path, path).map_err(|e| ConfigError::Io(e.to_string()))?;
    Ok(())
}

/// Test database connection with 5s timeout (connect + SELECT 1).
pub async fn test_database_connection(url: &str) -> Result<(), ConfigError> {
    use std::time::Duration;
    use tokio::time::timeout;

    let result = timeout(Duration::from_secs(5), sqlx::PgPool::connect(url)).await;
    match result {
        Ok(Ok(pool)) => {
            sqlx::query("SELECT 1")
                .execute(&pool)
                .await
                .map_err(|e| ConfigError::Connection(e.to_string()))?;
            pool.close().await;
            Ok(())
        }
        Ok(Err(e)) => Err(ConfigError::Connection(e.to_string())),
        Err(_) => Err(ConfigError::Connection("timed out (5s)".into())),
    }
}

/// Test Redis connection via PING.
pub fn test_redis_connection(url: &str) -> Result<(), ConfigError> {
    let client = redis::Client::open(url).map_err(|e| ConfigError::Connection(e.to_string()))?;
    let mut conn = client
        .get_connection()
        .map_err(|e| ConfigError::Connection(e.to_string()))?;
    let result: String = redis::cmd("PING")
        .query(&mut conn)
        .map_err(|e| ConfigError::Connection(e.to_string()))?;
    if result == "PONG" {
        Ok(())
    } else {
        Err(ConfigError::Connection(format!("unexpected PING: {}", result)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_and_read_config_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = SystemConfig {
            database_url: Some("postgresql://test:test@localhost/test".into()),
            redis_url: Some("redis://localhost:6379".into()),
            jwt_secret: None,
            token_encryption_key: None,
            public_url: Some("https://pichost.example.com".into()),
            default_backend: Some("local".into()),
            local_base_path: Some("./test-storage".into()),
        };
        write_config_toml(&path, &config).unwrap();
        let read = read_config_toml(&path).unwrap();
        assert_eq!(read.database_url, config.database_url);
        assert_eq!(read.public_url, config.public_url);
        assert_eq!(read.default_backend, config.default_backend);
    }

    #[test]
    fn test_read_defaults_when_no_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = read_config_toml(&path).unwrap();
        assert_eq!(config.database_url, None);
        assert_eq!(config.public_url, None);
    }

    #[test]
    fn test_from_effective_omits_secrets_and_roundtrips() {
        let app = pichost_core::config::AppConfig::default();
        let view = SystemConfig::from_effective(&app);
        assert!(view.jwt_secret.is_none());
        assert!(view.token_encryption_key.is_none());
        assert_eq!(
            view.database_url.as_deref(),
            Some("postgres://pichost:pichost@localhost:5432/pichost")
        );
        assert_eq!(view.redis_url.as_deref(), Some("redis://localhost:6379"));

        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config_toml(&path, &view).unwrap();
        let read = read_config_toml(&path).unwrap();
        assert_eq!(read.database_url, view.database_url);
        assert_eq!(read.public_url, view.public_url);
        assert_eq!(read.default_backend, view.default_backend);
    }
}
