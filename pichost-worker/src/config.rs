use pichost_core::config::{load_config, AppConfig};

pub fn load_worker_config() -> Result<AppConfig, figment::Error> {
    load_config()
}
