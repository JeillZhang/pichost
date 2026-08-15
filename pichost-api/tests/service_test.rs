// pichost-api/tests/service_test.rs — 新建(跨平台,仅测 env 引导逻辑)
use pichost_api::service::{ensure_service_env, env_has_valid_jwt};

#[test]
fn creates_env_when_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let env_path = tmp.path().join("PicHost").join(".env");
    let data_dir = tmp.path().join("PicHost").join("data");
    ensure_service_env(&env_path, &data_dir).unwrap();

    let content = std::fs::read_to_string(&env_path).unwrap();
    assert!(content.contains("PICHOST_DATABASE_MODE=sqlite"));
    let db_url = data_dir.join("pichost.db").to_string_lossy().replace('\\', "/");
    assert!(content.contains(&format!("sqlite://{db_url}")));
    assert!(content.contains("PICHOST_STORAGE__LOCAL_BASE_PATH="));
    assert!(env_has_valid_jwt(&content));
    assert!(data_dir.is_dir());
}

#[test]
fn appends_jwt_when_missing_or_short() {
    let tmp = tempfile::tempdir().unwrap();
    let env_path = tmp.path().join(".env");
    let data_dir = tmp.path().join("data");
    std::fs::write(&env_path, "PICHOST_DATABASE_MODE=sqlite\n").unwrap();
    ensure_service_env(&env_path, &data_dir).unwrap();
    let content = std::fs::read_to_string(&env_path).unwrap();
    assert!(env_has_valid_jwt(&content));
    assert!(content.starts_with("PICHOST_DATABASE_MODE=sqlite"));
}

#[test]
fn leaves_valid_env_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let env_path = tmp.path().join(".env");
    let data_dir = tmp.path().join("data");
    let original =
        "PICHOST_DATABASE_MODE=sqlite\nPICHOST_AUTH__JWT_SECRET=abcdef0123456789abcdef0123456789\n";
    std::fs::write(&env_path, original).unwrap();
    ensure_service_env(&env_path, &data_dir).unwrap();
    assert_eq!(std::fs::read_to_string(&env_path).unwrap(), original);
}
