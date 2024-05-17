use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::standard::{StandardArgs, STANDARD_PUZZLE_HASH};
use clvm_utils::{CurriedProgram, ToTreeHash};
use sqlx::{Result, SqliteConnection};

/// Get the number of keys in the store.
pub async fn fetch_derivation_index(conn: &mut SqliteConnection, is_hardened: bool) -> Result<u32> {
    let record = sqlx::query!(
        "
            SELECT COUNT(*) as `count` FROM `p2_derivations`
            WHERE `is_hardened` = ?
            ",
        is_hardened
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(record.count as u32)
}

/// Get all of the puzzle hashes in the store.
pub async fn fetch_puzzle_hashes(
    conn: &mut SqliteConnection,
    is_hardened: bool,
) -> Result<Vec<Bytes32>> {
    let records = sqlx::query!(
        "
            SELECT `p2_puzzle_hash` FROM `p2_derivations`
            WHERE `is_hardened` = ?
            ORDER BY `index` ASC
            ",
        is_hardened
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(records
        .into_iter()
        .map(|record| Bytes32::new(record.p2_puzzle_hash.try_into().unwrap()))
        .collect())
}

/// Add new keys to the store.
pub async fn insert_keys(
    conn: &mut SqliteConnection,
    index: u32,
    public_keys: &[PublicKey],
    is_hardened: bool,
) -> Result<()> {
    for (i, public_key) in public_keys.iter().enumerate() {
        let index = index + i as u32;
        let public_key_bytes = public_key.to_bytes().to_vec();

        let p2_puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs {
                synthetic_key: *public_key,
            },
        }
        .tree_hash()
        .to_vec();

        sqlx::query!(
            "
                REPLACE INTO `p2_derivations` (
                    `index`,
                    `is_hardened`,
                    `synthetic_pk`,
                    `p2_puzzle_hash`
                )
                VALUES (?, ?, ?, ?)
                ",
            index,
            is_hardened,
            public_key_bytes,
            p2_puzzle_hash
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}

/// Get the public key at the given index.
pub async fn fetch_public_key(
    conn: &mut SqliteConnection,
    index: u32,
    is_hardened: bool,
) -> Result<Option<PublicKey>> {
    let Some(record) = sqlx::query!(
        "
            SELECT `synthetic_pk` FROM `p2_derivations`
            WHERE `index` = ? AND `is_hardened` = ?
            ",
        index,
        is_hardened
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(
        PublicKey::from_bytes(&record.synthetic_pk.try_into().unwrap()).unwrap(),
    ))
}

/// Get the puzzle hash at the given index.
pub async fn fetch_puzzle_hash(
    conn: &mut SqliteConnection,
    index: u32,
    is_hardened: bool,
) -> Result<Option<Bytes32>> {
    let Some(record) = sqlx::query!(
        "
            SELECT `p2_puzzle_hash` FROM `p2_derivations`
            WHERE `index` = ? AND `is_hardened` = ?
            ",
        index,
        is_hardened
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(Bytes32::new(
        record.p2_puzzle_hash.try_into().unwrap(),
    )))
}

/// Get the index of a puzzle hash.
pub async fn puzzle_hash_index(
    conn: &mut SqliteConnection,
    puzzle_hash: Bytes32,
    is_hardened: bool,
) -> Result<Option<u32>> {
    let puzzle_hash = puzzle_hash.to_vec();

    Ok(sqlx::query!(
        "
            SELECT `index` FROM `p2_derivations`
            WHERE `p2_puzzle_hash` = ? AND `is_hardened` = ?
            ",
        puzzle_hash,
        is_hardened
    )
    .fetch_optional(&mut *conn)
    .await?
    .map(|record| record.index as u32))
}

/// Get the index of a public key.
pub async fn public_key_index(
    conn: &mut SqliteConnection,
    public_key: PublicKey,
    is_hardened: bool,
) -> Result<Option<u32>> {
    let public_key_bytes = public_key.to_bytes().to_vec();

    Ok(sqlx::query!(
        "
        SELECT `index` FROM `p2_derivations`
        WHERE `synthetic_pk` = ? AND `is_hardened` = ?
        ",
        public_key_bytes,
        is_hardened
    )
    .fetch_optional(&mut *conn)
    .await?
    .map(|record| record.index as u32))
}

#[cfg(test)]
mod tests {
    use chia_bls::{
        derive_keys::{
            master_to_wallet_hardened_intermediate, master_to_wallet_unhardened_intermediate,
        },
        DerivableKey,
    };
    use chia_puzzles::DeriveSynthetic;
    use sqlx::SqlitePool;

    use crate::test::SECRET_KEY;

    use super::*;

    #[sqlx::test]
    async fn test_insert_batches(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();
        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Ensure empty by default.
        let derivation_index = fetch_derivation_index(&mut conn, false).await.unwrap();
        assert_eq!(derivation_index, 0);

        let puzzle_hashes = fetch_puzzle_hashes(&mut conn, false).await.unwrap();
        assert!(puzzle_hashes.is_empty());

        // Insert the first batch.
        let pk_batch_1: Vec<PublicKey> = (0..100)
            .map(|i| intermediate_pk.derive_unhardened(i).derive_synthetic())
            .collect();
        insert_keys(&mut conn, 0, &pk_batch_1, false).await.unwrap();

        // Insert the second batch.
        let pk_batch_2: Vec<PublicKey> = (100..200)
            .map(|i| intermediate_pk.derive_unhardened(i).derive_synthetic())
            .collect();
        insert_keys(&mut conn, 100, &pk_batch_2, false)
            .await
            .unwrap();

        // Check the number of keys.
        let derivation_index = fetch_derivation_index(&mut conn, false).await.unwrap();
        assert_eq!(derivation_index, 200);

        // Check the first key.
        let pk = fetch_public_key(&mut conn, 0, false)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(pk, pk_batch_1[0]);

        // Check the last key.
        let pk = fetch_public_key(&mut conn, 199, false)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(pk, pk_batch_2[99]);
    }

    #[sqlx::test]
    async fn test_indices(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();
        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Insert a batch of keys.
        let pk_batch: Vec<PublicKey> = (0..100)
            .map(|i| intermediate_pk.derive_unhardened(i).derive_synthetic())
            .collect();
        insert_keys(&mut conn, 0, &pk_batch, false).await.unwrap();

        for (i, pk) in pk_batch.into_iter().enumerate() {
            // Check the index of the key.
            let index = public_key_index(&mut conn, pk, false)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(index, i as u32);

            // Ensure the key at the index matches.
            let actual = fetch_public_key(&mut conn, index, false)
                .await
                .unwrap()
                .expect("no public key");
            assert_eq!(actual, pk);

            // Ensure the puzzle hash at the index matches.
            let ph = CurriedProgram {
                program: STANDARD_PUZZLE_HASH,
                args: StandardArgs { synthetic_key: pk },
            }
            .tree_hash();

            let actual = fetch_puzzle_hash(&mut conn, index, false)
                .await
                .unwrap()
                .expect("no puzzle hash");
            assert_eq!(actual, ph.into());

            // Ensure the index of the puzzle hash matches.
            let index = puzzle_hash_index(&mut conn, ph.into(), false)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(index, i as u32);
        }
    }

    #[sqlx::test]
    async fn test_separation(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();

        let unhardened_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());
        let hardened_sk = master_to_wallet_hardened_intermediate(&SECRET_KEY);

        // Insert a public key to unhardened and make sure it's not in hardened.
        let pk = unhardened_pk.derive_unhardened(0).derive_synthetic();
        insert_keys(&mut conn, 0, &[pk], false).await.unwrap();
        assert!(public_key_index(&mut conn, pk, true)
            .await
            .unwrap()
            .is_none());

        // Insert a public key to hardened and make sure it's not in unhardened.
        let pk = hardened_sk
            .derive_hardened(0)
            .public_key()
            .derive_synthetic();
        insert_keys(&mut conn, 0, &[pk], true).await.unwrap();
        assert!(public_key_index(&mut conn, pk, false)
            .await
            .unwrap()
            .is_none());
    }

    #[sqlx::test]
    async fn test_overlap(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();
        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Insert a batch of keys.
        let pk_batch: Vec<PublicKey> = (0..100)
            .map(|i| intermediate_pk.derive_unhardened(i).derive_synthetic())
            .collect();
        insert_keys(&mut conn, 0, &pk_batch, false).await.unwrap();

        // Insert a batch of keys with overlap.
        let pk_batch: Vec<PublicKey> = (50..150)
            .map(|i| intermediate_pk.derive_unhardened(i).derive_synthetic())
            .collect();
        insert_keys(&mut conn, 50, &pk_batch, false).await.unwrap();

        // Check the number of keys.
        let derivation_index = fetch_derivation_index(&mut conn, false).await.unwrap();
        assert_eq!(derivation_index, 150);

        // Check the first key.
        let pk = fetch_public_key(&mut conn, 0, false)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(pk, intermediate_pk.derive_unhardened(0).derive_synthetic());

        // Check the last key.
        let pk = fetch_public_key(&mut conn, 149, false)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(
            pk,
            intermediate_pk.derive_unhardened(149).derive_synthetic()
        );
    }
}
