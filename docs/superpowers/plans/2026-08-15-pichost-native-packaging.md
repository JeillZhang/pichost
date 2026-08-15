# 原生安装包与软件仓库分发实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 PicHost 新增 deb/rpm/NSIS-exe/Homebrew 原生安装包,构建自托管 apt+rpm 软件仓库(pichost-repo GitHub Pages)并接入 Homebrew tap 与 winget,实现四平台"一行命令下载→安装→部署→启动"。

**Architecture:** ① API 内嵌 SPA 静态服务(`PICHOST_STATIC_DIR`,默认 `./dist`,目录缺失跳过)使所有安装包无 Nginx 即开即用;② Windows 走 `windows-service` crate 原生服务 + NSIS 安装器;③ macOS 走 brew formula(预编译 universal2 tarball + launchd service block);④ CI 矩阵扩展为 linux(amd64/arm64 zigbuild)/macOS(universal2)/Windows(x64),发布 job 组装签名 apt/rpm 仓库推送到 `pichost-repo` Pages 并更新 homebrew tap,winget 独立 workflow 自动 PR;⑤ deb/rpm 走 FHS 布局(/usr/bin、/usr/share/pichost、/var/lib/pichost、/etc/pichost),与 install.sh 单目录契约分叉。设计文档:`docs/superpowers/specs/2026-08-15-pichost-native-packaging-design.md`。

**Tech Stack:** Rust(axum + tower-http `fs` + `windows-service`)、cargo-deb、cargo-generate-rpm、cargo-zigbuild、NSIS(makensis)、bash(packaging/ + scripts/)、GitHub Actions(矩阵 + Pages + wingetcreate)。

## Agent Worker Instructions

- **Required sub-skills**: `superpowers:subagent-driven-development`(推荐)或 `superpowers:executing-plans`;bash 脚本调试;本计划涉及 Rust 单测、shell 测试脚本、CI workflow 三类交付物,任务内已分别给出 TDD 顺序(test_code 先行)
- **Execution mode**: `subagent-driven-development`(每任务新 subagent + 两段式评审)
- **Required verification**: `cargo test --workspace`(406+ pass)、`cargo clippy --workspace -- -D warnings`、`npm run build`、`bash scripts/tests/*.sh`(本计划新增/修改的脚本测试)
- **Version bump reminder**: 0.22.0 → **0.23.0**(feature,minor)— Cargo.toml workspace + web-ui/package.json + Cargo.lock + package-lock.json 对齐;提交信息英文语义化(`feat:`/`chore:`/`docs:`)
- **Rust 规范**: 函数 ≤50 行、行宽 ≤120 字符;tower-http 特性在 workspace Cargo.toml 统一声明;`windows-service` 必须 `[target.'cfg(windows)'.dependencies]` 条件编译,Linux 构建零影响

## Global Constraints

- `PICHOST_STATIC_DIR`(AppConfig 顶层 `Option<PathBuf>`,serde default None):figment env 自动映射,与 `PICHOST_TOKEN_ENCRYPTION_KEY` 同机制;默认回退 `./dist`
- 静态服务挂载:显式路由(`/api/v1/*`、`/u`、`/t`、`/metrics`、`/health`)优先级不变;目录不存在 → warn + 跳过(不挂 fallback)
- Windows 服务名 `PicHost`;`--install-service`/`--uninstall-service`/`--service` 三命令 + 无参前台运行 + `-h/--help` 打印 usage 退出码 0
- 服务模式 .env:`%ProgramData%\PicHost\.env`,缺失/无有效 JWT(≥32 字符)时自动生成并写入;数据 `%ProgramData%\PicHost\data`
- deb/rpm 共享安装逻辑在 `packaging/common/install-lib.sh`(随包安装至 `/usr/share/pichost/`),postinst/%post source 复用;`.env` 已存在不覆盖(幂等)
- FHS 路径(包内):`/usr/bin/pichost-api`、`/usr/bin/pichost-worker`、`/usr/share/pichost/web-ui`、`/usr/share/pichost/migrations{,-sqlite}`、`/lib/systemd/system/pichost-api.service`(rpm 用 `/usr/lib/systemd/system/`)、`/etc/pichost/.env`、`/var/lib/pichost/{pichost.db,storage-local}`
- 脚本统一 `set -euo pipefail`(maintainer 脚本 `set -e`);提交前 `bash -n` 语法检查
- 版本 0.23.0:Cargo.toml + Cargo.lock + web-ui/package.json + package-lock.json 对齐;CHANGELOG Keep a Changelog
- 不做:Docker compose 改动、应用层默认模式翻转、macOS dmg/公证、Windows 代码签名、PPA/COPR/homebrew-core、Windows arm64

## 任务依赖图

```
T0 (static_dir 配置+fs 特性) ──→ T1 (SPA fallback 挂载)
T2 (CLI 解析) ──→ T3 (Windows 服务)
T4 (deb 共享 lib+postinst) ──→ T5 (deb prerm/postrm) ──→ T6 (cargo-deb 元数据)
T6 ──→ T7 (rpm 元数据+%post) ──→ T8 (rpm preun/postun)
T6/T7/T8 ──→ T9 (CI linux 矩阵) ──→ T10 (macOS universal2+formula)
T3/T9 ──→ T11 (NSIS 安装器)
T9 ──→ T12 (publish-repo.sh + setup-repo.sh)
T10/T12 ──→ T13 (CI publish-repo + tap job)
T11 ──→ T14 (winget workflow)
T15 (版本 0.23.0,独立)
T6/T15 ──→ T16 (verify-release.sh deb 冒烟)
T15/T16 ──→ T17 (README/AGENTS 同步)
T9/T17 ──→ T18 (CHANGELOG/summary)
```

---

