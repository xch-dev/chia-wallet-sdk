use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_wallet::standard::standard_puzzle_hash;
use sqlx::{Result, SqliteConnection};

use crate::KeyStore;

/// A key store that uses SQLite as a backend. Uses the table name `derivations`.
#[derive(Debug)]
pub struct SqliteKeyStore<'a> {
    db: &'a mut SqliteConnection,
    is_hardened: bool,
}

impl<'a> SqliteKeyStore<'a> {
    /// Create a new `SqliteKeyStore` from a connection pool.
    pub fn new(db: &'a mut SqliteConnection, is_hardened: bool) -> Self {
        Self { db, is_hardened }
    }

    /// Check if the store contains any keys.
    pub async fn is_empty(&mut self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    /// Get the number of keys in the store.
    pub async fn len(&mut self) -> Result<u32> {
        let record = sqlx::query!(
            "
            SELECT COUNT(*) as `count` FROM `p2_derivations`
            WHERE `is_hardened` = ?
            ",
            self.is_hardened
        )
        .fetch_one(&mut *self.db)
        .await?;

        Ok(record.count as u32)
    }

    /// Get all of the puzzle hashes in the store.
    pub async fn puzzle_hashes(&mut self) -> Result<Vec<Bytes32>> {
        let records = sqlx::query!(
            "
            SELECT `p2_puzzle_hash` FROM `p2_derivations`
            WHERE `is_hardened` = ?
            ORDER BY `index` ASC
            ",
            self.is_hardened
        )
        .fetch_all(&mut *self.db)
        .await?;

        Ok(records
            .into_iter()
            .map(|record| Bytes32::new(record.p2_puzzle_hash.try_into().unwrap()))
            .collect())
    }

    /// Add new keys to the store.
    pub async fn extend_keys(&mut self, index: u32, public_keys: &[PublicKey]) -> Result<()> {
        for (i, public_key) in public_keys.iter().enumerate() {
            let index = index + i as u32;
            let public_key_bytes = public_key.to_bytes().to_vec();
            let p2_puzzle_hash = standard_puzzle_hash(public_key).to_vec();

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
                self.is_hardened,
                public_key_bytes,
                p2_puzzle_hash
            )
            .execute(&mut *self.db)
            .await?;
        }

        Ok(())
    }

    /// Get the public key at the given index.
    pub async fn public_key(&mut self, index: u32) -> Result<Option<PublicKey>> {
        let Some(record) = sqlx::query!(
            "
            SELECT `synthetic_pk` FROM `p2_derivations`
            WHERE `index` = ? AND `is_hardened` = ?
            ",
            index,
            self.is_hardened
        )
        .fetch_optional(&mut *self.db)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(
            PublicKey::from_bytes(&record.synthetic_pk.try_into().unwrap()).unwrap(),
        ))
    }

    /// Get the puzzle hash at the given index.
    pub async fn puzzle_hash(&mut self, index: u32) -> Result<Option<Bytes32>> {
        let Some(record) = sqlx::query!(
            "
            SELECT `p2_puzzle_hash` FROM `p2_derivations`
            WHERE `index` = ? AND `is_hardened` = ?
            ",
            index,
            self.is_hardened
        )
        .fetch_optional(&mut *self.db)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(Bytes32::new(
            record.p2_puzzle_hash.try_into().unwrap(),
        )))
    }

    /// Get the index of a puzzle hash.
    pub async fn ph_index(&mut self, puzzle_hash: Bytes32) -> Result<Option<u32>> {
        let puzzle_hash = puzzle_hash.to_vec();

        Ok(sqlx::query!(
            "
            SELECT `index` FROM `p2_derivations`
            WHERE `p2_puzzle_hash` = ? AND `is_hardened` = ?
            ",
            puzzle_hash,
            self.is_hardened
        )
        .fetch_optional(&mut *self.db)
        .await?
        .map(|record| record.index as u32))
    }
}

