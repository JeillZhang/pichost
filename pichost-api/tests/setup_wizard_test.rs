use pichost_api::setup::admin::create_admin_flow;
use pichost_api::setup::prompts::{MockPrompts, MockReply};
use pichost_api::setup::{
    choose_language, decide_tty, maybe_run, run_wizard, should_run_wizard, TtyDecision,
};
use pichost_api::services::user_ops;
use pichost_core::config::AppConfig;
use pichost_core::db::{create_sqlite_pool, run_sqlite_migrations};
use pichost_core::i18n::Language;
use serial_test::serial;
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn sqlite_pool() -> SqlitePool {
    let pool = create_sqlite_pool("sqlite::memory:", 5).await.unwrap();
    run_sqlite_migrations(&pool).await.unwrap();
    pool
}

fn base_config() -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.database.url = "sqlite::memory:".into();
    cfg.i18n.language = "en".into();
    cfg
}

/// 两个 run_wizard 测试均写/删全局 env(PICHOST_ENV_FILE 等),必须串行,
/// 避免并发互踩(serial_test 为既有 dev-dependency,AGENTS.md 约定)。
#[test]
fn gate_pure_functions() {
    assert!(should_run_wizard(0, false));
    assert!(!should_run_wizard(3, false));
    assert!(should_run_wizard(3, true));
    assert!(matches!(decide_tty(false, false).unwrap(), TtyDecision::SkipWarn));
    assert!(decide_tty(true, false).is_err());
    assert!(matches!(decide_tty(true, true).unwrap(), TtyDecision::Run));
    assert_eq!(choose_language(&base_config()), Language::En);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn run_wizard_writes_env_and_creates_admin() {
    let pool = sqlite_pool().await;
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    std::env::set_var("PICHOST_ENV_FILE", env_path.to_str().unwrap());
    std::env::remove_var("PICHOST_AUTH__JWT_SECRET");
    std::env::remove_var("PICHOST_SERVER__PUBLIC_URL");
    let cfg = base_config();
    let mut prompts = MockPrompts::new(vec![
        MockReply::Input("https://img.example.com".into()),
        MockReply::Confirm(true),
        MockReply::Input("admin".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Input(String::new()),
    ]);
    let result = run_wizard(&pool, &cfg, Language::En, &mut prompts)
        .await
        .unwrap()
        .expect("wizard returns config");
    assert_eq!(result.server.public_url, "https://img.example.com");
    let content = std::fs::read_to_string(&env_path).unwrap();
    assert!(content.contains("PICHOST_SERVER__PUBLIC_URL=https://img.example.com"));
    assert!(content.contains("PICHOST_AUTH__JWT_SECRET="));
    let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = TRUE")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(admins, 1);
    let prefix: String = sqlx::query_scalar("SELECT storage_prefix FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(prefix.starts_with("users/"));
    std::env::remove_var("PICHOST_ENV_FILE");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn run_wizard_skips_admin_when_users_exist() {
    let pool = sqlite_pool().await;
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    std::env::set_var("PICHOST_ENV_FILE", env_path.to_str().unwrap());
    std::env::remove_var("PICHOST_AUTH__JWT_SECRET");
    std::env::remove_var("PICHOST_SERVER__PUBLIC_URL");
    let cfg = base_config();
    let hash = user_ops::hash_password("password123").unwrap();
    user_ops::insert_user(&pool, "existing", &None, &hash, false, None).await.unwrap();
    let mut prompts = MockPrompts::new(vec![MockReply::Input("https://img.example.com".into())]);
    let result = run_wizard(&pool, &cfg, Language::En, &mut prompts)
        .await
        .unwrap()
        .expect("wizard returns config");
    assert_eq!(result.server.public_url, "https://img.example.com");
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 1);
    std::env::remove_var("PICHOST_ENV_FILE");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_admin_flow_reprompts_on_conflict() {
    let pool = sqlite_pool().await;
    let hash = user_ops::hash_password("password123").unwrap();
    user_ops::insert_user(&pool, "taken", &None, &hash, false, None).await.unwrap();
    let cfg = base_config();
    let mut prompts = MockPrompts::new(vec![
        MockReply::Confirm(true),
        MockReply::Input("taken".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Input(String::new()),
        MockReply::Input("fresh".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Input(String::new()),
    ]);
    let created = create_admin_flow(&pool, &cfg, Language::En, &mut prompts).await.unwrap();
    assert!(created);
    let fresh: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = 'fresh'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fresh, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maybe_run_non_tty_first_run_skips() {
    let pool = sqlite_pool().await;
    let result = maybe_run(&pool, &base_config(), false).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maybe_run_forced_non_tty_errors() {
    let pool = sqlite_pool().await;
    let err = maybe_run(&pool, &base_config(), true).await.unwrap_err();
    assert!(err.to_string().contains("interactive terminal"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maybe_run_existing_users_short_circuits() {
    let pool = sqlite_pool().await;
    let hash = user_ops::hash_password("password123").unwrap();
    user_ops::insert_user(&pool, "someone", &None, &hash, false, None).await.unwrap();
    let result = maybe_run(&pool, &base_config(), false).await.unwrap();
    assert!(result.is_none());
}
