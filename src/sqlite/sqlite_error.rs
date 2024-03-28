use thiserror::Error;

/// An error that can occur while interacting with the SQLite database.
#[derive(Debug, Error)]
pub enum SqliteError {
    /// An error occurred while interacting with the SQLite database.
    #[error("sqlite error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// An error occurred while running migrations.
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("record not found")]
    /// The requested record was not found in the database.
    NotFound,
}
