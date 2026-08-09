//! Config unit tests: `DatabaseMode` default + `PICHOST_DATABASE_MODE` env parsing.
//!
//! Mirrors the `EnvGuard` snapshot/restore pattern already used inside
//! `config.rs`'s inline `mod tests` (see `test_load_config_env_override_and_restore`).

use crate::config::{AppConfig, DatabaseMode};
use serial_test::serial;

/// Snapshot/restore a single env var so tests never leak state to siblings.
struct EnvGuard {
    key: &'static str,
    old: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let old = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, old }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.old {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
fn database_mode_defaults_to_postgres() {
    let cfg = AppConfig::default();
    assert!(matches!(cfg.database.mode, DatabaseMode::Postgres));
}

#[serial]
#[test]
fn database_mode_parses_sqlite_from_env() {
    let _mode_guard = EnvGuard::set("PICHOST_DATABASE_MODE", "sqlite");
    let _url_guard = EnvGuard::set("PICHOST_DATABASE_URL", "sqlite:///tmp/test.db");
    let cfg = crate::config::load_config().unwrap();
    assert!(matches!(cfg.database.mode, DatabaseMode::Sqlite));
    assert_eq!(cfg.database.url, "sqlite:///tmp/test.db");
}
