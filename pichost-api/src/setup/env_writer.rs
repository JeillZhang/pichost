use rand::RngCore;
use std::io;
use std::path::{Path, PathBuf};

fn single_underscore(key: &str) -> String {
    key.replace("__", "_")
}

pub fn upsert_env(content: &str, updates: &[(&str, &str)]) -> String {
    let mut result = String::new();
    for line in content.lines() {
        let key = line.split('=').next().unwrap_or("");
        let replaced = updates
            .iter()
            .any(|(k, _)| key == *k || key == single_underscore(k));
        if !replaced {
            result.push_str(line);
            result.push('\n');
        }
    }
    for (k, v) in updates {
        result.push_str(&format!("{k}={v}\n"));
    }
    result
}

pub fn probe_env_path(
    explicit: Option<&Path>,
    system_dir: &Path,
    cwd: &Path,
) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    let system = system_dir.join(".env");
    if system.exists() {
        return Some(system);
    }
    let local = cwd.join(".env");
    if local.exists() {
        return Some(local);
    }
    None
}

pub fn validate_jwt_secret(secret: &str) -> bool {
    secret.len() >= 32
}

pub fn validate_public_url(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(u) => matches!(u.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

pub fn generate_jwt_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn apply_env_file(path: &Path, updates: &[(&str, &str)]) -> io::Result<()> {
    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let new_content = upsert_env(&content, updates);
    // 显式 .env.tmp 临时名(避免 with_extension 对点文件的非预期拼接)
    let tmp = path.with_extension("tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 先清陈旧 tmp(或恶意预置的符号链接本身,而非其目标),防 CWE-377/59
    let _ = std::fs::remove_file(&tmp);
    if let Err(e) = write_env_tmp(&tmp, new_content.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// 以 O_EXCL + 0600 权限创建临时文件并写入,避免先写后 chmod 的权限窗口。
#[cfg(unix)]
fn write_env_tmp(tmp: &Path, content: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(tmp)?;
    file.write_all(content)
}

#[cfg(not(unix))]
fn write_env_tmp(tmp: &Path, content: &[u8]) -> io::Result<()> {
    std::fs::write(tmp, content)
}
