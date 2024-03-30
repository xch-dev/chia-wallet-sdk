mod coin_store;
mod key_store;
mod puzzle_store;
mod transaction_store;

pub use coin_store::*;
pub use key_store::*;
pub use puzzle_store::*;
pub use transaction_store::*;

use sqlx::migrate::Migrator;

/// The migrator for the SQLite database.
pub static SQLITE_MIGRATOR: Migrator = sqlx::migrate!();