```yaml
- id: T0
  title: "Add PICHOST_STATIC_DIR config field and tower-http fs feature"
  files: [Cargo.toml, pichost-core/src/config.rs, pichost-core/tests/static_config_test.rs]
  depends_on: []
  breaking: false
  ac:
    - given: "环境变量 PICHOST_STATIC_DIR=/opt/pichost/dist 已设置且其余 PICHOST_* 清除"
      when: "调用 load_config()"
      then: "返回的 AppConfig.static_dir == Some(PathBuf::from(\"/opt/pichost/dist\"))"
    - given: "全部 PICHOST_* 环境变量清除"
      when: "调用 load_config()"
      then: "AppConfig.static_dir == None"
    - given: "workspace Cargo.toml 的 tower-http 依赖"
      when: "执行 cargo check -p pichost-api"
      then: "编译通过且 tower-http features 含 \"fs\"(无未启用特性错误)"
  regression:
    - "cargo test -p pichost-core"
    - "cargo test -p pichost-api"
  test_code: |
    // pichost-core/tests/static_config_test.rs — 新建
    use pichost_core::config::load_config;
    use std::path::Path;

    /// 快照/恢复全部 PICHOST_* 环境变量,避免并行测试互扰
    struct EnvGuard {
        saved: Vec<(String, Option<String>)>,
    }
    impl EnvGuard {
        fn new() -> Self {
            let saved: Vec<(String, Option<String>)> = std::env::vars()
                .filter(|(k, _)| k.starts_with("PICHOST_"))
                .map(|(k, v)| (k, Some(v)))
                .collect();
            for (k, _) in &saved {
                std::env::remove_var(k);
            }
            Self { saved }
        }
    }
    impl Drop for EnvGuard {
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
    fn static_dir_unset_defaults_none() {
        let _g = EnvGuard::new();
        let cfg = load_config().expect("default config loads");
        assert!(cfg.static_dir.is_none(), "static_dir should default to None");
    }

    #[test]
    fn static_dir_parses_env() {
        let _g = EnvGuard::new();
        std::env::set_var("PICHOST_STATIC_DIR", "/opt/pichost/dist");
        let cfg = load_config().expect("config loads with static_dir");
        assert_eq!(cfg.static_dir.as_deref(), Some(Path::new("/opt/pichost/dist")));
    }
  impl_code: |
    // Cargo.toml(workspace, 第 30 行):tower-http features 追加 "fs"
    // tower-http = { version = "0.6", features = ["cors", "trace", "set-header", "fs"] }

    // pichost-core/src/config.rs — AppConfig 顶层字段区(约第 26 行 storage_max_user_configs 之后)追加:
    /// 静态资源目录(env PICHOST_STATIC_DIR);None 时运行期回退 ./dist(目录不存在则不挂载)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_dir: Option<PathBuf>,
    // 说明:figment Env 映射自动生效(与 PICHOST_TOKEN_ENCRYPTION_KEY 同机制,大小写不敏感)
  verify:
    - "cargo test -p pichost-core --test static_config_test"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T1
  title: "Mount SPA static fallback service in build_router"
  files: [pichost-api/src/app.rs, pichost-api/tests/static_serve_test.rs]
  depends_on: [T0]
  breaking: false
  ac:
    - given: "临时 dist 目录含 index.html 与 assets/app.js,路由含 /api/v1/ping"
      when: "mount_static_fallback(router, dist) 后依次请求 /、/assets/app.js、/missing、/api/v1/ping"
      then: "/ 返回 index.html 内容(200);/assets/app.js 返回文件内容;/missing 返回 index.html(SPA fallback);/api/v1/ping 仍返回 pong(路由优先级不变)"
    - given: "dist 目录不存在"
      when: "mount_static_fallback(router, missing_dir) 后请求 /index.html"
      then: "返回 404(未挂载,原路由不受影响)"
    - given: "Docker 场景(无 ./dist 目录)启动 build_router"
      when: "运行 API 并请求 /"
      then: "返回 404 且日志含 warn(静态服务跳过),API 路由 /api/v1/health 正常"
  regression:
    - "cargo test -p pichost-api"
    - "cargo clippy --workspace -- -D warnings"
  test_code: |
    // pichost-api/tests/static_serve_test.rs — 新建(无 DB,纯 Router 单测)
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use http_body_util::BodyExt;
    use pichost_api::app::mount_static_fallback;
    use std::path::Path;
    use tower::ServiceExt;

    async fn body_text(resp: axum::response::Response) -> String {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn serves_spa_with_fallback_and_route_priority() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("index.html"), "<html>SPA</html>").unwrap();
        std::fs::create_dir(tmp.path().join("assets")).unwrap();
        std::fs::write(tmp.path().join("assets/app.js"), "console.log(1)").unwrap();

        let router = Router::new().route("/api/v1/ping", get(|| async { "pong" }));
        let router = mount_static_fallback(router, tmp.path());

        let req = |uri: &str| {
            router
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        };
        let resp = req("/").await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_text(resp).await.contains("SPA"));

        let resp = req("/assets/app.js").await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_text(resp).await, "console.log(1)");

        let resp = req("/missing").await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_text(resp).await.contains("SPA"));

        let resp = req("/api/v1/ping").await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_text(resp).await, "pong");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn missing_dir_skips_mount() {
        let router = Router::new().route("/api/v1/ping", get(|| async { "pong" }));
        let router = mount_static_fallback(router, Path::new("/nonexistent-dist-xyz"));
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/index.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
  impl_code: |
    // pichost-api/src/app.rs — 新增 import:
    // use std::path::Path;
    // use tower_http::services::{ServeDir, ServeFile};

    /// 挂载 SPA 静态服务:目录存在时 ServeDir + index.html fallback;否则原样返回(≤50 行)
    pub fn mount_static_fallback(router: Router, dir: &Path) -> Router {
        if !dir.is_dir() {
            tracing::warn!("static dir {:?} not found; SPA serving disabled", dir);
            return router;
        }
        let index = dir.join("index.html");
        router.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
    }

    // build_router 末尾(现 return router 处)改为:
    //     let static_dir = state
    //         .config
    //         .static_dir
    //         .clone()
    //         .unwrap_or_else(|| PathBuf::from("./dist"));
    //     mount_static_fallback(router, &static_dir)
    // (若 build_router 现以表达式返回 Router,改为先绑定时量再返回;保持函数 ≤50 行,
    //  超出则提取 resolve_static_dir(state) -> PathBuf 辅助函数)
  verify:
    - "cargo test -p pichost-api --test static_serve_test"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T2
  title: "Add minimal CLI arg parsing for service commands"
  files: [pichost-api/src/cli.rs, pichost-api/src/main.rs, pichost-api/tests/cli_test.rs]
  depends_on: []
  breaking: false
  ac:
    - given: "无参数调用 pichost-api"
      when: "parse_cli_args(&[]) 执行"
      then: "返回 Ok(CliCommand::Run)"
    - given: "分别以 --install-service / --uninstall-service / --service / -h 调用"
      when: "parse_cli_args 执行"
      then: "返回 Ok(InstallService) / Ok(UninstallService) / Ok(Service) / Ok(Help)"
    - given: "以未知参数 --bogus 或两个以上参数调用"
      when: "parse_cli_args 执行"
      then: "返回 Err(USAGE),主程序打印 usage 并以退出码 2 终止"
    - given: "main.rs 收到 CliCommand::Run"
      when: "无参数启动 pichost-api"
      then: "进程保持前台运行且监听 3000 端口(启动日志出现 listening),不打印 usage,GET /api/health 返回 200"
  regression:
    - "cargo test -p pichost-api"
    - "cargo test -p pichost-worker"
  test_code: |
    // pichost-api/tests/cli_test.rs — 新建
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
    fn unknown_or_multi_args_error() {
        assert!(parse_cli_args(&args(&["--bogus"])).is_err());
        assert!(parse_cli_args(&args(&["--service", "extra"])).is_err());
    }
  impl_code: |
    // pichost-api/src/cli.rs — 新建(不引入 clap,std::env::args 手写最小解析)
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

    // pichost-api/src/main.rs — 顶部加 `mod cli;`,#[tokio::main] 入口最前面插入:
    //     let cmd = match cli::parse_cli_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
    //         Ok(c) => c,
    //         Err(usage) => {
    //             eprintln!("{usage}");
    //             std::process::exit(2);
    //         }
    //     };
    //     match cmd {
    //         cli::CliCommand::Run => { /* 现有 load_config → run_with 逻辑原样保留 */ }
    //         cli::CliCommand::Help => {
    //             println!("{}", cli::USAGE);
    //             return;
    //         }
    //         other => {
    //             #[cfg(windows)]
    //             {
    //                 crate::service::dispatch_cli(other).await;
    //             }
    //             #[cfg(not(windows))]
    //             {
    //                 eprintln!("error: {:?} is only supported on Windows", other);
    //                 std::process::exit(1);
    //             }
    //         }
    //     }
  verify:
    - "cargo test -p pichost-api --test cli_test"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T3
  title: "Add Windows service module with env-file bootstrap"
  files: [pichost-api/src/service.rs, pichost-api/src/main.rs, pichost-api/tests/service_test.rs]
  depends_on: [T2]
  breaking: false
  ac:
    - given: "env_path 不存在,data_dir 为临时目录"
      when: "ensure_service_env(env_path, data_dir) 执行"
      then: "创建 .env,含 PICHOST_DATABASE_MODE=sqlite、sqlite://<data_dir>/pichost.db(正斜杠)、PICHOST_STORAGE__LOCAL_BASE_PATH,且 JWT secret ≥32 字符、data_dir 目录已创建"
    - given: "env_path 存在但无有效 JWT(或 <32 字符)"
      when: "ensure_service_env 执行"
      then: "追加 PICHOST_AUTH__JWT_SECRET=<64 hex>,原有内容保留"
    - given: "env_path 已含 ≥32 字符 JWT"
      when: "ensure_service_env 执行"
      then: "文件内容完全不变(幂等)"
    - given: "Linux 上执行 cargo check -p pichost-api"
      when: "编译"
      then: "成功(windows-service 仅 cfg(windows) 编译,Linux 零影响)"
  regression:
    - "cargo test -p pichost-api"
    - "cargo clippy --workspace -- -D warnings"
  test_code: |
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
  impl_code: |
    // pichost-api/src/service.rs — 新建
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

    // pichost-api/Cargo.toml — 追加条件依赖:
    // [target.'cfg(windows)'.dependencies]
    // windows-service = "0.7"

    // pichost-api/src/main.rs — T3 增加 `mod service;`,并把现有 sqlite 启动链路
    // (load_config → run_sqlite_migrations → run_with_sqlite)提取为:
    //     pub async fn run_lite_from_env() { ... 现有逻辑原样 ... }
    // 供服务模式复用;postgres 模式在服务下同样可用(URL 来自 .env)
  verify:
    - "cargo test -p pichost-api --test service_test"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T4
  title: "Create shared package install lib and deb postinst"
  files: [packaging/common/install-lib.sh, packaging/deb/postinst, scripts/tests/deb_package_test.sh]
  depends_on: []
  breaking: false
  ac:
    - given: "source packaging/common/install-lib.sh 且 PICHOST_PKG_ROOT=$TMP"
      when: "调用 ensure_pkg_dirs + generate_pkg_env + ensure_pkg_jwt + set_pkg_ownership"
      then: "$TMP/etc/pichost/.env 生成且含 PICHOST_DATABASE_MODE=sqlite、sqlite://$TMP/var/lib/pichost/pichost.db、PICHOST_STATIC_DIR=$TMP/usr/share/pichost/web-ui、JWT ≥32 字符;目录 $TMP/var/lib/pichost 存在"
    - given: ".env 已存在且含有效 JWT"
      when: "再次调用 generate_pkg_env + ensure_pkg_jwt"
      then: "文件内容不变(幂等,无重复行)"
    - given: "packaging/deb/postinst"
      when: "执行 bash -n"
      then: "语法通过,且包含 source /usr/share/pichost/install-lib.sh 与 ensure_pkg_*/enable_pkg_services 调用"
  regression:
    - "bash scripts/tests/install_test.sh <pkg_dir>"
    - "cargo test --workspace"
  test_code: |
    # scripts/tests/deb_package_test.sh — 新建
    #!/usr/bin/env bash
    set -euo pipefail
    # 用法: bash scripts/tests/deb_package_test.sh
    # 断言 packaging/common/install-lib.sh 的 FHS env 生成(共享给 deb postinst / rpm %post)
    ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

    # ① 语法检查
    bash -n "$ROOT/packaging/common/install-lib.sh"
    bash -n "$ROOT/packaging/deb/postinst"

    # ② 功能:临时根下生成 FHS env
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    export PICHOST_PKG_ROOT="$TMP"
    # shellcheck disable=SC1091
    . "$ROOT/packaging/common/install-lib.sh"
    ensure_pkg_dirs
    generate_pkg_env
    ensure_pkg_jwt

    ENV_FILE="$TMP/etc/pichost/.env"
    [ -f "$ENV_FILE" ] || { echo "FAIL: .env missing"; exit 1; }
    grep -q '^PICHOST_DATABASE_MODE=sqlite' "$ENV_FILE" \
      || { echo "FAIL: mode not sqlite"; exit 1; }
    grep -Fq "sqlite://$TMP/var/lib/pichost/pichost.db" "$ENV_FILE" \
      || { echo "FAIL: db url wrong"; exit 1; }
    grep -Fq "PICHOST_STATIC_DIR=$TMP/usr/share/pichost/web-ui" "$ENV_FILE" \
      || { echo "FAIL: static dir missing"; exit 1; }
    [ -d "$TMP/var/lib/pichost" ] || { echo "FAIL: var dir missing"; exit 1; }

    # ③ 幂等:JWT 有效时内容不变
    cp "$ENV_FILE" "$TMP/env.before"
    generate_pkg_env
    ensure_pkg_jwt
    diff -q "$TMP/env.before" "$ENV_FILE" >/dev/null \
      || { echo "FAIL: rerun not idempotent"; exit 1; }

    # ④ JWT 长度断言
    jwt="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$ENV_FILE" | tail -n 1)"
    secret="${jwt#*=}"; secret="${secret%\"}"; secret="${secret#\"}"
    [ "${#secret}" -ge 32 ] || { echo "FAIL: JWT too short"; exit 1; }

    # ⑤ postinst 内容断言
    grep -q 'source /usr/share/pichost/install-lib.sh' "$ROOT/packaging/deb/postinst" \
      || { echo "FAIL: postinst missing lib source"; exit 1; }
    echo "deb_package_test.sh PASS"
  impl_code: |
    # packaging/common/install-lib.sh — 新建(deb postinst / rpm %post 共享;随包安装到 /usr/share/pichost/)
    # 共享包安装逻辑。用法: source install-lib.sh
    # 测试/冒烟覆盖:PICHOST_PKG_ROOT 为所有绝对路径前缀(默认空)
    : "${PICHOST_PKG_ROOT:=}"
    ETC_DIR="${PICHOST_PKG_ROOT}/etc/pichost"
    VAR_DIR="${PICHOST_PKG_ROOT}/var/lib/pichost"
    STATIC_DIR="${PICHOST_PKG_ROOT}/usr/share/pichost/web-ui"
    ENV_FILE="$ETC_DIR/.env"
    SVC_API="pichost-api"

    ensure_pkg_user() {
        [ "$(id -u)" = "0" ] || return 0
        id pichost >/dev/null 2>&1 || useradd --system --home-dir /var/lib/pichost pichost
    }

    ensure_pkg_dirs() {
        mkdir -p "$ETC_DIR" "$VAR_DIR" "$STATIC_DIR"
    }

    generate_pkg_env() {
        [ -f "$ENV_FILE" ] && return 0
        cat > "$ENV_FILE" <<EOF
    PICHOST_DATABASE_MODE=sqlite
    PICHOST_DATABASE_URL="sqlite://$VAR_DIR/pichost.db"
    PICHOST_STORAGE__LOCAL_BASE_PATH="$VAR_DIR/storage-local"
    PICHOST_STATIC_DIR="$STATIC_DIR"
    EOF
        chmod 600 "$ENV_FILE"
    }

    ensure_pkg_jwt() {
        local line secret new_secret
        line="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$ENV_FILE" | tail -n 1 || true)"
        secret="${line#*=}"; secret="${secret%\"}"; secret="${secret#\"}"
        [ -n "$secret" ] && [ "${#secret}" -ge 32 ] && return 0
        if command -v openssl >/dev/null 2>&1; then
            new_secret="$(openssl rand -hex 32)"
        else
            new_secret="$(tr -dc 'a-f0-9' < /dev/urandom | head -c 64 || true)"
        fi
        sed -i -e '/^PICHOST_AUTH_JWT_SECRET=/d' -e '/^PICHOST_AUTH__JWT_SECRET=/d' "$ENV_FILE"
        printf 'PICHOST_AUTH__JWT_SECRET=%s\n' "$new_secret" >> "$ENV_FILE"
        chmod 600 "$ENV_FILE"
    }

    set_pkg_ownership() {
        chown -R pichost:pichost "$VAR_DIR" "$ETC_DIR" 2>/dev/null || true
    }

    enable_pkg_services() {
        [ "$(id -u)" = "0" ] || return 0
        command -v systemctl >/dev/null 2>&1 || return 0
        [ -d /run/systemd/system ] || return 0
        systemctl daemon-reload
        systemctl enable --now "$SVC_API" >/dev/null 2>&1 || true
    }

    # packaging/deb/postinst — 新建
    #!/bin/bash
    set -e
    # deb maintainer script (configure):建用户/建目录/生成 .env + JWT/启动服务
    . /usr/share/pichost/install-lib.sh
    ensure_pkg_user
    ensure_pkg_dirs
    generate_pkg_env
    ensure_pkg_jwt
    set_pkg_ownership
    enable_pkg_services
    exit 0
  verify:
    - "bash scripts/tests/deb_package_test.sh"
    - "cargo test --workspace"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T5
  title: "Add deb prerm and postrm maintainer scripts"
  files: [packaging/deb/prerm, packaging/deb/postrm, scripts/tests/deb_package_test.sh]
  depends_on: [T4]
  breaking: false
  ac:
    - given: "packaging/deb/prerm"
      when: "bash -n 校验且检索分支"
      then: "语法通过;remove/upgrade/deconfigure 分支调用 systemctl stop pichost-api pichost-worker(失败容忍)"
    - given: "packaging/deb/postrm"
      when: "bash -n 校验且检索分支"
      then: "语法通过;purge 分支删除 /var/lib/pichost 与 /etc/pichost/.env;remove 分支清理 systemd 单元并 daemon-reload;upgrade 分支不删数据"
    - given: "deb_package_test.sh"
      when: "执行"
      then: "既有断言 + 新增 prerm/postrm 断言全部 PASS"
  regression:
    - "bash scripts/tests/deb_package_test.sh"
    - "bash -n packaging/common/install-lib.sh"
  test_code: |
    # scripts/tests/deb_package_test.sh — 追加(置于 PASS 输出前):
    # ⑥ prerm/postrm 语法与分支断言
    bash -n "$ROOT/packaging/deb/prerm"
    bash -n "$ROOT/packaging/deb/postrm"
    grep -q 'systemctl stop pichost-api pichost-worker' "$ROOT/packaging/deb/prerm" \
      || { echo "FAIL: prerm missing stop"; exit 1; }
    grep -q 'purge' "$ROOT/packaging/deb/postrm" \
      || { echo "FAIL: postrm missing purge branch"; exit 1; }
    grep -q 'rm -rf /var/lib/pichost' "$ROOT/packaging/deb/postrm" \
      || { echo "FAIL: postrm missing data wipe"; exit 1; }
    grep -q 'upgrade' "$ROOT/packaging/deb/postrm" \
      || { echo "FAIL: postrm missing upgrade guard"; exit 1; }
    echo "deb_package_test.sh PASS"
  impl_code: |
    # packaging/deb/prerm — 新建
    #!/bin/bash
    set -e
    # deb maintainer script (prerm):停止服务
    case "$1" in
        remove|upgrade|deconfigure)
            systemctl stop pichost-api pichost-worker 2>/dev/null || true
            ;;
    esac
    exit 0

    # packaging/deb/postrm — 新建
    #!/bin/bash
    set -e
    # deb maintainer script (postrm):remove 清单元;purge 清数据;upgrade 不动
    case "$1" in
        remove)
            rm -f /lib/systemd/system/pichost-api.service /lib/systemd/system/pichost-worker.service
            systemctl daemon-reload 2>/dev/null || true
            ;;
        purge)
            rm -rf /var/lib/pichost
            rm -f /etc/pichost/.env
            rm -f /lib/systemd/system/pichost-api.service /lib/systemd/system/pichost-worker.service
            ;;
    esac
    exit 0
  verify:
    - "bash scripts/tests/deb_package_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T6
  title: "Add cargo-deb metadata to pichost-api"
  files: [pichost-api/Cargo.toml, scripts/tests/deb_package_test.sh]
  depends_on: [T5]
  breaking: false
  ac:
    - given: "pichost-api/Cargo.toml 含 [package.metadata.deb]"
      when: "检索 assets 清单"
      then: "包含 pichost-api→usr/bin、pichost-worker→usr/bin、../web-ui/dist→usr/share/pichost/web-ui、../migrations→usr/share/pichost/migrations、../migrations-sqlite、../packaging/common/install-lib.sh、../scripts/*.service→lib/systemd/system;maintainer-scripts=packaging/deb"
    - given: "装有 cargo-deb 且已有 release 二进制"
      when: "cargo deb -p pichost-api --no-build --output /tmp/pichost.deb 执行"
      then: "产出 /tmp/pichost.deb 且 dpkg-deb -I 显示 Package: pichost"
    - given: "deb 元数据新增后"
      when: "cargo check -p pichost-api"
      then: "编译不受影响(纯元数据)"
  regression:
    - "bash scripts/tests/deb_package_test.sh"
    - "cargo check -p pichost-api"
  test_code: |
    # scripts/tests/deb_package_test.sh — 追加(置 PASS 输出前):
    # ⑦ cargo-deb 元数据断言
    grep -q '\[package.metadata.deb\]' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: deb metadata missing"; exit 1; }
    grep -q 'maintainer-scripts = "packaging/deb"' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: maintainer-scripts missing"; exit 1; }
    grep -q 'web-ui/dist' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: web-ui asset missing"; exit 1; }
    grep -q 'usr/share/pichost/web-ui' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: web-ui dest missing"; exit 1; }
    grep -q 'pichost-api.service' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: systemd unit asset missing"; exit 1; }
    echo "deb_package_test.sh PASS"
  impl_code: |
    # pichost-api/Cargo.toml — 末尾追加(cargo-deb 元数据;assets 相对 crate 根,用 ../ 引用工作区资源)
    [package.metadata.deb]
    name = "pichost"
    section = "web"
    priority = "optional"
    maintainer = "Jeill Zhang <jeillzhang@users.noreply.github.com>"
    homepage = "https://github.com/JeillZhang/pichost"
    license = "MIT"
    depends = "$auto"
    assets = [
        ["target/release/pichost-api", "usr/bin/", "755"],
        ["target/release/pichost-worker", "usr/bin/", "755"],
        ["../web-ui/dist/", "usr/share/pichost/web-ui/"],
        ["../migrations/", "usr/share/pichost/migrations/"],
        ["../migrations-sqlite/", "usr/share/pichost/migrations-sqlite/"],
        ["../packaging/common/install-lib.sh", "usr/share/pichost/", "644"],
        ["../scripts/pichost-api.service", "lib/systemd/system/", "644"],
        ["../scripts/pichost-worker.service", "lib/systemd/system/", "644"],
    ]
    maintainer-scripts = "packaging/deb"
  verify:
    - "bash scripts/tests/deb_package_test.sh"
    - "command -v cargo-deb >/dev/null 2>&1 && cargo deb -p pichost-api --no-build --output /tmp/pichost.deb && dpkg-deb -I /tmp/pichost.deb | grep -q 'Package: pichost'"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T7
  title: "Add rpm package metadata and %post script"
  files: [pichost-api/Cargo.toml, packaging/rpm/postinstall.sh, scripts/tests/rpm_package_test.sh]
  depends_on: [T6]
  breaking: false
  ac:
    - given: "pichost-api/Cargo.toml 含 [package.metadata.generate-rpm]"
      when: "检索 assets 与脚本字段"
      then: "assets 覆盖 usr/bin、usr/share/pichost/web-ui、migrations、install-lib.sh、usr/lib/systemd/system;postinstall_script=packaging/rpm/postinstall.sh 且 preuninstall/postuninstall 字段指向 packaging/rpm/ 下文件"
    - given: "packaging/rpm/postinstall.sh"
      when: "bash -n 校验"
      then: "语法通过;内容 source /usr/share/pichost/install-lib.sh 并调用 ensure_pkg_* 系列"
    - given: "rpm_package_test.sh"
      when: "执行"
      then: "全部断言 PASS"
  regression:
    - "bash scripts/tests/deb_package_test.sh"
    - "cargo check -p pichost-api"
  test_code: |
    # scripts/tests/rpm_package_test.sh — 新建
    #!/usr/bin/env bash
    set -euo pipefail
    # 用法: bash scripts/tests/rpm_package_test.sh
    ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

    # ① 语法
    bash -n "$ROOT/packaging/rpm/postinstall.sh"
    bash -n "$ROOT/packaging/common/install-lib.sh"

    # ② rpm 元数据
    grep -q '\[package.metadata.generate-rpm\]' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: generate-rpm metadata missing"; exit 1; }
    grep -q 'postinstall_script = "packaging/rpm/postinstall.sh"' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: postinstall_script missing"; exit 1; }
    grep -q 'preuninstall_script' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: preuninstall_script missing"; exit 1; }
    grep -q 'postuninstall_script' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: postuninstall_script missing"; exit 1; }
    grep -q 'usr/lib/systemd/system' "$ROOT/pichost-api/Cargo.toml" \
      || { echo "FAIL: rpm systemd units missing"; exit 1; }

    # ③ %post 内容
    grep -q 'source /usr/share/pichost/install-lib.sh' "$ROOT/packaging/rpm/postinstall.sh" \
      || { echo "FAIL: %post missing lib source"; exit 1; }
    grep -q 'ensure_pkg_jwt' "$ROOT/packaging/rpm/postinstall.sh" \
      || { echo "FAIL: %post missing jwt"; exit 1; }
    echo "rpm_package_test.sh PASS"
  impl_code: |
    # pichost-api/Cargo.toml — [package.metadata.generate-rpm] 追加(在 deb metadata 之后)
    [package.metadata.generate-rpm]
    name = "pichost"
    summary = "Self-hosted image hosting server (SQLite-first)"
    license = "MIT"
    url = "https://github.com/JeillZhang/pichost"
    assets = [
        ["target/release/pichost-api", "usr/bin/", "755"],
        ["target/release/pichost-worker", "usr/bin/", "755"],
        ["../web-ui/dist/", "usr/share/pichost/web-ui/"],
        ["../migrations/", "usr/share/pichost/migrations/"],
        ["../migrations-sqlite/", "usr/share/pichost/migrations-sqlite/"],
        ["../packaging/common/install-lib.sh", "usr/share/pichost/", "644"],
        ["../scripts/pichost-api.service", "usr/lib/systemd/system/", "644"],
        ["../scripts/pichost-worker.service", "usr/lib/systemd/system/", "644"],
    ]
    postinstall_script = "packaging/rpm/postinstall.sh"
    preuninstall_script = "packaging/rpm/preuninstall.sh"
    postuninstall_script = "packaging/rpm/postuninstall.sh"

    # packaging/rpm/postinstall.sh — 新建(rpm %post,复用共享 lib)
    #!/bin/bash
    set -e
    . /usr/share/pichost/install-lib.sh
    ensure_pkg_user
    ensure_pkg_dirs
    generate_pkg_env
    ensure_pkg_jwt
    set_pkg_ownership
    enable_pkg_services
    exit 0
  verify:
    - "bash scripts/tests/rpm_package_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T8
  title: "Add rpm preuninstall and postuninstall scripts"
  files: [packaging/rpm/preuninstall.sh, packaging/rpm/postuninstall.sh, scripts/tests/rpm_package_test.sh]
  depends_on: [T7]
  breaking: false
  ac:
    - given: "packaging/rpm/preuninstall.sh"
      when: "bash -n 校验"
      then: "语法通过;参数 $1=0(卸载)时停止服务并移除 systemd 单元;=1(升级)时不动"
    - given: "packaging/rpm/postuninstall.sh"
      when: "bash -n 校验"
      then: "语法通过;$1=0 时删除 /var/lib/pichost 与 /etc/pichost/.env"
    - given: "rpm_package_test.sh"
      when: "执行"
      then: "既有 + 新增断言全部 PASS"
  regression:
    - "bash scripts/tests/rpm_package_test.sh"
    - "cargo check -p pichost-api"
  test_code: |
    # scripts/tests/rpm_package_test.sh — 追加(置 PASS 输出前):
    # ④ preun/postun 断言
    bash -n "$ROOT/packaging/rpm/preuninstall.sh"
    bash -n "$ROOT/packaging/rpm/postuninstall.sh"
    grep -q '"$1" = "0"' "$ROOT/packaging/rpm/preuninstall.sh" \
      || { echo "FAIL: preun missing uninstall guard"; exit 1; }
    grep -q 'systemctl stop' "$ROOT/packaging/rpm/preuninstall.sh" \
      || { echo "FAIL: preun missing stop"; exit 1; }
    grep -q '"$1" = "0"' "$ROOT/packaging/rpm/postuninstall.sh" \
      || { echo "FAIL: postun missing uninstall guard"; exit 1; }
    grep -q 'rm -rf /var/lib/pichost' "$ROOT/packaging/rpm/postuninstall.sh" \
      || { echo "FAIL: postun missing data wipe"; exit 1; }
    echo "rpm_package_test.sh PASS"
  impl_code: |
    # packaging/rpm/preuninstall.sh — 新建(rpm %preun;rpm 约定 $1=0 卸载 / 1 升级)
    #!/bin/bash
    set -e
    if [ "$1" = "0" ]; then
        systemctl stop pichost-api pichost-worker 2>/dev/null || true
        rm -f /usr/lib/systemd/system/pichost-api.service /usr/lib/systemd/system/pichost-worker.service
        systemctl daemon-reload 2>/dev/null || true
    fi
    exit 0

    # packaging/rpm/postuninstall.sh — 新建(rpm %postun;仅卸载时清数据)
    #!/bin/bash
    set -e
    if [ "$1" = "0" ]; then
        rm -rf /var/lib/pichost
        rm -f /etc/pichost/.env
    fi
    exit 0
  verify:
    - "bash scripts/tests/rpm_package_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T9
  title: "Extend release.yml with linux amd64/arm64 package matrix"
  files: [.github/workflows/release.yml, scripts/tests/release_ci_test.sh]
  depends_on: [T6, T7, T8]
  breaking: false
  ac:
    - given: "release.yml build job"
      when: "检索矩阵"
      then: "包含 x86_64-unknown-linux-gnu/amd64 与 aarch64-unknown-linux-gnu/arm64 两项;arm64 构建命令含 zigbuild"
    - given: "release.yml Package 步骤"
      when: "检索"
      then: "包含 cargo deb -p pichost-api、cargo generate-rpm -p pichost-api、strip、tar.gz 打包与 upload-artifact;deb/rpm 文件随 artifact 上传"
    - given: "release_ci_test.sh"
      when: "执行"
      then: "全部断言 PASS(矩阵双架构、zigbuild、deb/rpm 命令、既有 test/clippy 门保留)"
  regression:
    - "bash scripts/tests/verify_release_test.sh"
    - "cargo test --workspace"
  test_code: |
    # scripts/tests/release_ci_test.sh — 新建
    #!/usr/bin/env bash
    set -euo pipefail
    # 用法: bash scripts/tests/release_ci_test.sh
    ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
    WF="$ROOT/.github/workflows/release.yml"

    grep -q 'x86_64-unknown-linux-gnu' "$WF" || { echo "FAIL: amd64 target missing"; exit 1; }
    grep -q 'aarch64-unknown-linux-gnu' "$WF" || { echo "FAIL: arm64 target missing"; exit 1; }
    grep -q 'zigbuild' "$WF" || { echo "FAIL: zigbuild missing"; exit 1; }
    grep -q 'cargo deb' "$WF" || { echo "FAIL: cargo deb missing"; exit 1; }
    grep -q 'generate-rpm' "$WF" || { echo "FAIL: generate-rpm missing"; exit 1; }
    grep -q 'cargo clippy --workspace -- -D warnings' "$WF" \
      || { echo "FAIL: clippy gate lost"; exit 1; }
    grep -q 'cargo test --workspace' "$WF" || { echo "FAIL: test gate lost"; exit 1; }
    grep -q 'upload-artifact' "$WF" || { echo "FAIL: artifact upload lost"; exit 1; }
    echo "release_ci_test.sh PASS"
  impl_code: |
    # .github/workflows/release.yml — build job 改造(替换现有 matrix + Package 步骤)
    # matrix(替换原单条 include):
    #   strategy:
    #     matrix:
    #       include:
    #         - target: x86_64-unknown-linux-gnu
    #           os: ubuntu-24.04
    #           arch: amd64
    #         - target: aarch64-unknown-linux-gnu
    #           os: ubuntu-24.04
    #           arch: arm64
    # Install Rust toolchain 步骤追加 targets: ${{ matrix.target }}(保持)
    # Build backend 步骤改为(arm64 用 zigbuild):
    #   run: |
    #     if [ "${{ matrix.arch }}" = "arm64" ]; then
    #       cargo install cargo-zigbuild
    #       cargo zigbuild --release --target ${{ matrix.target }} -p pichost-api -p pichost-worker
    #     else
    #       cargo build --release --target ${{ matrix.target }} -p pichost-api -p pichost-worker
    #     fi
    # Package 步骤在现有 tar.gz 逻辑后追加(assets 相对 crate 根,CI 从工作区根执行):
    #   run: |
    #     VERSION=${GITHUB_REF#refs/tags/}
    #     PKG_NAME="pichost-${VERSION}-${{ matrix.arch }}"
    #     mkdir -p dist/$PKG_NAME/web-ui dist/$PKG_NAME/scripts
    #     cp target/${{ matrix.target }}/release/pichost-api dist/$PKG_NAME/
    #     cp target/${{ matrix.target }}/release/pichost-worker dist/$PKG_NAME/
    #     cp -r web-ui/dist dist/$PKG_NAME/web-ui/
    #     cp -r migrations dist/$PKG_NAME/
    #     cp -r migrations-sqlite dist/$PKG_NAME/
    #     cp -r nginx dist/$PKG_NAME/
    #     cp .env.example dist/$PKG_NAME/
    #     cp scripts/install.sh scripts/uninstall.sh dist/$PKG_NAME/scripts/
    #     cp scripts/pichost-api.service scripts/pichost-worker.service dist/$PKG_NAME/scripts/
    #     cp README.md dist/$PKG_NAME/
    #     cd dist && tar czf "${PKG_NAME}.tar.gz" "$PKG_NAME"
    #     # 原生包(已 build release 二进制;--no-build 复用,不重复编译)
    #     cd .. && (cd pichost-api && cargo deb --target ${{ matrix.target }} --no-build --output ../dist/pichost.deb)
    #     (cd pichost-api && cargo generate-rpm --target ${{ matrix.target }} -o ../dist/pichost.rpm) 2>/dev/null || true
    #     cd dist && mv pichost.deb "pichost-${VERSION}-${{ matrix.arch }}.deb"
    #     mv pichost.rpm "pichost-${VERSION}-${{ matrix.arch }}.rpm" 2>/dev/null || true
    #     ls -la
    # Upload artifact 步骤 path 保持 dist/* (含 .deb/.rpm/.tar.gz)
    # 注:预发布 tag(含 -rc/-beta/-alpha/-pre)不上传 rpm 的正式仓库决策在 T13;本任务只负责产出物
  verify:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T10
  title: "Add macOS universal2 build job and brew formula template"
  files: [packaging/homebrew/pichost.rb.tpl, .github/workflows/release.yml, scripts/tests/release_ci_test.sh]
  depends_on: [T9]
  breaking: false
  ac:
    - given: "release.yml 含 macos job"
      when: "检索"
      then: "runs-on macos-14;build 步骤含 aarch64-apple-darwin 与 x86_64-apple-darwin 两 target;lipo -create 产出 pichost-api-universal;打包 darwin-universal.tar.gz 并上传 artifact"
    - given: "packaging/homebrew/pichost.rb.tpl"
      when: "检索占位符"
      then: "含 __VERSION__/__TAG__/__SHA256__;url 指向 GitHub Release 的 darwin-universal tar.gz;含 service block(environment_variables 含 PICHOST_DATABASE_MODE=sqlite 与 PICHOST_STATIC_DIR)与 post_install mkpath var/pichost;test 调用 pichost-api --help"
    - given: "release_ci_test.sh"
      when: "执行"
      then: "既有断言 + 新增 macos/formula 断言全部 PASS"
  regression:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo test --workspace"
  test_code: |
    # scripts/tests/release_ci_test.sh — 追加(置 PASS 输出前):
    # macos job 与 formula 模板断言
    grep -q 'macos-14' "$WF" || { echo "FAIL: macos runner missing"; exit 1; }
    grep -q 'aarch64-apple-darwin' "$WF" || { echo "FAIL: arm64-darwin target missing"; exit 1; }
    grep -q 'x86_64-apple-darwin' "$WF" || { echo "FAIL: x86_64-darwin target missing"; exit 1; }
    grep -q 'lipo -create' "$WF" || { echo "FAIL: lipo missing"; exit 1; }
    grep -q 'darwin-universal.tar.gz' "$WF" || { echo "FAIL: universal tarball missing"; exit 1; }
    FML="$ROOT/packaging/homebrew/pichost.rb.tpl"
    grep -q '__VERSION__' "$FML" || { echo "FAIL: formula version placeholder missing"; exit 1; }
    grep -q '__SHA256__' "$FML" || { echo "FAIL: formula sha placeholder missing"; exit 1; }
    grep -q 'service do' "$FML" || { echo "FAIL: formula service block missing"; exit 1; }
    grep -q 'PICHOST_STATIC_DIR' "$FML" || { echo "FAIL: formula env missing"; exit 1; }
    grep -q -- '--help' "$FML" || { echo "FAIL: formula test missing"; exit 1; }
    echo "release_ci_test.sh PASS"
  impl_code: |
    # .github/workflows/release.yml — 新增 macos job(在 build job 之后,release job 之前):
    #   macos:
    #     name: Build macOS universal2
    #     runs-on: macos-14
    #     steps:
    #       - uses: actions/checkout@v4
    #       - uses: dtolnay/rust-toolchain@stable
    #         with:
    #           targets: aarch64-apple-darwin, x86_64-apple-darwin
    #       - uses: actions/setup-node@v4
    #         with:
    #           node-version: '22'
    #           cache: 'npm'
    #           cache-dependency-path: web-ui/package-lock.json
    #       - run: cd web-ui && npm ci && npm run build
    #       - run: cargo build --release --target aarch64-apple-darwin -p pichost-api -p pichost-worker
    #       - run: cargo build --release --target x86_64-apple-darwin -p pichost-api -p pichost-worker
    #       - name: Create universal binaries
    #         run: |
    #           lipo -create -output pichost-api-universal \
    #             target/aarch64-apple-darwin/release/pichost-api \
    #             target/x86_64-apple-darwin/release/pichost-api
    #           lipo -create -output pichost-worker-universal \
    #             target/aarch64-apple-darwin/release/pichost-worker \
    #             target/x86_64-apple-darwin/release/pichost-worker
    #       - name: Package
    #         run: |
    #           VERSION=${GITHUB_REF#refs/tags/}
    #           PKG_NAME="pichost-${VERSION}-darwin-universal"
    #           mkdir -p dist/$PKG_NAME/web-ui
    #           cp pichost-api-universal pichost-worker-universal dist/$PKG_NAME/
    #           cp -r web-ui/dist dist/$PKG_NAME/web-ui/
    #           cp -r migrations dist/$PKG_NAME/
    #           cp -r migrations-sqlite dist/$PKG_NAME/
    #           cd dist && tar czf "${PKG_NAME}.tar.gz" "$PKG_NAME"
    #       - uses: actions/upload-artifact@v4
    #         with:
    #           name: pichost-darwin-universal
    #           path: dist/*.tar.gz
    #   release job 的 needs 追加 macos;files 列表追加 dist/*.tar.gz(artifact 下载按名)

    # packaging/homebrew/pichost.rb.tpl — 新建(formula 模板;CI 替换 __VERSION__/__TAG__/__SHA256__)
    # frozen_string_literal: true
    #
    # PicHost Homebrew formula(由 release.yml 的 publish job 渲染后推送到 JeillZhang/homebrew-tap)
    class Pichost < Formula
      desc "Self-hosted image hosting server (SQLite-first, zero external deps)"
      homepage "https://github.com/JeillZhang/pichost"
      url "https://github.com/JeillZhang/pichost/releases/download/__TAG__/pichost-__TAG__-darwin-universal.tar.gz"
      sha256 "__SHA256__"
      version "__VERSION__"
      license "MIT"

      depends_on :macos

      def install
        bin.install "pichost-api-universal" => "pichost-api"
        bin.install "pichost-worker-universal" => "pichost-worker"
        (share/"pichost").install Dir["web-ui"]
        (share/"pichost").install Dir["migrations"]
        (share/"pichost").install Dir["migrations-sqlite"]
      end

      def post_install
        (var/"pichost").mkpath
      end

      service do
        run [opt_bin/"pichost-api"]
        environment_variables(
          "PICHOST_DATABASE_MODE" => "sqlite",
          "PICHOST_DATABASE_URL" => "sqlite://#{var}/pichost/pichost.db",
          "PICHOST_STORAGE__LOCAL_BASE_PATH" => "#{var}/pichost/storage-local",
          "PICHOST_STATIC_DIR" => "#{share}/pichost/web-ui",
          "PICHOST_SERVER_PUBLIC_URL" => "http://localhost:3000",
        )
        keep_alive true
      end

      test do
        system "#{bin}/pichost-api", "--help"
      end
    end
  verify:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T11
  title: "Add Windows NSIS installer and release job"
  files: [packaging/windows/installer.nsi, .github/workflows/release.yml, scripts/tests/release_ci_test.sh]
  depends_on: [T3, T9]
  breaking: false
  ac:
    - given: "release.yml 含 windows job"
      when: "检索"
      then: "runs-on windows-latest;cargo build --release --target x86_64-pc-windows-msvc;makensis 调用;上传 PicHost-setup-*.exe artifact"
    - given: "packaging/windows/installer.nsi"
      when: "检索关键节"
      then: "安装目录 $PROGRAMFILES64\\PicHost;安装段复制 pichost-api.exe 并执行 --install-service;卸载段执行 --uninstall-service;卸载页含保留数据选项(ProgramData);RequestExecutionLevel admin"
    - given: "release_ci_test.sh"
      when: "执行"
      then: "既有断言 + 新增 windows/nsi 断言全部 PASS"
  regression:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo test -p pichost-api"
  test_code: |
    # scripts/tests/release_ci_test.sh — 追加(置 PASS 输出前):
    # windows job 与 NSIS 断言
    grep -q 'windows-latest' "$WF" || { echo "FAIL: windows runner missing"; exit 1; }
    grep -q 'x86_64-pc-windows-msvc' "$WF" || { echo "FAIL: windows target missing"; exit 1; }
    grep -q 'makensis' "$WF" || { echo "FAIL: makensis missing"; exit 1; }
    grep -q 'PicHost-setup' "$WF" || { echo "FAIL: installer artifact missing"; exit 1; }
    NSI="$ROOT/packaging/windows/installer.nsi"
    grep -q 'PROGRAMFILES64' "$NSI" || { echo "FAIL: nsi install dir missing"; exit 1; }
    grep -q -- '--install-service' "$NSI" || { echo "FAIL: nsi install-service missing"; exit 1; }
    grep -q -- '--uninstall-service' "$NSI" || { echo "FAIL: nsi uninstall-service missing"; exit 1; }
    grep -q 'ProgramData' "$NSI" || { echo "FAIL: nsi data retention missing"; exit 1; }
    grep -q 'RequestExecutionLevel' "$NSI" || { echo "FAIL: nsi admin missing"; exit 1; }
    echo "release_ci_test.sh PASS"
  impl_code: |
    # packaging/windows/installer.nsi — 新建(NSIS3;CI 以 -DINSTALLER_VERSION=__VERSION__ 传入)
    !include "MUI2.nsh"
    !include "LogicLib.nsh"

    !define APP_NAME "PicHost"
    !define INSTALLER_VERSION "0.0.0" ; CI 以 -DINSTALLER_VERSION=v0.23.0 覆写

    Name "${APP_NAME}"
    OutFile "PicHost-setup-${INSTALLER_VERSION}.exe"
    InstallDir "$PROGRAMFILES64\PicHost"
    RequestExecutionLevel admin

    !define MUI_ABORTWARNING
    !insertmacro MUI_PAGE_WELCOME
    !insertmacro MUI_PAGE_DIRECTORY
    Page custom DataRetentionPage DataRetentionPageLeave
    !insertmacro MUI_PAGE_INSTFILES
    !insertmacro MUI_PAGE_FINISH
    !insertmacro MUI_UNPAGE_CONFIRM
    !insertmacro MUI_UNPAGE_INSTFILES
    !insertmacro MUI_LANGUAGE "English"

    Var KeepData

    Function DataRetentionPage
        nsDialogs::Create 1018
        Pop $0
        ${NSD_CreateCheckBox} 0 0 100% 20u "Keep data in %ProgramData%\PicHost on uninstall"
        Pop $KeepData
        SetBrandingImage /IMGID=$KeepData
        nsDialogs::Show
    FunctionEnd

    Function DataRetentionPageLeave
        ${NSD_GetState} $KeepData $0
        ${If} $0 == ${BST_CHECKED}
            WriteRegDWORD HKCU "Software\PicHost" "KeepData" 1
        ${Else}
            WriteRegDWORD HKCU "Software\PicHost" "KeepData" 0
        ${EndIf}
    FunctionEnd

    Section "Install"
        SetOutPath "$INSTDIR"
        File "pichost-api.exe"
        File "pichost-worker.exe"
        File /r "dist"
        File /r "migrations"
        File /r "migrations-sqlite"
        nsExec::Exec '"$INSTDIR\pichost-api.exe" --install-service'
        WriteUninstaller "$INSTDIR\Uninstall.exe"
    SectionEnd

    Section "Uninstall"
        nsExec::Exec '"$INSTDIR\pichost-api.exe" --uninstall-service'
        ReadRegDWORD $0 HKCU "Software\PicHost" "KeepData"
        ${If} $0 == 1
            RMDir /r "$INSTDIR"
        ${Else}
            RMDir /r "$INSTDIR"
            RMDir /r "$PROGRAMDATA\PicHost"
        ${EndIf}
        DeleteRegKey HKCU "Software\PicHost"
    SectionEnd

    # .github/workflows/release.yml — 新增 windows job(在 macos job 之后):
    #   windows:
    #     name: Build Windows installer
    #     runs-on: windows-latest
    #     steps:
    #       - uses: actions/checkout@v4
    #       - uses: dtolnay/rust-toolchain@stable
    #         with:
    #           targets: x86_64-pc-windows-msvc
    #       - uses: actions/setup-node@v4
    #         with:
    #           node-version: '22'
    #           cache: 'npm'
    #           cache-dependency-path: web-ui/package-lock.json
    #       - run: cd web-ui && npm ci && npm run build
    #       - run: cargo build --release --target x86_64-pc-windows-msvc -p pichost-api -p pichost-worker
    #       - name: Install NSIS
    #         run: choco install nsis -y
    #       - name: Build installer
    #         run: |
    #           mkdir staging
    #           copy target\x86_64-pc-windows-msvc\release\pichost-api.exe staging\
    #           copy target\x86_64-pc-windows-msvc\release\pichost-worker.exe staging\
    #           xcopy web-ui\dist staging\dist\ /E /I /Y
    #           xcopy migrations staging\migrations\ /E /I /Y
    #           xcopy migrations-sqlite staging\migrations-sqlite\ /E /I /Y
    #           makensis /DINSTALLER_VERSION=${{ github.ref_name }} \
    #             /DstagingDir=staging packaging\windows\installer.nsi
    #       - uses: actions/upload-artifact@v4
    #         with:
    #           name: pichost-windows
    #           path: PicHost-setup-*.exe
    #   注:NSIS 脚本内 File 路径以 makensis 工作目录为基准 — CI 在 staging 同级运行,
    #   或脚本内用 ${stagingDir} 绝对路径;两种皆可,以 CI 实际布局为准(保持脚本 ≤120 行)
  verify:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T12
  title: "Create publish-repo.sh and setup-repo.sh"
  files: [scripts/publish-repo.sh, scripts/setup-repo.sh, scripts/tests/repo_publish_test.sh]
  depends_on: [T9]
  breaking: false
  ac:
    - given: "scripts/publish-repo.sh <repo_root> <deb_dir> <rpm_dir> stable 且 GPG_SIGN=0"
      when: "在含假 .deb/.rpm 的临时目录执行"
      then: "repo_root 下生成 apt/dists/stable/main/binary-{amd64,arm64}/Packages(.gz)、apt/dists/stable/Release、apt/pool/main/p/pichost/*.deb、rpm/{x86_64,aarch64}/repodata 布局;GPG_SIGN=0 时跳过签名不报错"
    - given: "GPG_SIGN=1 且 gpg 密钥可用"
      when: "执行 publish-repo.sh"
      then: "Release 文件被签名(InRelease 存在),rpm repomd.xml 被签名"
    - given: "scripts/setup-repo.sh"
      when: "bash -n 校验且检索分支"
      then: "语法通过;apt 分支含 gpg --dearmor + sources.list.d 写入;dnf 分支含 rpm --import + yum.repos.d 写入;未知系统 exit 1"
    - given: "repo_publish_test.sh"
      when: "执行"
      then: "全部断言 PASS(含 publish 布局与 setup 分支)"
  regression:
    - "bash scripts/tests/verify_release_test.sh"
    - "cargo test --workspace"
  test_code: |
    # scripts/tests/repo_publish_test.sh — 新建
    #!/usr/bin/env bash
    set -euo pipefail
    # 用法: bash scripts/tests/repo_publish_test.sh
    ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

    # ① 语法
    bash -n "$ROOT/scripts/publish-repo.sh"
    bash -n "$ROOT/scripts/setup-repo.sh"

    # ② setup-repo.sh 分支断言
    grep -q 'gpg --dearmor' "$ROOT/scripts/setup-repo.sh" \
      || { echo "FAIL: setup apt key missing"; exit 1; }
    grep -q 'sources.list.d/pichost.list' "$ROOT/scripts/setup-repo.sh" \
      || { echo "FAIL: setup apt sources missing"; exit 1; }
    grep -q 'rpm --import' "$ROOT/scripts/setup-repo.sh" \
      || { echo "FAIL: setup rpm key missing"; exit 1; }
    grep -q 'yum.repos.d/pichost.repo' "$ROOT/scripts/setup-repo.sh" \
      || { echo "FAIL: setup dnf repo missing"; exit 1; }

    # ③ publish-repo.sh 布局(需要 dpkg-scanpackages;缺工具则跳过功能部分)
    if command -v dpkg-scanpackages >/dev/null 2>&1 && command -v gzip >/dev/null 2>&1; then
        TMP="$(mktemp -d)"
        trap 'rm -rf "$TMP"' EXIT
        # 造一个最小假 deb(dpkg-deb --build)
        FAKE="$TMP/fake"
        mkdir -p "$FAKE/DEBIAN"
        printf 'Package: pichost\nVersion: 0.23.0\nArchitecture: amd64\nMaintainer: t <t@t>\nDescription: fake\n' \
          > "$FAKE/DEBIAN/control"
        mkdir -p "$TMP/debs" "$TMP/rpms"
        dpkg-deb --build "$FAKE" "$TMP/debs/pichost_0.23.0_amd64.deb" >/dev/null
        mkdir -p "$TMP/rpms/x86_64"
        : > "$TMP/rpms/x86_64/pichost-0.23.0-1.x86_64.rpm"   # 假 rpm(布局测试仅需文件)
        GPG_SIGN=0 bash "$ROOT/scripts/publish-repo.sh" "$TMP/repo" "$TMP/debs" "$TMP/rpms" stable
        [ -f "$TMP/repo/apt/dists/stable/main/binary-amd64/Packages.gz" ] \
          || { echo "FAIL: Packages.gz missing"; exit 1; }
        [ -f "$TMP/repo/apt/dists/stable/Release" ] \
          || { echo "FAIL: Release missing"; exit 1; }
        [ -f "$TMP/repo/apt/pool/main/p/pichost/pichost_0.23.0_amd64.deb" ] \
          || { echo "FAIL: pool deb missing"; exit 1; }
        [ -d "$TMP/repo/rpm/x86_64" ] || { echo "FAIL: rpm arch dir missing"; exit 1; }
    else
        echo "WARN: dpkg-scanpackages missing; publish layout functional check skipped"
    fi
    echo "repo_publish_test.sh PASS"
  impl_code: |
    # scripts/publish-repo.sh — 新建(CI publish job 调用;组装+签名 apt/rpm 仓库)
    #!/bin/bash
    # 用法: publish-repo.sh <repo_root> <deb_dir> <rpm_dir> [suite]
    #   suite: stable(默认)/testing;GPG_SIGN=1 时签名(CI 已导入私钥),=0 跳过(本地测试)
    set -euo pipefail
    REPO_ROOT="${1:?repo_root}"; DEB_DIR="${2:?deb_dir}"; RPM_DIR="${3:?rpm_dir}"
    SUITE="${4:-stable}"
    GPG_SIGN="${GPG_SIGN:-1}"
    PASS_ARGS=()
    [ -n "${GPG_PASSPHRASE:-}" ] && PASS_ARGS=(--pinentry-mode loopback --passphrase "$GPG_PASSPHRASE")

    # ---- apt 仓库 ----
    APT="$REPO_ROOT/apt"
    mkdir -p "$APT/pool/main/p/pichost" \
        "$APT/dists/$SUITE/main/binary-amd64" "$APT/dists/$SUITE/main/binary-arm64"
    cp "$DEB_DIR"/*.deb "$APT/pool/main/p/pichost/"
    cd "$APT"
    for A in amd64 arm64; do
        apt-ftparchive packages "pool/main" \
            > "dists/$SUITE/main/binary-$A/Packages" 2>/dev/null || dpkg-scanpackages "pool/main" \
            > "dists/$SUITE/main/binary-$A/Packages"
        gzip -9c "dists/$SUITE/main/binary-$A/Packages" \
            > "dists/$SUITE/main/binary-$A/Packages.gz"
    done
    apt-ftparchive -o "APT::FTPArchive::Release::Origin=PicHost" \
        -o "APT::FTPArchive::Release::Label=PicHost" \
        -o "APT::FTPArchive::Release::Suite=$SUITE" \
        -o "APT::FTPArchive::Release::Codename=$SUITE" \
        -o "APT::FTPArchive::Release::Components=main" \
        -o "APT::FTPArchive::Release::Architectures=amd64 arm64" \
        release "dists/$SUITE" > "dists/$SUITE/Release"
    if [ "$GPG_SIGN" = "1" ]; then
        gpg "${PASS_ARGS[@]}" --batch --yes -abs -o "dists/$SUITE/Release.gpg" "dists/$SUITE/Release"
        gpg "${PASS_ARGS[@]}" --batch --yes --clearsign -o "dists/$SUITE/InRelease" "dists/$SUITE/Release"
    fi
    cd - >/dev/null

    # ---- rpm 仓库 ----
    RPM="$REPO_ROOT/rpm"
    for ARCH in x86_64 aarch64; do
        [ -d "$RPM_DIR/$ARCH" ] || continue
        mkdir -p "$RPM/$ARCH"
        cp "$RPM_DIR/$ARCH"/*.rpm "$RPM/$ARCH/" 2>/dev/null || true
        if command -v createrepo_c >/dev/null 2>&1; then
            createrepo_c --update -q "$RPM/$ARCH"
            if [ "$GPG_SIGN" = "1" ] && [ -f "$RPM/$ARCH/repodata/repomd.xml" ]; then
                gpg "${PASS_ARGS[@]}" --batch --yes -a -o "$RPM/$ARCH/repodata/repomd.xml.asc" \
                    --detach-sign "$RPM/$ARCH/repodata/repomd.xml"
            fi
        fi
    done

    # ---- 公钥(可选,CI 已导出时) ----
    if [ "$GPG_SIGN" = "1" ] && [ -n "${GPG_FINGERPRINT:-}" ]; then
        gpg --batch --yes --armor --export "$GPG_FINGERPRINT" > "$REPO_ROOT/public.key"
    fi
    echo "repo published: $REPO_ROOT (suite=$SUITE, gpg_sign=$GPG_SIGN)"

    # scripts/setup-repo.sh — 新建(用户侧一键配置;pichost-repo README 指引)
    #!/bin/bash
    # 一键配置 PicHost 软件仓库(apt / dnf 自动检测)
    set -euo pipefail
    REPO_BASE="${PICHOST_REPO_BASE:-https://jeillzhang.github.io/pichost-repo}"
    if command -v apt-get >/dev/null 2>&1; then
        curl -fsSL "$REPO_BASE/public.key" \
            | sudo gpg --dearmor -o /usr/share/keyrings/pichost-archive-keyring.gpg
        echo "deb [signed-by=/usr/share/keyrings/pichost-archive-keyring.gpg] $REPO_BASE/apt stable main" \
            | sudo tee /etc/apt/sources.list.d/pichost.list >/dev/null
        sudo apt-get update -qq
        echo "Done. Run: sudo apt-get install pichost"
    elif command -v dnf >/dev/null 2>&1; then
        sudo rpm --import "$REPO_BASE/public.key"
        ARCH="$(uname -m)"
        case "$ARCH" in
            x86_64) ARCHDIR="x86_64" ;;
            aarch64) ARCHDIR="aarch64" ;;
            *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
        esac
        sudo tee /etc/yum.repos.d/pichost.repo >/dev/null <<EOF
    [pichost]
    name=PicHost Repository
    baseurl=$REPO_BASE/rpm/$ARCHDIR
    enabled=1
    gpgcheck=1
    repo_gpgcheck=0
    gpgkey=$REPO_BASE/public.key
    EOF
        echo "Done. Run: sudo dnf install pichost"
    else
        echo "Unsupported system (need apt-get or dnf)" >&2
        exit 1
    fi
  verify:
    - "bash scripts/tests/repo_publish_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T13
  title: "Add publish-repo and homebrew tap jobs to release.yml"
  files: [.github/workflows/release.yml, scripts/tests/release_ci_test.sh]
  depends_on: [T10, T12]
  breaking: false
  ac:
    - given: "release.yml 含 publish-repo job"
      when: "检索"
      then: "needs 含 build+macos+windows;steps 含导入 gpg(APT_GPG_PRIVATE_KEY)、publish-repo.sh 调用、peaceiris/actions-gh-pages target_repo=jeillzhang/pichost-repo external_token=PICHOST_REPO_PAT、homebrew tap formula 更新(替换 __VERSION__/__TAG__/__SHA256__ 并 push);预发布 tag 用 testing suite"
    - given: "release_ci_test.sh"
      when: "执行"
      then: "既有断言 + 新增 publish/tap 断言全部 PASS"
  regression:
    - "bash scripts/tests/release_ci_test.sh"
    - "bash scripts/tests/repo_publish_test.sh"
  test_code: |
    # scripts/tests/release_ci_test.sh — 追加(置 PASS 输出前):
    # publish-repo job 与 tap 更新断言
    grep -q 'publish-repo' "$WF" || { echo "FAIL: publish job missing"; exit 1; }
    grep -q 'APT_GPG_PRIVATE_KEY' "$WF" || { echo "FAIL: gpg secret missing"; exit 1; }
    grep -q 'publish-repo.sh' "$WF" || { echo "FAIL: publish script call missing"; exit 1; }
    grep -q 'peaceiris/actions-gh-pages' "$WF" || { echo "FAIL: pages action missing"; exit 1; }
    grep -q 'jeillzhang/pichost-repo' "$WF" || { echo "FAIL: pages repo missing"; exit 1; }
    grep -q 'PICHOST_REPO_PAT' "$WF" || { echo "FAIL: PAT secret missing"; exit 1; }
    grep -q 'homebrew-tap' "$WF" || { echo "FAIL: tap update missing"; exit 1; }
    grep -q '__SHA256__' "$WF" || { echo "FAIL: formula sha substitution missing"; exit 1; }
    grep -q 'testing' "$WF" || { echo "FAIL: prerelease suite routing missing"; exit 1; }
    echo "release_ci_test.sh PASS"
  impl_code: |
    # .github/workflows/release.yml — 新增 publish-repo job(在 release job 之前;release job 保持附件发布):
    #   publish-repo:
    #     name: Publish apt/rpm repos + homebrew tap
    #     needs: [build, macos, windows]
    #     runs-on: ubuntu-24.04
    #     steps:
    #       - uses: actions/checkout@v4
    #       - uses: actions/download-artifact@v4
    #         with:
    #           path: artifacts
    #       - name: Install repo tools
    #         run: sudo apt-get update -qq && sudo apt-get install -y -qq dpkg-dev createrepo-c gnupg2
    #       - name: Import GPG signing key
    #         uses: crazy-max/ghaction-import-gpg@v6
    #         with:
    #           gpg_private_key: ${{ secrets.APT_GPG_PRIVATE_KEY }}
    #           passphrase: ${{ secrets.APT_GPG_PASSPHRASE }}
    #           git_user_signingkey: true
    #       - name: Determine suite
    #         id: suite
    #         run: |
    #           TAG="${GITHUB_REF#refs/tags/}"
    #           case "$TAG" in *-rc.*|*-beta*|*-alpha*|*-pre*) echo "suite=testing";; *) echo "suite=stable";; esac \
    #             >> "$GITHUB_OUTPUT"
    #       - name: Assemble and sign repositories
    #         run: |
    #           mkdir -p debs rpms/x86_64 rpms/aarch64
    #           find artifacts -name '*.deb' -exec cp {} debs/ \;
    #           find artifacts -name '*.x86_64.rpm' -exec cp {} rpms/x86_64/ \;
    #           find artifacts -name '*.aarch64.rpm' -exec cp {} rpms/aarch64/ \;
    #           GPG_SIGN=1 GPG_FINGERPRINT="${{ steps.import-gpg.outputs.fingerprint }}" \
    #             bash scripts/publish-repo.sh repo-out debs rpms "${{ steps.suite.outputs.suite }}"
    #       - name: Deploy to GitHub Pages
    #         uses: peaceiris/actions-gh-pages@v4
    #         with:
    #           external_repository: jeillzhang/pichost-repo
    #           publish_branch: gh-pages
    #           publish_dir: ./repo-out
    #           external_token: ${{ secrets.PICHOST_REPO_PAT }}
    #           enable_jekyll: false
    #       - name: Update homebrew tap formula
    #         run: |
    #           TAG="${GITHUB_REF#refs/tags/}"
    #           VERSION="${TAG#v}"
    #           SHA="$(sha256sum artifacts/pichost-darwin-universal/*.tar.gz | cut -d' ' -f1)"
    #           git clone "https://x-access-token:${PICHOST_REPO_PAT}@github.com/jeillzhang/homebrew-tap.git" tap
    #           sed -e "s|__TAG__|${TAG}|g" \
    #               -e "s|__VERSION__|${VERSION}|g" \
    #               -e "s|__SHA256__|${SHA}|g" \
    #               packaging/homebrew/pichost.rb.tpl > tap/pichost.rb
    #           cd tap && git config user.email "ci@users.noreply.github.com" \
    #             && git config user.name "pichost-ci" \
    #             && git add pichost.rb && git commit -m "pichost ${VERSION}" \
    #             && git push origin main
    #   注:预发布 tag 进 testing suite(apt),rpm 不发布(上游产物仍附 GitHub Release)
  verify:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T14
  title: "Add winget submission workflow and manifest reference"
  files: [.github/workflows/winget.yml, packaging/winget/manifest.yaml, scripts/tests/release_ci_test.sh]
  depends_on: [T11]
  breaking: false
  ac:
    - given: ".github/workflows/winget.yml"
      when: "检索"
      then: "release published 触发;windows-latest;WINGET_CREATE_GITHUB_TOKEN 环境变量;wingetcreate update PicHost.PicHost --version --urls --submit;仅稳定版(prerelease 跳过)"
    - given: "packaging/winget/manifest.yaml"
      when: "检索"
      then: "PackageIdentifier: PicHost.PicHost;InstallerType: exe;Silent 开关 /S;安装器 URL 指向 GitHub Release PicHost-setup-*.exe"
    - given: "release_ci_test.sh"
      when: "执行"
      then: "既有断言 + 新增 winget 断言全部 PASS"
  regression:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo test --workspace"
  test_code: |
    # scripts/tests/release_ci_test.sh — 追加(置 PASS 输出前):
    # winget workflow 与 manifest 断言
    WINGET="$ROOT/.github/workflows/winget.yml"
    grep -q 'release' "$WINGET" || { echo "FAIL: winget trigger missing"; exit 1; }
    grep -q 'windows-latest' "$WINGET" || { echo "FAIL: winget runner missing"; exit 1; }
    grep -q 'WINGET_CREATE_GITHUB_TOKEN' "$WINGET" || { echo "FAIL: winget token missing"; exit 1; }
    grep -q 'wingetcreate.exe update PicHost.PicHost' "$WINGET" \
      || { echo "FAIL: winget update missing"; exit 1; }
    grep -q -- '--submit' "$WINGET" || { echo "FAIL: winget submit missing"; exit 1; }
    MAN="$ROOT/packaging/winget/manifest.yaml"
    grep -q 'PicHost.PicHost' "$MAN" || { echo "FAIL: manifest id missing"; exit 1; }
    grep -q 'exe' "$MAN" || { echo "FAIL: manifest type missing"; exit 1; }
    grep -q '/S' "$MAN" || { echo "FAIL: manifest silent switch missing"; exit 1; }
    echo "release_ci_test.sh PASS"
  impl_code: |
    # .github/workflows/winget.yml — 新建(winget-studio 模板模式;首次 manifest 人工提交后自动 PR)
    name: Submit to WinGet
    on:
      release:
        types: [published]
    jobs:
      winget:
        name: Submit PicHost to winget-pkgs
        runs-on: windows-latest
        if: ${{ !github.event.release.prerelease }}
        env:
          WINGET_CREATE_GITHUB_TOKEN: ${{ secrets.WINGET_CREATE_GITHUB_TOKEN }}
        steps:
          - name: Submit package using wingetcreate
            run: |
              $assets = '${{ toJSON(github.event.release.assets) }}' | ConvertFrom-Json
              $url = ($assets | Where-Object { $_.name -like 'PicHost-setup-*.exe' } | Select-Object -First 1).browser_download_url
              $version = $env:GITHUB_REF_NAME.Trim('v')
              curl.exe -JLO https://aka.ms/wingetcreate/latest
              .\wingetcreate.exe update PicHost.PicHost --version $version --urls $url --submit

    # packaging/winget/manifest.yaml — 新建(首次提交 winget-pkgs 的人工清单参考;
    # 之后 wingetcreate update 自动维护同一清单)
    # 提交路径: manifests/p/PicHost/PicHost/0.23.0/
    # InstallerSha256 在首次人工提交时以实际 exe 计算(wingetcreate new 引导)
    # PackageIdentifier: PicHost.PicHost
    # PackageVersion: 0.23.0
    # InstallerType: exe
    # Installers:
    #   - Architecture: x64
    #     InstallerUrl: https://github.com/JeillZhang/pichost/releases/download/v0.23.0/PicHost-setup-v0.23.0.exe
    #     InstallerSha256: <computed-at-submit-time>
    #     InstallerSwitches:
    #       Silent: /S
    #       SilentWithProgress: /S
    #     Scope: machine
    # ManifestType: singleton
    # ManifestVersion: 1.9.0
  verify:
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T15
  title: "Bump version to 0.23.0 with version alignment test"
  files: [Cargo.toml, web-ui/package.json, scripts/tests/version_check_test.sh]
  depends_on: []
  breaking: false
  ac:
    - given: "workspace 根 Cargo.toml 与 web-ui/package.json"
      when: "执行 bash scripts/tests/version_check_test.sh"
      then: "全部断言通过(Cargo.toml、package.json、package-lock.json、Cargo.lock 均 0.23.0)"
    - given: "Cargo.lock"
      when: "cargo check --workspace 后检索 name = \"pichost-api\""
      then: "版本为 0.23.0"
  regression:
    - "cargo check --workspace"
    - "cargo test --workspace"
  test_code: |
    # scripts/tests/version_check_test.sh — 改写为 0.23.0 断言(现为 0.22.0)
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
    grep -q '^version = "0.23.0"' "$ROOT/Cargo.toml" \
      || { echo "FAIL: Cargo.toml not 0.23.0"; exit 1; }
    grep -q '"version": "0.23.0"' "$ROOT/web-ui/package.json" \
      || { echo "FAIL: package.json not 0.23.0"; exit 1; }
    grep -q '"version": "0.23.0"' "$ROOT/web-ui/package-lock.json" \
      || { echo "FAIL: package-lock.json not 0.23.0"; exit 1; }
    grep -A1 'name = "pichost-api"' "$ROOT/Cargo.lock" | grep -q '0.23.0' \
      || { echo "FAIL: Cargo.lock pichost-api not 0.23.0"; exit 1; }
    echo "version_check_test.sh PASS"
  impl_code: |
    sed -i 's/^version = "0.22.0"/version = "0.23.0"/' Cargo.toml
    sed -i 's/"version": "0.22.0"/"version": "0.23.0"/' web-ui/package.json web-ui/package-lock.json
    # 重新生成 Cargo.lock(workspace 成员版本同步)
    cargo check --workspace
    # 确认 Cargo.lock 内 pichost-* 包版本为 0.23.0
    grep -A1 'name = "pichost-api"' Cargo.lock | grep -q '0.23.0'
  verify:
    - "bash scripts/tests/version_check_test.sh"
    - "cargo test --workspace"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T16
  title: "Extend verify-release.sh with deb build and smoke test"
  files: [scripts/verify-release.sh, scripts/tests/verify_release_test.sh]
  depends_on: [T6, T15]
  breaking: false
  ac:
    - given: "scripts/verify-release.sh"
      when: "检索"
      then: "包含 deb 冒烟步骤:cargo deb --no-build 构建 .deb,容器内 dpkg -i 安装后运行二进制,curl /health 与 /(静态首页)返回 200"
    - given: "verify_release_test.sh"
      when: "执行"
      then: "既有断言 + 新增 deb 冒烟步骤断言全部 PASS"
    - given: "完整执行 verify-release.sh"
      when: "运行 bash scripts/verify-release.sh"
      then: "tar.gz 链路回归通过 + deb 冒烟通过(cargo-deb 可用时;缺失则打印 WARN 不阻断)"
  regression:
    - "bash scripts/tests/verify_release_test.sh"
    - "bash scripts/tests/version_check_test.sh"
    - "cargo test --workspace"
  test_code: |
    # scripts/tests/verify_release_test.sh — 追加(置 PASS 输出前):
    # deb 冒烟步骤断言
    grep -q 'deb smoke\|cargo deb' "$ROOT/scripts/verify-release.sh" \
      || { echo "FAIL: verify-release missing deb step"; exit 1; }
    grep -q 'dpkg -i' "$ROOT/scripts/verify-release.sh" \
      || { echo "FAIL: verify-release missing dpkg install"; exit 1; }
    grep -q 'curl.*/health' "$ROOT/scripts/verify-release.sh" \
      || { echo "FAIL: verify-release missing health check"; exit 1; }
    echo "verify_release_test.sh PASS"
  impl_code: |
    # scripts/verify-release.sh — 在 [7/7] install dry-run 之后追加 deb 冒烟步骤(可选,缺工具不阻断):
    # # [8/8] deb 构建 + 安装冒烟(SQLite + 静态服务即开即用)
    # if command -v cargo-deb >/dev/null 2>&1; then
    #     cargo deb -p pichost-api --no-build --output "$DIST_DIR/pichost-smoke.deb"
    #     docker run --rm -v "$DIST_DIR:/pkg:ro" ubuntu:24.04 bash -c "
    #         set -e
    #         apt-get update -qq && apt-get install -y -qq sqlite3 >/dev/null 2>&1 || true
    #         dpkg -i /pkg/pichost-smoke.deb
    #         mkdir -p /var/lib/pichost && chown -R pichost:pichost /var/lib/pichost 2>/dev/null || true
    #         su -s /bin/bash pichost -c 'PICHOST_DATABASE_MODE=sqlite PICHOST_DATABASE_URL=sqlite:///tmp/smoke/pichost.db \
    #             PICHOST_STORAGE__LOCAL_BASE_PATH=/tmp/smoke/storage PICHOST_AUTH__JWT_SECRET=0123456789abcdef0123456789abcdef \
    #             PICHOST_STATIC_DIR=/usr/share/pichost/web-ui PICHOST_SERVER_PUBLIC_URL=http://localhost:3000 \
    #             /usr/bin/pichost-api &'
    #         sleep 4
    #         curl -fsS http://localhost:3000/api/health | grep -q ok
    #         curl -fsS http://localhost:3000/ | grep -qi '<!doctype html'
    #     "
    # else
    #     echo "WARN: cargo-deb not installed; deb smoke skipped"
    # fi
    # 注:容器内 sqlite3 依赖为冒烟自用,包本身零外部依赖(embedded sqlite)
  verify:
    - "bash scripts/tests/verify_release_test.sh"
    - "bash scripts/verify-release.sh --skip-test --skip-lint"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T17
  title: "Sync README.md and AGENTS.md with native packaging"
  files: [README.md, AGENTS.md, scripts/tests/docs_check_test.sh]
  depends_on: [T15, T16]
  breaking: false
  ac:
    - given: "README.md"
      when: "检索原生安装小节"
      then: "包含四平台安装命令(apt install pichost / dnf install pichost / brew install pichost / winget install PicHost.PicHost)、pichost-repo 地址、FHS 与单目录布局说明、PICHOST_STATIC_DIR 配置行"
    - given: "AGENTS.md"
      when: "检索"
      then: "版本 0.23.0;release.yml 矩阵(packaging/ 目录、PICHOST_STATIC_DIR、publish-repo job、winget)记录在案"
    - given: "docs_check_test.sh"
      when: "执行"
      then: "既有断言 + 新增断言全部 PASS"
  regression:
    - "bash scripts/tests/docs_check_test.sh"
    - "bash scripts/tests/version_check_test.sh"
  test_code: |
    # scripts/tests/docs_check_test.sh — 追加(置 PASS 输出前):
    # 原生安装包文档断言
    grep -q 'apt install pichost' "$ROOT/README.md" \
      || { echo "FAIL: README missing apt install"; exit 1; }
    grep -q 'brew install pichost' "$ROOT/README.md" \
      || { echo "FAIL: README missing brew install"; exit 1; }
    grep -q 'winget install PicHost.PicHost' "$ROOT/README.md" \
      || { echo "FAIL: README missing winget"; exit 1; }
    grep -q 'PICHOST_STATIC_DIR' "$ROOT/README.md" \
      || { echo "FAIL: README missing static dir config"; exit 1; }
    grep -q '0.23.0' "$ROOT/AGENTS.md" \
      || { echo "FAIL: AGENTS version not 0.23.0"; exit 1; }
    grep -q 'PICHOST_STATIC_DIR' "$ROOT/AGENTS.md" \
      || { echo "FAIL: AGENTS missing static dir"; exit 1; }
    grep -q 'packaging/' "$ROOT/AGENTS.md" \
      || { echo "FAIL: AGENTS missing packaging dir"; exit 1; }
    echo "docs_check_test.sh PASS"
  impl_code: |
    # README.md 变更:
    # - 版本标语 → **v0.23.0** — Native packages + software repos (deb/rpm/exe, apt/rpm/Homebrew/winget)
    # - Quick Start 新增 "Native packages" 小节:
    #   Debian/Ubuntu:  bash <(curl -sL https://jeillzhang.github.io/pichost-repo/setup-repo.sh) && sudo apt install pichost
    #   Fedora/RHEL:    同上(自动检测 dnf)&& sudo dnf install pichost
    #   macOS:          brew tap jeillzhang/tap && brew install pichost && brew services start pichost
    #   Windows:        winget install PicHost.PicHost
    # - 配置表新增 PICHOST_STATIC_DIR(默认 ./dist,None/缺失不挂载)
    # - Deployment 小节注明:deb/rpm 包采用 FHS 布局(/usr/bin + /usr/share/pichost + /var/lib/pichost
    #   + /etc/pichost),与 install.sh 单目录(/opt/pichost/data)分叉,各自文档化
    # - Features 勾选 Native packaging + software repositories

    # AGENTS.md 变更:
    # - Version 行 0.22.0 → 0.23.0,标语追加 native packaging
    # - Key Commands / Setup Gotchas / Deployment:packaging/ 目录、PICHOST_STATIC_DIR、
    #   release.yml 四平台矩阵 + publish-repo job + winget workflow 记录
    # - 架构笔记新增"原生安装包"小节(deb/rpm FHS、brew formula、NSIS+windows-service)
  verify:
    - "bash scripts/tests/docs_check_test.sh"
    - "cargo clippy --workspace -- -D warnings"
```

