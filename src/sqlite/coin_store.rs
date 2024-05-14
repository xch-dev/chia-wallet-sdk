use chia_protocol::{Bytes32, Coin, CoinState};
use sqlx::{Result, SqliteConnection};

fn asset_id_to_bytes(asset_id: Option<Bytes32>) -> Vec<u8> {
    asset_id.map(|id| id.to_vec()).unwrap_or_default()
}

/// Apply a list of coin updates to the store.
pub async fn upsert_coin_states(
    conn: &mut SqliteConnection,
    coin_states: Vec<CoinState>,
    asset_id: Option<Bytes32>,
) -> Result<()> {
    for coin_state in coin_states {
        let coin_id = coin_state.coin.coin_id().to_vec();
        let parent_coin_info = coin_state.coin.parent_coin_info.to_bytes().to_vec();
        let puzzle_hash = coin_state.coin.puzzle_hash.to_bytes().to_vec();
        let amount = coin_state.coin.amount as i64;
        let asset_id = asset_id_to_bytes(asset_id);

        sqlx::query!(
            "
            REPLACE INTO `coin_states` (
                `coin_id`,
                `parent_coin_info`,
                `puzzle_hash`,
                `amount`,
                `created_height`,
                `spent_height`,
                `asset_id`
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ",
            coin_id,
            parent_coin_info,
            puzzle_hash,
            amount,
            coin_state.created_height,
            coin_state.spent_height,
            asset_id
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}

/// Get a list of all unspent coins in the store.
pub async fn fetch_unspent_coins(
    conn: &mut SqliteConnection,
    asset_id: Option<Bytes32>,
) -> Result<Vec<Coin>> {
    let asset_id = asset_id_to_bytes(asset_id);

    let rows = sqlx::query!(
        "
        SELECT `parent_coin_info`, `puzzle_hash`, `amount`
        FROM `coin_states`
        WHERE `spent_height` IS NULL AND `asset_id` = ?
        ",
        asset_id
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows
        .into_iter()
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
        .collect())
}

/// Get the state of a coin by its id.
pub async fn fetch_coin_state(
    conn: &mut SqliteConnection,
    coin_id: Bytes32,
) -> Result<Option<CoinState>> {
    let coin_id = coin_id.to_vec();

    let Some(row) = sqlx::query!(
        "
        SELECT `parent_coin_info`, `puzzle_hash`, `amount`, `created_height`, `spent_height`
        FROM `coin_states`
        WHERE `coin_id` = ?
        ",
        coin_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(CoinState {
        coin: Coin {
            parent_coin_info: row.parent_coin_info.try_into().unwrap(),
            puzzle_hash: row.puzzle_hash.try_into().unwrap(),
            amount: row.amount as u64,
        },
        created_height: row.created_height.map(|height| height as u32),
        spent_height: row.spent_height.map(|height| height as u32),
    }))
}

/// Check if a puzzle hash is used in the store.
pub async fn query_is_used(conn: &mut SqliteConnection, puzzle_hash: Bytes32) -> Result<bool> {
    let puzzle_hash = puzzle_hash.to_vec();

    let row = sqlx::query!(
        "
        SELECT COUNT(*) AS `count`
        FROM `coin_states`
        WHERE `puzzle_hash` = ?
        ",
        puzzle_hash
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(row.count > 0)
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::*;

    #[sqlx::test]
    async fn test_unspent_coins(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();

        // Insert a spent and unspent coin.
        upsert_coin_states(
            &mut conn,
            vec![
                CoinState {
                    coin: Coin {
                        parent_coin_info: Bytes32::default(),
                        puzzle_hash: Bytes32::default(),
                        amount: 100,
                    },
                    created_height: Some(10),
                    spent_height: None,
                },
                CoinState {
                    coin: Coin {
                        parent_coin_info: Bytes32::default(),
                        puzzle_hash: Bytes32::default(),
                        amount: 101,
                    },
                    created_height: Some(10),
                    spent_height: Some(15),
                },
            ],
            None,
        )
        .await
        .unwrap();

        // Make sure only one is unspent.
        let unspent_coins = fetch_unspent_coins(&mut conn, None).await.unwrap();
        assert_eq!(unspent_coins.len(), 1);
        assert_eq!(unspent_coins[0].amount, 100);
    }

    #[sqlx::test]
    async fn test_coin_state(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();

        // Insert a coin state into the database.
        let coin_state = CoinState {
            coin: Coin {
                parent_coin_info: Bytes32::default(),
                puzzle_hash: Bytes32::default(),
                amount: 100,
            },
            created_height: Some(10),
            spent_height: None,
        };

        upsert_coin_states(&mut conn, vec![coin_state], None)
            .await
            .unwrap();

        // Ensure the result is the same as when it was put in.
        let roundtrip = fetch_coin_state(&mut conn, coin_state.coin.coin_id())
            .await
            .unwrap()
            .expect("coin state not found");
        assert_eq!(coin_state, roundtrip);
    }

    #[sqlx::test]
    async fn test_is_used(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();

        // Insert a coin state into the database.
        let coin_state = CoinState {
            coin: Coin {
                parent_coin_info: Bytes32::default(),
                puzzle_hash: Bytes32::default(),
                amount: 100,
            },
            created_height: Some(10),
            spent_height: None,
        };

        upsert_coin_states(&mut conn, vec![coin_state], None)
            .await
            .unwrap();

        // Ensure the puzzle hash we inserted is used.
        let is_used = query_is_used(&mut conn, coin_state.coin.puzzle_hash)
            .await
            .unwrap();
        assert!(is_used);

        // Ensure a different puzzle hash is not used.
        let is_used = query_is_used(&mut conn, Bytes32::new([1; 32]))
            .await
            .unwrap();
        assert!(!is_used);
    }

    #[sqlx::test]
    async fn test_asset_id(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();
        let asset_id = Some(Bytes32::default());

        // Insert a coin state into the database.
        let coin_state = CoinState {
            coin: Coin {
                parent_coin_info: Bytes32::default(),
                puzzle_hash: Bytes32::default(),
                amount: 100,
            },
            created_height: Some(10),
            spent_height: None,
        };

        upsert_coin_states(&mut conn, vec![coin_state], asset_id)
            .await
            .unwrap();

        // Ensure the result is the same as when it was put in.
        let roundtrip = fetch_coin_state(&mut conn, coin_state.coin.coin_id())
            .await
            .unwrap()
            .expect("coin state not found");
        assert_eq!(coin_state, roundtrip);

        // Ensure the coin is unspent.
        let unspent_coins = fetch_unspent_coins(&mut conn, asset_id).await.unwrap();
        assert_eq!(unspent_coins.len(), 1);

        // Ensure the coin is not found with another asset id.
        let unspent_coins = fetch_unspent_coins(&mut conn, Some(Bytes32::new([1; 32])))
            .await
            .unwrap();
        assert_eq!(unspent_coins.len(), 0);

        // Ensure the coin is not found without the asset id.
        let unspent_coins = fetch_unspent_coins(&mut conn, None).await.unwrap();
        assert_eq!(unspent_coins.len(), 0);
    }
}
