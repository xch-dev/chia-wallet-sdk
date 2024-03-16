use chia_protocol::{Coin, CoinState};
use sqlx::SqlitePool;

pub struct CoinStore {
    pool: SqlitePool,
}

impl CoinStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn unspent_coins(&self) -> Vec<Coin> {
        let rows = sqlx::query!(
            "
            SELECT `parent_coin_info`, `puzzle_hash`, `amount`
            FROM `coin_states`
            WHERE `spent_height` IS NULL
            "
        )
        .fetch_all(&self.pool)
        .await
        .unwrap();

        rows.into_iter()
            .map(|row| {
                let parent_coin_info = row.parent_coin_info;
                let puzzle_hash = row.puzzle_hash;
                let amount = row.amount as u64;

                Coin {
                    parent_coin_info: parent_coin_info.try_into().unwrap(),
                    puzzle_hash: puzzle_hash.try_into().unwrap(),
                    amount,
                }
            })
            .collect()
    }

    pub async fn coin_state(&self, coin_id: [u8; 32]) -> CoinState {
        let coin_id = coin_id.to_vec();

        let row = sqlx::query!(
            "
            SELECT `parent_coin_info`, `puzzle_hash`, `amount`, `created_height`, `spent_height`
            FROM `coin_states`
            WHERE `coin_id` = ?
            ",
            coin_id
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();

        CoinState {
            coin: Coin {
                parent_coin_info: row.parent_coin_info.try_into().unwrap(),
                puzzle_hash: row.puzzle_hash.try_into().unwrap(),
                amount: row.amount as u64,
            },
            created_height: row.created_height.map(|height| height as u32),
            spent_height: row.spent_height.map(|height| height as u32),
        }
    }

    pub async fn apply_updates(&self, coin_states: Vec<CoinState>) {
        let mut tx = self.pool.begin().await.unwrap();

        for coin_state in coin_states {
            let coin_id = coin_state.coin.coin_id().to_vec();
            let parent_coin_info = coin_state.coin.parent_coin_info.to_bytes().to_vec();
            let puzzle_hash = coin_state.coin.puzzle_hash.to_bytes().to_vec();
            let amount = coin_state.coin.amount as i64;

            sqlx::query!(
                "
                REPLACE INTO `coin_states` (
                    `coin_id`,
                    `parent_coin_info`,
                    `puzzle_hash`,
                    `amount`,
                    `created_height`,
                    `spent_height`
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ",
                coin_id,
                parent_coin_info,
                puzzle_hash,
                amount,
                coin_state.created_height,
                coin_state.spent_height
            )
            .execute(&mut *tx)
            .await
            .unwrap();
        }

        tx.commit().await.unwrap();
    }
}