---

```yaml
- id: T18
  title: "Finalize CHANGELOG and summary for 0.23.0"
  files: [CHANGELOG.md, .omo/summary/summary_and_next.md, scripts/tests/docs_check_test.sh]
  depends_on: [T9, T17]
  breaking: false
  ac:
    - given: "CHANGELOG.md"
      when: "检索 0.23.0"
      then: "存在 Keep a Changelog 格式条目(Added 分组列出原生包/仓库/静态服务/Windows 服务)"
    - given: ".omo/summary/summary_and_next.md"
      when: "检索"
      then: "存在本次完成小节(含验证与版本 0.23.0)且待实施表更新"
    - given: "docs_check_test.sh"
      when: "执行"
      then: "既有断言 + 新增收尾断言全部 PASS"
  regression:
    - "bash scripts/tests/docs_check_test.sh"
    - "bash scripts/tests/release_ci_test.sh"
  test_code: |
    # scripts/tests/docs_check_test.sh — 追加(置 PASS 输出前):
    grep -q '0.23.0' "$ROOT/CHANGELOG.md" \
      || { echo "FAIL: CHANGELOG missing 0.23.0"; exit 1; }
    grep -q '原生安装包' "$ROOT/.omo/summary/summary_and_next.md" \
      || { echo "FAIL: summary missing phase section"; exit 1; }
    echo "docs_check_test.sh PASS"
  impl_code: |
    # CHANGELOG.md(Keep a Changelog 顶部插入):
    ## [0.23.0] - 2026-08-15
    ### Added
    - Native packages: .deb (amd64/arm64), .rpm (x86_64/aarch64), Windows NSIS installer, macOS universal2 tarball
    - Self-hosted apt + rpm repositories at jeillzhang.github.io/pichost-repo (gpg-signed, one-line setup script)
    - Homebrew tap formula (brew install pichost + brew services) and winget submission workflow
    - In-API SPA static serving (PICHOST_STATIC_DIR, default ./dist) — packages work without Nginx
    - Windows native service (windows-service crate, --install-service / --service)
    ### Changed
    - release.yml build matrix: linux amd64/arm64 (zigbuild), macOS universal2 (lipo), Windows x86_64; publish-repo job assembles/signs repos and updates the tap
    - deb/rpm packages use FHS layout (/usr/bin, /usr/share/pichost, /var/lib/pichost, /etc/pichost)

    # .omo/summary/summary_and_next.md — 顶部新增小节(要点):
    ## 原生安装包与软件仓库分发 ✅ (本次完成)
    - 安装包:deb(amd64+arm64)/rpm(x86_64+aarch64)/NSIS exe/Homebrew universal2
    - 仓库:pichost-repo Pages(apt+rpm,gpg 签名)+ 个人 tap + winget 自动 PR
    - API 静态服务(PICHOST_STATIC_DIR)+ Windows 原生服务(windows-service)
    - 验证:cargo test/clippy/npm build/scripts/tests 全量;版本 0.22.0 → 0.23.0
    - 待实施表清理并标注下一步
  verify:
    - "bash scripts/tests/docs_check_test.sh"
    - "bash scripts/tests/release_ci_test.sh"
    - "cargo test --workspace"
    - "cargo clippy --workspace -- -D warnings"
```
