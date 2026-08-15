// pichost-core/tests/static_config_test.rs — 新建
use pichost_core::config::load_config;
use serial_test::serial;
use std::path::Path;

/// 快照/恢复全部 PICHOST_* 环境变量,避免并行测试互扰
struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
}
impl EnvGuard {
    fn new() -> Self {
        let saved: Vec<(String, Option<String>)> = std::env::vars()
            .filter(|(k, _)| k.starts_with("PICHOST_"))
            .map(|(k, v)| (k, Some(v)))
            .collect();
        for (k, _) in &saved {
            std::env::remove_var(k);
        }
        Self { saved }
    }
}
impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

#[test]
#[serial]
fn static_dir_unset_defaults_none() {
    let _g = EnvGuard::new();
    let cfg = load_config().expect("default config loads");
    assert!(cfg.static_dir.is_none(), "static_dir should default to None");
}

#[test]
#[serial]
fn static_dir_parses_env() {
    let _g = EnvGuard::new();
    std::env::set_var("PICHOST_STATIC_DIR", "/opt/pichost/dist");
    let cfg = load_config().expect("config loads with static_dir");
    assert_eq!(cfg.static_dir.as_deref(), Some(Path::new("/opt/pichost/dist")));
}
