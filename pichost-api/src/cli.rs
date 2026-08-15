//! 最小 CLI 参数解析(Windows 服务命令;无参数 = 前台运行)

/// 支持的子命令
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    Run,
    InstallService,
    UninstallService,
    Service,
    Help,
}

pub const USAGE: &str =
    "Usage: pichost-api [--install-service|--uninstall-service|--service]";

/// 解析 CLI 参数;未知/多参数返回 Err(调用方打印 usage 后以退出码 2 终止)
pub fn parse_cli_args(args: &[String]) -> Result<CliCommand, &'static str> {
    match args {
        [] => Ok(CliCommand::Run),
        [flag] => match flag.as_str() {
            "--install-service" => Ok(CliCommand::InstallService),
            "--uninstall-service" => Ok(CliCommand::UninstallService),
            "--service" => Ok(CliCommand::Service),
            "-h" | "--help" => Ok(CliCommand::Help),
            _ => Err(USAGE),
        },
        _ => Err(USAGE),
    }
}
