use sqlx::{migrate::MigrateError, SqlitePool};

mod coin_store;
mod hardened_key_store;
mod unhardened_key_store;

pub use coin_store::*;
pub use hardened_key_store::*;
pub use unhardened_key_store::*;

pub async fn migrate(pool: &SqlitePool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
