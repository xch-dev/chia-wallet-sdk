use chia_protocol::Bytes32;
use chia_puzzles::cat::{CatArgs, CAT_PUZZLE_HASH};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use sqlx::{Result, SqliteConnection};

use super::fetch_derivation_index;

/// Get all of the CAT puzzle hashes in the store.
pub async fn fetch_cat_puzzle_hashes(
    conn: &mut SqliteConnection,
    asset_id: Bytes32,
    is_hardened: bool,
) -> Result<Vec<Bytes32>> {
    let asset_id = asset_id.to_vec();

    let records = sqlx::query!(
        "
            SELECT `puzzle_hash` FROM `cat_puzzle_hashes`
            WHERE `is_hardened` = ? AND `asset_id` = ?
            ORDER BY `index` ASC
            ",
        is_hardened,
        asset_id
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(records
        .into_iter()
        .map(|record| Bytes32::new(record.puzzle_hash.try_into().unwrap()))
        .collect())
}

/// Extend the CAT puzzle hashes to match the derivation index.
/// Returns new puzzle hashes, but not existing ones.
pub async fn extend_cat_puzzle_hashes(
    conn: &mut SqliteConnection,
    asset_id: Bytes32,
    is_hardened: bool,
) -> Result<Vec<Bytes32>> {
    let asset_id_bytes = asset_id.to_vec();

    let count = sqlx::query!(
        "
            SELECT COUNT(*) AS `count` FROM `cat_puzzle_hashes`
            WHERE `is_hardened` = ? AND `asset_id` = ?
            ORDER BY `index` ASC
            ",
        is_hardened,
        asset_id_bytes
    )
    .fetch_one(&mut *conn)
    .await?
    .count as u32;

    let index = fetch_derivation_index(conn, is_hardened).await?;

    let mut puzzle_hashes = Vec::new();

    for index in count..index {
        let p2_puzzle_hash: [u8; 32] = sqlx::query!(
            "
                SELECT `p2_puzzle_hash` FROM `p2_derivations`
                WHERE `index` = ? AND `is_hardened` = ?
                ",
            index,
            is_hardened,
        )
        .fetch_one(&mut *conn)
        .await?
        .p2_puzzle_hash
        .try_into()
        .unwrap();

        let puzzle_hash = CurriedProgram {
            program: CAT_PUZZLE_HASH,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                tail_program_hash: asset_id,
                inner_puzzle: TreeHash::new(p2_puzzle_hash),
            },
        }
        .tree_hash();

        puzzle_hashes.push(Bytes32::from(puzzle_hash));

        let puzzle_hash = puzzle_hash.to_vec();
        let asset_id = asset_id.to_vec();

        sqlx::query!(
            "
                REPLACE INTO `cat_puzzle_hashes` (
                    `puzzle_hash`,
                    `index`,
                    `is_hardened`,
                    `asset_id`
                )
                VALUES (?, ?, ?, ?)
                ",
            puzzle_hash,
            index,
            is_hardened,
            asset_id
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(puzzle_hashes)
}

/// Get the puzzle hash at the given index.
pub async fn fetch_cat_puzzle_hash(
    conn: &mut SqliteConnection,
    index: u32,
    asset_id: Bytes32,
    is_hardened: bool,
) -> Result<Option<Bytes32>> {
    let asset_id = asset_id.to_vec();

    let Some(record) = sqlx::query!(
        "
            SELECT `puzzle_hash` FROM `cat_puzzle_hashes`
            WHERE `index` = ? AND `is_hardened` = ? AND `asset_id` = ?
            ",
        index,
        is_hardened,
        asset_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(Bytes32::new(record.puzzle_hash.try_into().unwrap())))
}

/// Get the index of a CAT puzzle hash.
pub async fn cat_puzzle_hash_index(
    conn: &mut SqliteConnection,
    puzzle_hash: Bytes32,
    asset_id: Bytes32,
    is_hardened: bool,
) -> Result<Option<u32>> {
    let asset_id = asset_id.to_vec();
    let puzzle_hash = puzzle_hash.to_vec();

    let Some(record) = sqlx::query!(
        "
            SELECT `index` FROM `cat_puzzle_hashes`
            WHERE `puzzle_hash` = ? AND `is_hardened` = ? AND `asset_id` = ?
            ",
        puzzle_hash,
        is_hardened,
        asset_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(record.index as u32))
}

#[cfg(test)]
mod tests {
    use chia_bls::{
        derive_keys::master_to_wallet_unhardened_intermediate, DerivableKey, PublicKey,
    };
    use sqlx::SqlitePool;

    use super::*;

    #[sqlx::test]
    async fn test_puzzle_store(pool: SqlitePool) {
        let asset_id = Bytes32::default();

        let mut conn = pool.acquire().await.unwrap();

        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Ensure that a new puzzle store is empty.
        let derivation_index = fetch_derivation_index(&mut conn, false).await.unwrap();
        assert_eq!(derivation_index, 0);

        let puzzle_hashes = fetch_cat_puzzle_hashes(&mut conn, asset_id, false)
            .await
            .unwrap();
        assert!(puzzle_hashes.is_empty());

        // Extend the puzzle store to a given index.
        let pks: Vec<PublicKey> = (0..100)
            .map(|i| intermediate_pk.derive_unhardened(i))
            .collect();
        insert_keys(&mut conn, 0, &pks, false).await.unwrap();

        let puzzle_hashes = extend_cat_puzzle_hashes(&mut conn, asset_id, false)
            .await
            .unwrap();

        // Check indices and puzzle hashes.
        for (index, ph) in puzzle_hashes.into_iter().enumerate() {
            assert_eq!(
                cat_puzzle_hash_index(&mut conn, ph, asset_id, false)
                    .await
                    .unwrap()
                    .expect("no puzzle hash"),
                index as u32
            );
            assert_eq!(
                fetch_cat_puzzle_hash(&mut conn, index as u32, asset_id, false)
                    .await
                    .unwrap()
                    .expect("no puzzle hash"),
                ph
            );
        }

        // Try to extend duplicates.
        let puzzle_hashes = extend_cat_puzzle_hashes(&mut conn, asset_id, false)
            .await
            .unwrap();
        let derivation_index = fetch_derivation_index(&mut conn, false).await.unwrap();
        assert_eq!(derivation_index, 100);

        // No new puzzle hashes should be added.
        assert_eq!(puzzle_hashes.len(), 0);
    }
}
