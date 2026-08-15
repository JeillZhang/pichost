use pichost_core::config::AppConfig;
use pichost_core::db::DbErrorKind;
use pichost_core::i18n::{I18n, Language};
use pichost_core::DbType;
use sqlx::Pool;
use std::error::Error;

use crate::services::user_ops;
use super::prompts::Prompt;

pub async fn create_admin_flow<DB: DbType>(
    pool: &Pool<DB>,
    config: &AppConfig,
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<bool, Box<dyn Error + Send + Sync>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
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
    let i18n = I18n::global();
    if !prompts.confirm(&i18n.t(lang, "setup.admin_confirm"), true)? {
        println!("{}", i18n.t(lang, "setup.admin_skip"));
        return Ok(false);
    }
    loop {
        let username = prompts.input(&i18n.t(lang, "setup.username"), None)?;
        if username.is_empty() || username.len() > 64 {
            println!("{}", i18n.t(lang, "setup.invalid_username"));
            continue;
        }
        let password = prompts.password(
            &i18n.t(lang, "setup.password"),
            Some(&i18n.t(lang, "setup.password_confirm")),
        )?;
        if password.len() < 8 {
            println!("{}", i18n.t(lang, "setup.invalid_password"));
            continue;
        }
        let email_raw = prompts.input(&i18n.t(lang, "setup.email"), None)?;
        let email = if email_raw.is_empty() { None } else { Some(email_raw) };
        if let Some(e) = &email {
            if !e.contains('@') {
                println!("{}", i18n.t(lang, "setup.invalid_email"));
                continue;
            }
        }
        let hash = user_ops::hash_password(&password).map_err(|e| format!("password hashing failed: {e}"))?;
        let quota = if config.upload.storage_quota_default > 0 {
            Some(config.upload.storage_quota_default as i64)
        } else {
            None
        };
        match user_ops::insert_user(pool, &username, &email, &hash, true, quota).await {
            Ok(user_id) => {
                let prefix = format!("users/{user_id}");
                let _ = sqlx::query("UPDATE users SET storage_prefix = $1 WHERE id = $2")
                    .bind(&prefix)
                    .bind(user_id)
                    .execute(pool)
                    .await;
                println!("{}", i18n.t(lang, "setup.complete"));
                return Ok(true);
            }
            Err(e) if pichost_core::db::db_error_kind(&e) == DbErrorKind::UniqueViolation => {
                println!("{}", i18n.t(lang, "setup.username_taken"));
            }
            Err(e) => return Err(e.into()),
        }
    }
}
