pub mod admin;
pub mod env_writer;
pub mod prompts;

use std::error::Error;
use std::io::IsTerminal;
use std::path::PathBuf;

use pichost_core::config::{load_config, AppConfig};
use pichost_core::i18n::{I18n, Language};
use pichost_core::DbType;
use sqlx::Pool;

use crate::services::user_ops;
use prompts::Prompt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyDecision {
    Run,
    SkipWarn,
    ForcedError,
}

pub fn decide_tty(forced: bool, is_tty: bool) -> Result<TtyDecision, &'static str> {
    if is_tty {
        return Ok(TtyDecision::Run);
    }
    if forced {
        return Err("--setup requires an interactive terminal");
    }
    Ok(TtyDecision::SkipWarn)
}

pub fn should_run_wizard(user_count: i64, forced: bool) -> bool {
    forced || user_count == 0
}

pub fn choose_language(config: &AppConfig) -> Language {
    Language::from_str_opt(&config.i18n.language)
}

pub async fn maybe_run<DB: DbType>(
    pool: &Pool<DB>,
    config: &AppConfig,
    forced: bool,
) -> Result<Option<AppConfig>, Box<dyn Error + Send + Sync>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    if !should_run_wizard(user_ops::count_users(pool).await?, forced) {
        return Ok(None);
    }
    let mut lang = choose_language(config);
    match decide_tty(forced, std::io::stdin().is_terminal())? {
        TtyDecision::SkipWarn => {
            let msg = I18n::global().t(lang, "setup.warn_notty");
            tracing::warn!("{msg}");
            return Ok(None);
        }
        TtyDecision::ForcedError => return Err("--setup requires an interactive terminal".into()),
        TtyDecision::Run => {}
    }
    let mut prompts = prompts::DialoguerPrompts;
    println!("{}", I18n::global().t(lang, "setup.welcome"));
    let items = ["en", "zh-CN"];
    let default = match lang {
        Language::En => 0,
        _ => 1,
    };
    let choice = prompts.select(&I18n::global().t(lang, "setup.language"), &items, default)?;
    lang = if choice == 0 { Language::En } else { Language::ZhCN };
    run_wizard(pool, config, lang, &mut prompts).await
}

pub async fn run_wizard<DB: DbType>(
    pool: &Pool<DB>,
    config: &AppConfig,
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<Option<AppConfig>, Box<dyn Error + Send + Sync>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    phase1_config(config, lang, prompts)?;
    let new_config = load_config()?;
    I18n::reload_global(
        Language::from_str_opt(&new_config.i18n.language),
        new_config.i18n.locales_dir.clone(),
    );
    if user_ops::count_users(pool).await? == 0 {
        admin::create_admin_flow(pool, &new_config, lang, prompts).await?;
    }
    Ok(Some(new_config))
}

fn phase1_config(
    config: &AppConfig,
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let i18n = I18n::global();
    let mut updates: Vec<(String, String)> =
        vec![("PICHOST_I18N_LANGUAGE".into(), lang.as_str().into())];
    if !env_writer::validate_jwt_secret(&config.auth.jwt_secret) {
        updates.push(("PICHOST_AUTH__JWT_SECRET".into(), env_writer::generate_jwt_secret()));
        println!("{}", i18n.t(lang, "setup.jwt_generated"));
    } else {
        println!("{}", i18n.t(lang, "setup.jwt_configured"));
    }
    if std::env::var("PICHOST_SERVER__PUBLIC_URL").is_err() {
        loop {
            let url = prompts.input(&i18n.t(lang, "setup.public_url"), Some(&config.server.public_url))?;
            if env_writer::validate_public_url(&url) {
                updates.push(("PICHOST_SERVER__PUBLIC_URL".into(), url));
                break;
            }
            println!("{}", i18n.t(lang, "setup.invalid_url"));
        }
    }
    let path = resolve_env_path(lang, prompts)?;
    let refs: Vec<(&str, &str)> = updates.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    env_writer::apply_env_file(&path, &refs)?;
    for (k, v) in &updates {
        std::env::set_var(k, v);
    }
    Ok(())
}

fn resolve_env_path(
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let explicit = std::env::var("PICHOST_ENV_FILE").ok();
    let cwd = std::env::current_dir()?;
    if let Some(p) = env_writer::probe_env_path(
        explicit.as_deref().map(std::path::Path::new),
        std::path::Path::new("/etc/pichost"),
        &cwd,
    ) {
        return Ok(p);
    }
    let prompt = I18n::global().t(lang, "setup.env_path");
    let answer = prompts.input(&prompt, Some("/etc/pichost/.env"))?;
    Ok(PathBuf::from(answer))
}
