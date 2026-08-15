use pichost_api::setup::env_writer::{
    apply_env_file, generate_jwt_secret, probe_env_path, upsert_env, validate_jwt_secret,
    validate_public_url,
};
use std::path::Path;

#[test]
fn upsert_removes_both_underscore_variants_and_appends_canonical() {
    let content = "# comment\nPICHOST_AUTH_JWT_SECRET=old\nKEEP=yes\n";
    let out = upsert_env(
        content,
        &[("PICHOST_AUTH__JWT_SECRET", "abcdef0123456789abcdef0123456789")],
    );
    assert!(out.contains("# comment"));
    assert!(out.contains("KEEP=yes"));
    assert!(!out.contains("PICHOST_AUTH_JWT_SECRET=old"));
    assert!(!out.contains("PICHOST_AUTH_JWT_SECRET=abcdef"));
    assert!(out.contains("PICHOST_AUTH__JWT_SECRET=abcdef0123456789abcdef0123456789"));
}

#[test]
fn probe_env_path_prefers_explicit_override() {
    let p = probe_env_path(
        Some(Path::new("/tmp/x.env")),
        Path::new("/nonexistent-dir"),
        Path::new("/nonexistent-cwd"),
    );
    assert_eq!(p, Some(std::path::PathBuf::from("/tmp/x.env")));
}

#[test]
fn probe_env_path_returns_none_when_nothing_exists() {
    let p = probe_env_path(None, Path::new("/nonexistent-dir"), Path::new("/nonexistent-cwd"));
    assert!(p.is_none());
}

#[test]
fn apply_env_file_creates_atomic_file_with_600_perms() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(".env");
    apply_env_file(
        &path,
        &[("PICHOST_AUTH__JWT_SECRET", "abcdef0123456789abcdef0123456789")],
    )
    .unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("PICHOST_AUTH__JWT_SECRET=abcdef0123456789abcdef0123456789"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn validation_rules() {
    assert!(validate_jwt_secret("12345678901234567890123456789012"));
    assert!(!validate_jwt_secret("short"));
    assert!(validate_public_url("https://img.example.com"));
    assert!(!validate_public_url("ftp://img.example.com"));
    assert!(generate_jwt_secret().len() >= 64);
}
