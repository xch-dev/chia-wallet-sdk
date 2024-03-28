use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program, SpendBundle};
use sqlx::SqlitePool;

use super::{Result, SqliteError};

/// A SQLite implementation of a transaction store. Uses the tables `transactions` and `coin_spends`.
#[derive(Debug, Clone)]
pub struct TransactionStore {
    db: SqlitePool,
}

impl TransactionStore {
    /// Create a new `TransactionStore` from a connection pool.
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// Connect to a SQLite database and run migrations.
    pub async fn new_with_migrations(db: SqlitePool) -> Result<Self> {
        sqlx::migrate!().run(&db).await?;
        Ok(Self { db })
    }

    /// Get all spent coins from the store.
    pub async fn spent_coins(&self) -> Result<Vec<Coin>> {
        Ok(sqlx::query!(
            "
            SELECT `parent_coin_id`, `puzzle_hash`, `amount`
            FROM `coin_spends` ORDER BY `coin_id` ASC
            "
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|record| {
            let parent_coin_id: [u8; 32] = record.parent_coin_id.try_into().unwrap();
            let puzzle_hash: [u8; 32] = record.puzzle_hash.try_into().unwrap();

            Coin {
                parent_coin_info: parent_coin_id.into(),
                puzzle_hash: puzzle_hash.into(),
                amount: record.amount as u64,
            }
        })
        .collect())
    }

    /// Get a list of all transactions in the store.
    pub async fn transactions(&self) -> Result<Vec<Bytes32>> {
        Ok(
            sqlx::query!(
                "SELECT `transaction_id` AS `transaction_id: Vec<u8>` FROM `transactions`"
            )
            .fetch_all(&self.db)
            .await?
            .into_iter()
            .map(|row| row.transaction_id.try_into().unwrap())
            .collect(),
        )
    }

    /// Get a transaction by its id.
    pub async fn transaction(&self, transaction_id: [u8; 32]) -> Result<SpendBundle> {
        let transaction_id = transaction_id.to_vec();
        let spend_transaction_id = transaction_id.clone();

        let Some(transaction) = sqlx::query!(
            "
            SELECT
                `aggregated_signature` AS `aggregated_signature: Vec<u8>`
            FROM `transactions` WHERE `transaction_id` = ?
            ",
            transaction_id
        )
        .fetch_optional(&self.db)
        .await?
        else {
            return Err(SqliteError::NotFound);
        };

        let coin_spends = sqlx::query!(
            "
            SELECT `coin_id`, `parent_coin_id`, `puzzle_hash`, `amount`,
                   `puzzle_reveal`, `solution`, `transaction_id`
            FROM `coin_spends` WHERE `transaction_id` = ?
            ",
            spend_transaction_id
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|row| {
            let parent_coin_id: [u8; 32] = row.parent_coin_id.try_into().unwrap();
            let puzzle_hash: [u8; 32] = row.puzzle_hash.try_into().unwrap();

            CoinSpend {
                coin: Coin {
                    parent_coin_info: parent_coin_id.into(),
                    puzzle_hash: puzzle_hash.into(),
                    amount: row.amount as u64,
                },
                puzzle_reveal: Program::from(row.puzzle_reveal),
                solution: Program::from(row.solution),
            }
        })
        .collect();

        let signature: [u8; 96] = transaction.aggregated_signature.try_into().unwrap();
        Ok(SpendBundle::new(
            coin_spends,
            Signature::from_bytes(&signature).unwrap(),
        ))
    }

    /// Get the coins spent by a transaction.
    pub async fn removals(&self, transaction_id: Bytes32) -> Result<Vec<Coin>> {
        let transaction_id = transaction_id.to_vec();

        Ok(sqlx::query!(
            "
            SELECT `parent_coin_id`, `puzzle_hash`, `amount`
            FROM `coin_spends` WHERE `transaction_id` = ?
            ORDER BY `coin_id` ASC
            ",
            transaction_id
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|record| {
            let parent_coin_id: [u8; 32] = record.parent_coin_id.try_into().unwrap();
            let puzzle_hash: [u8; 32] = record.puzzle_hash.try_into().unwrap();

            Coin {
                parent_coin_info: parent_coin_id.into(),
                puzzle_hash: puzzle_hash.into(),
                amount: record.amount as u64,
            }
        })
        .collect())
    }

    /// Add a transaction to the store.
    pub async fn add_transaction(&self, spend_bundle: SpendBundle) -> Result<bool> {
        let transaction_id = spend_bundle.name().to_vec();
        let add_transaction_id = transaction_id.clone();
        let aggregated_signature = spend_bundle.aggregated_signature.to_bytes().to_vec();

        let affected = sqlx::query!(
            "
            REPLACE INTO `transactions` (
                `transaction_id`,
                `aggregated_signature`
            )
            VALUES (?, ?)
            ",
            add_transaction_id,
            aggregated_signature
        )
        .execute(&self.db)
        .await?
        .rows_affected();

        if affected == 0 {
            return Ok(false);
        }

        for coin_spend in spend_bundle.coin_spends {
            let coin_id = coin_spend.coin.coin_id().to_vec();
            let parent_coin_id = coin_spend.coin.parent_coin_info.to_vec();
            let puzzle_hash = coin_spend.coin.puzzle_hash.to_vec();
            let amount = coin_spend.coin.amount as i64;
            let puzzle_reveal = coin_spend.puzzle_reveal.as_ref().to_vec();
            let solution = coin_spend.solution.as_ref().to_vec();
            let transaction_id = transaction_id.clone();

            sqlx::query!(
                "
                REPLACE INTO `coin_spends` (
                    `coin_id`,
                    `parent_coin_id`,
                    `puzzle_hash`,
                    `amount`,
                    `puzzle_reveal`,
                    `solution`,
                    `transaction_id`
                )
                VALUES (?, ?, ?, ?, ?, ?, ?)
                ",
                coin_id,
                parent_coin_id,
                puzzle_hash,
                amount,
                puzzle_reveal,
                solution,
                transaction_id
            )
            .execute(&self.db)
            .await?;
        }

        Ok(true)
    }

    /// Remove a transaction from the store.
    pub async fn remove_transaction(&self, transaction_id: Bytes32) -> Result<bool> {
        let transaction_id = transaction_id.to_vec();

        let affected = sqlx::query!(
            "DELETE FROM `transactions` WHERE `transaction_id` = ?",
            transaction_id
        )
        .execute(&self.db)
        .await?
        .rows_affected();

        Ok(affected > 0)
    }
}
