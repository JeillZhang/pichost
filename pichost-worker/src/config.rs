use pichost_core::config::{load_config, AppConfig};

#[allow(clippy::result_large_err)]
pub fn load_worker_config() -> Result<AppConfig, figment::Error> {
    load_config()
}
