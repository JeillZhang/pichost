//! Windows 服务支持(cfg(windows))与跨平台服务 .env 引导
use std::path::{Path, PathBuf};

pub const SERVICE_NAME: &str = "PicHost";

/// 判定内容是否含有效 JWT(≥32 字符)
pub fn env_has_valid_jwt(content: &str) -> bool {
    content
        .lines()
        .filter(|l| l.starts_with("PICHOST_AUTH") && l.contains("JWT_SECRET="))
        .map(|l| l.split_once('=').map(|(_, v)| v.trim().trim_matches('"')).unwrap_or(""))
        .any(|v| v.len() >= 32)
}

fn generate_secret() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 服务数据目录 %ProgramData%\PicHost(ProgramData 缺失时回退 C:\ProgramData)
pub fn service_data_dir() -> PathBuf {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    PathBuf::from(base).join("PicHost")
}

/// 确保服务 .env 存在:缺失则生成(sqlite 默认);JWT 缺失/过短则补齐;有效则不动
pub fn ensure_service_env(env_path: &Path, data_dir: &Path) -> std::io::Result<()> {
    if let Some(parent) = env_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(data_dir)?;
    let mut content = std::fs::read_to_string(env_path).unwrap_or_default();
    if content.is_empty() {
        let db = data_dir.join("pichost.db").to_string_lossy().replace('\\', "/");
        let storage = data_dir.join("storage-local").to_string_lossy().replace('\\', "/");
        content = format!(
            "PICHOST_DATABASE_MODE=sqlite\nPICHOST_DATABASE_URL=\"sqlite://{db}\"\n\
             PICHOST_STORAGE__LOCAL_BASE_PATH=\"{storage}\"\n"
        );
    }
    if !env_has_valid_jwt(&content) {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("PICHOST_AUTH__JWT_SECRET={}\n", generate_secret()));
    }
    std::fs::write(env_path, content)
}

// ---- cfg(windows):服务注册/卸载/运行 ----

#[cfg(windows)]
/// 服务命令分发(T2 中 main.rs 调用)
pub async fn dispatch_cli(cmd: crate::cli::CliCommand) {
    match cmd {
        crate::cli::CliCommand::InstallService => {
            if let Err(e) = install_service() {
                eprintln!("error: install service failed: {e}");
                std::process::exit(1);
            }
            println!("PicHost service installed (name: {SERVICE_NAME})");
        }
        crate::cli::CliCommand::UninstallService => {
            if let Err(e) = uninstall_service() {
                eprintln!("error: uninstall service failed: {e}");
                std::process::exit(1);
            }
            println!("PicHost service uninstalled");
        }
        crate::cli::CliCommand::Service => {
            if let Err(e) = run_service() {
                eprintln!("error: service run failed: {e}");
                std::process::exit(1);
            }
        }
        _ => unreachable!("Run/Help handled in main"),
    }
}

#[cfg(windows)]
fn install_service() -> windows_service::Result<()> {
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    let exe = std::env::current_exe().expect("current exe");
    let bin = format!("\"{}\" --service", exe.display());
    let info = ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: "PicHost".into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        binary_path: bin.into(),
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let mgr = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    mgr.create_service(&info, ServiceAccess::CHANGE_CONFIG)?;
    Ok(())
}

#[cfg(windows)]
fn uninstall_service() -> windows_service::Result<()> {
    use windows_service::service::{ServiceAccess, ServiceState};
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    let mgr = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let svc = mgr.open_service(SERVICE_NAME, ServiceAccess::DELETE)?;
    if svc.query_status()?.current_state != ServiceState::Stopped {
        svc.stop()?;
    }
    svc.delete()?;
    Ok(())
}

#[cfg(windows)]
/// 若环境未显式配置 PICHOST_STATIC_DIR,则从可执行文件所在目录推导 dist(NSIS 安装布局)
fn ensure_static_dir() {
    if std::env::var_os("PICHOST_STATIC_DIR").is_some() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            std::env::set_var("PICHOST_STATIC_DIR", dir.join("dist"));
        }
    }
}

#[cfg(windows)]
fn run_service() -> windows_service::Result<()> {
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceState, ServiceType,
    };
    use windows_service::service_control_handler::{
        self, ServiceControlHandlerResult, ServiceStatus,
    };
    use windows_service::service_dispatcher;

    fn main_service() -> windows_service::Result<()> {
        let status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: 0,
            checkpoint: 0,
            wait_hint: 5000,
            process_id: None,
        };
        let handler = service_control_handler::register(SERVICE_NAME, |event| match event {
            ServiceControl::Stop => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
        handler.set_service_status(status)?;

        let base = service_data_dir();
        let env_path = base.join(".env");
        let data_dir = base.join("data");
        if let Err(e) = ensure_service_env(&env_path, &data_dir) {
            eprintln!("error: env bootstrap failed: {e}");
        }
        if env_path.exists() {
            for line in std::fs::read_to_string(&env_path).unwrap_or_default().lines() {
                if let Some((k, v)) = line.split_once('=') {
                    std::env::set_var(k.trim(), v.trim().trim_matches('"'));
                }
            }
        }
        ensure_static_dir();
        handler.set_service_status(ServiceStatus {
            current_state: ServiceState::Running,
            ..status
        })?;

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(crate::run_lite_from_env());
        Ok(())
    }
    service_dispatcher::start(SERVICE_NAME, main_service)
}