impl KeyStore for SqliteKeyStore<'_> {
    type Error = sqlx::Error;

    /// Get the index of a public key.
    async fn pk_index(&mut self, public_key: &PublicKey) -> Result<Option<u32>> {
        let public_key_bytes = public_key.to_bytes().to_vec();

        Ok(sqlx::query!(
            "
            SELECT `index` FROM `p2_derivations`
            WHERE `synthetic_pk` = ? AND `is_hardened` = ?
            ",
            public_key_bytes,
            self.is_hardened
        )
        .fetch_optional(&mut *self.db)
        .await?
        .map(|record| record.index as u32))
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::{
        derive_keys::{
            master_to_wallet_hardened_intermediate, master_to_wallet_unhardened_intermediate,
        },
        DerivableKey,
    };
    use chia_wallet::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, DeriveSynthetic};
    use sqlx::SqlitePool;

    use crate::testing::SECRET_KEY;

    use super::*;

    #[sqlx::test]
    async fn test_insert_batches(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();
        let mut key_store = SqliteKeyStore::new(&mut conn, false);
        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Ensure empty by default.
        assert!(key_store.is_empty().await.unwrap());

        let puzzle_hashes = key_store.puzzle_hashes().await.unwrap();
        assert!(puzzle_hashes.is_empty());

        // Insert the first batch.
        let pk_batch_1: Vec<PublicKey> = (0..100)
            .map(|i| {
                intermediate_pk
                    .derive_unhardened(i)
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
            })
            .collect();
        key_store.extend_keys(0, &pk_batch_1).await.unwrap();

        // Insert the second batch.
        let pk_batch_2: Vec<PublicKey> = (100..200)
            .map(|i| {
                intermediate_pk
                    .derive_unhardened(i)
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
            })
            .collect();
        key_store.extend_keys(100, &pk_batch_2).await.unwrap();

        // Check the number of keys.
        assert_eq!(key_store.len().await.unwrap(), 200);

        // Check the first key.
        let pk = key_store
            .public_key(0)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(pk, pk_batch_1[0]);

        // Check the last key.
        let pk = key_store
            .public_key(199)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(pk, pk_batch_2[99]);
    }

    #[sqlx::test]
    async fn test_indices(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();
        let mut key_store = SqliteKeyStore::new(&mut conn, false);
        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Insert a batch of keys.
        let pk_batch: Vec<PublicKey> = (0..100)
            .map(|i| {
                intermediate_pk
                    .derive_unhardened(i)
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
            })
            .collect();
        key_store.extend_keys(0, &pk_batch).await.unwrap();

        for (i, pk) in pk_batch.into_iter().enumerate() {
            // Check the index of the key.
            let index = key_store.pk_index(&pk).await.unwrap().unwrap();
            assert_eq!(index, i as u32);

            // Ensure the key at the index matches.
            let actual = key_store
                .public_key(index)
                .await
                .unwrap()
                .expect("no public key");
            assert_eq!(actual, pk);

            // Ensure the puzzle hash at the index matches.
            let ph = standard_puzzle_hash(&pk);
            let actual = key_store
                .puzzle_hash(index)
                .await
                .unwrap()
                .expect("no puzzle hash");
            assert_eq!(actual, ph.into());

            // Ensure the index of the puzzle hash matches.
            let index = key_store.ph_index(ph.into()).await.unwrap().unwrap();
            assert_eq!(index, i as u32);
        }
    }

    #[sqlx::test]
    async fn test_separation(pool: SqlitePool) {
        let mut conn1 = pool.acquire().await.unwrap();
        let mut conn2 = pool.acquire().await.unwrap();

        let mut unhardened_key_store = SqliteKeyStore::new(&mut conn1, false);
        let unhardened_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        let mut hardened_key_store = SqliteKeyStore::new(&mut conn2, true);
        let hardened_sk = master_to_wallet_hardened_intermediate(&SECRET_KEY);

        // Insert a public key to unhardened and make sure it's not in hardened.
        let pk = unhardened_pk
            .derive_unhardened(0)
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
        unhardened_key_store
            .extend_keys(0, &[pk.clone()])
            .await
            .unwrap();
        assert!(hardened_key_store.pk_index(&pk).await.unwrap().is_none());

        // Insert a public key to hardened and make sure it's not in unhardened.
        let pk = hardened_sk
            .derive_hardened(0)
            .public_key()
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
        hardened_key_store
            .extend_keys(0, &[pk.clone()])
            .await
            .unwrap();
        assert!(unhardened_key_store.pk_index(&pk).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn test_overlap(pool: SqlitePool) {
        let mut conn = pool.acquire().await.unwrap();
        let mut key_store = SqliteKeyStore::new(&mut conn, false);
        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Insert a batch of keys.
        let pk_batch: Vec<PublicKey> = (0..100)
            .map(|i| {
                intermediate_pk
                    .derive_unhardened(i)
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
            })
            .collect();
        key_store.extend_keys(0, &pk_batch).await.unwrap();

        // Insert a batch of keys with overlap.
        let pk_batch: Vec<PublicKey> = (50..150)
            .map(|i| {
                intermediate_pk
                    .derive_unhardened(i)
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
            })
            .collect();
        key_store.extend_keys(50, &pk_batch).await.unwrap();

        // Check the number of keys.
        assert_eq!(key_store.len().await.unwrap(), 150);

        // Check the first key.
        let pk = key_store
            .public_key(0)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(
            pk,
            intermediate_pk
                .derive_unhardened(0)
                .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
        );

        // Check the last key.
        let pk = key_store
            .public_key(149)
            .await
            .unwrap()
            .expect("no public key");
        assert_eq!(
            pk,
            intermediate_pk
                .derive_unhardened(149)
                .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
        );
    }
}
