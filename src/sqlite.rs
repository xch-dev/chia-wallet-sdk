use sqlx::{migrate::MigrateError, SqlitePool};

mod hardened_key_store;
mod sqlite_coin_store;
mod unhardened_key_store;

pub use hardened_key_store::*;
pub use sqlite_coin_store::*;
pub use unhardened_key_store::*;

pub async fn migrate(pool: &SqlitePool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
