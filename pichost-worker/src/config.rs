use pichost_core::config::{load_config, AppConfig};

#[allow(clippy::result_large_err)]
pub fn load_worker_config() -> Result<AppConfig, figment::Error> {
    load_config()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PichostEnvGuard {
        saved: Vec<(String, Option<String>)>,
    }

    impl PichostEnvGuard {
        fn capture() -> Self {
            let saved = std::env::vars()
                .filter(|(k, _)| k.starts_with("PICHOST_"))
                .map(|(k, v)| {
                    std::env::remove_var(&k);
                    (k, Some(v))
                })
                .collect();
            Self { saved }
        }
    }

    impl Drop for PichostEnvGuard {
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
    fn test_load_worker_config_defaults() {
        let _guard = PichostEnvGuard::capture();
        let cfg = load_worker_config().expect("default config loads");
        assert_eq!(cfg.worker.concurrency, 4);
        assert_eq!(cfg.worker.queue_poll_timeout, 5);
        assert_eq!(cfg.worker.task_timeout, 300);
        assert_eq!(cfg.worker.processing.thumbnail_size, 300);
        assert_eq!(cfg.worker.processing.webp_quality, 82.0);
    }
}
