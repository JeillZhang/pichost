use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use pichost_core::DbType;
use sqlx::Pool;
use uuid::Uuid;

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
}

pub async fn count_users<DB: DbType>(pool: &Pool<DB>) -> Result<i64, sqlx::Error>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
{
    sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(pool).await
}

pub async fn insert_user<DB: DbType>(
    pool: &Pool<DB>,
    username: &str,
    email: &Option<String>,
    hash: &str,
    is_admin: bool,
    storage_quota: Option<i64>,
) -> Result<Uuid, sqlx::Error>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, is_admin, storage_quota) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(hash)
    .bind(is_admin)
    .bind(storage_quota)
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::hash_password;
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};

    #[test]
    fn hash_password_produces_verifiable_argon2_hash() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(hash.starts_with("$argon2id$"), "hash: {hash}");
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(Argon2::default()
            .verify_password(b"correct horse battery staple", &parsed)
            .is_ok());
        assert!(Argon2::default()
            .verify_password(b"wrong password", &parsed)
            .is_err());
    }
}
