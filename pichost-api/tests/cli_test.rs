// pichost-api/tests/cli_test.rs — CLI 参数解析单元测试(无 DB 依赖)
use pichost_api::cli::{parse_cli_args, CliCommand};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn no_args_is_run() {
    assert_eq!(parse_cli_args(&args(&[])), Ok(CliCommand::Run));
}

#[test]
fn service_flags_parse() {
    assert_eq!(
        parse_cli_args(&args(&["--install-service"])),
        Ok(CliCommand::InstallService)
    );
    assert_eq!(
        parse_cli_args(&args(&["--uninstall-service"])),
        Ok(CliCommand::UninstallService)
    );
    assert_eq!(parse_cli_args(&args(&["--service"])), Ok(CliCommand::Service));
    assert_eq!(parse_cli_args(&args(&["-h"])), Ok(CliCommand::Help));
    assert_eq!(parse_cli_args(&args(&["--help"])), Ok(CliCommand::Help));
}

#[test]
fn parses_setup_flag() {
    let args: Vec<String> = vec!["--setup".into()];
    assert_eq!(parse_cli_args(&args), Ok(CliCommand::Setup));
}

#[test]
fn unknown_or_multi_args_error() {
    assert!(parse_cli_args(&args(&["--bogus"])).is_err());
    assert!(parse_cli_args(&args(&["--service", "extra"])).is_err());
}
