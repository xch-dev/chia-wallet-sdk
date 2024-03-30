use chia_protocol::Bytes32;
use chia_wallet::cat::cat_puzzle_hash;
use sqlx::{Acquire, Result, Sqlite};

/// A CAT puzzle store that uses SQLite as a backend. Uses the table name `cat_puzzle_hashes`.
#[derive(Debug, Clone)]
pub struct SqlitePuzzleStore<T> {
    db: T,
    is_hardened: bool,
    asset_id: Bytes32,
}

impl<'a, T> SqlitePuzzleStore<T>
where
    for<'b> &'b T: Acquire<'a, Database = Sqlite>,
{
    /// Create a new `SqlitePuzzleStore` from a connection pool.
    pub fn new(db: T, is_hardened: bool, asset_id: Bytes32) -> Self {
        Self {
            db,
            is_hardened,
            asset_id,
        }
    }

    /// Check if the store contains any hashes.
    pub async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    /// Get the number of hashes in the store.
    pub async fn len(&self) -> Result<u32> {
        let mut conn = self.db.acquire().await?;
        let asset_id = self.asset_id.to_vec();

        let record = sqlx::query!(
            "
            SELECT COUNT(*) as `count` FROM `cat_puzzle_hashes`
            WHERE `is_hardened` = ? AND `asset_id` = ?
            ",
            self.is_hardened,
            asset_id
        )
        .fetch_one(&mut *conn)
        .await?;

        Ok(record.count as u32)
    }

    /// Get all of the puzzle hashes in the store.
    pub async fn puzzle_hashes(&self) -> Result<Vec<Bytes32>> {
        let mut conn = self.db.acquire().await?;
        let asset_id = self.asset_id.to_vec();

        let records = sqlx::query!(
            "
            SELECT `puzzle_hash` FROM `cat_puzzle_hashes`
            WHERE `is_hardened` = ? AND `asset_id` = ?
            ORDER BY `index` ASC
            ",
            self.is_hardened,
            asset_id
        )
        .fetch_all(&mut *conn)
        .await?;

        Ok(records
            .into_iter()
            .map(|record| Bytes32::new(record.puzzle_hash.try_into().unwrap()))
            .collect())
    }

    /// Extend the CAT puzzle hashes to a given index.
    /// Returns new puzzle hashes, but not existing ones.
    pub async fn extend_hashes(&self, index: u32) -> Result<Vec<Bytes32>> {
        let mut tx = self.db.begin().await?;
        let count = self.len().await?;

        let mut puzzle_hashes = Vec::new();

        for index in count..index {
            let asset_id = self.asset_id.to_vec();

            let p2_puzzle_hash: [u8; 32] = sqlx::query!(
                "
                SELECT `p2_puzzle_hash` FROM `p2_derivations`
                WHERE `index` = ? AND `is_hardened` = ?
                ",
                index,
                self.is_hardened,
            )
            .fetch_one(&mut *tx)
            .await?
            .p2_puzzle_hash
            .try_into()
            .unwrap();

            let puzzle_hash = cat_puzzle_hash(self.asset_id.to_bytes(), p2_puzzle_hash);
            puzzle_hashes.push(Bytes32::new(puzzle_hash));

            let puzzle_hash = puzzle_hash.to_vec();

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
                self.is_hardened,
                asset_id
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(puzzle_hashes)
    }

    /// Get the puzzle hash at the given index.
    pub async fn puzzle_hash(&self, index: u32) -> Result<Option<Bytes32>> {
        let mut conn = self.db.acquire().await?;
        let asset_id = self.asset_id.to_vec();

        let Some(record) = sqlx::query!(
            "
            SELECT `puzzle_hash` FROM `cat_puzzle_hashes`
            WHERE `index` = ? AND `is_hardened` = ? AND `asset_id` = ?
            ",
            index,
            self.is_hardened,
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
    pub async fn index(&self, puzzle_hash: Bytes32) -> Result<Option<u32>> {
        let mut conn = self.db.acquire().await?;
        let asset_id = self.asset_id.to_vec();
        let puzzle_hash = puzzle_hash.to_vec();

        let Some(record) = sqlx::query!(
            "
            SELECT `index` FROM `cat_puzzle_hashes`
            WHERE `puzzle_hash` = ? AND `is_hardened` = ? AND `asset_id` = ?
            ",
            puzzle_hash,
            self.is_hardened,
            asset_id
        )
        .fetch_optional(&mut *conn)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(record.index as u32))
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::{
        derive_keys::master_to_wallet_unhardened_intermediate, DerivableKey, PublicKey,
    };
    use sqlx::SqlitePool;

    use crate::{sqlite::SqliteKeyStore, testing::SECRET_KEY};

    use super::*;

    #[sqlx::test]
    async fn test_puzzle_store(pool: SqlitePool) {
        let asset_id = Bytes32::default();
        let key_store = SqliteKeyStore::new(pool.clone(), false);
        let puzzle_store = SqlitePuzzleStore::new(pool, false, asset_id);

        let intermediate_pk = master_to_wallet_unhardened_intermediate(&SECRET_KEY.public_key());

        // Ensure that a new puzzle store is empty.
        assert!(puzzle_store.is_empty().await.unwrap());

        let puzzle_hashes = puzzle_store.puzzle_hashes().await.unwrap();
        assert!(puzzle_hashes.is_empty());

        // Extend the puzzle store to a given index.
        let pks: Vec<PublicKey> = (0..100)
            .map(|i| intermediate_pk.derive_unhardened(i))
            .collect();
        key_store.extend_keys(0, &pks).await.unwrap();

        let puzzle_hashes = puzzle_store.extend_hashes(100).await.unwrap();

        // Check indices and puzzle hashes.
        for (index, ph) in puzzle_hashes.into_iter().enumerate() {
            assert_eq!(
                puzzle_store
                    .index(ph)
                    .await
                    .unwrap()
                    .expect("no puzzle hash"),
                index as u32
            );
            assert_eq!(
                puzzle_store
                    .puzzle_hash(index as u32)
                    .await
                    .unwrap()
                    .expect("no puzzle hash"),
                ph
            );
        }

        // Try to extend duplicates.
        let puzzle_hashes = puzzle_store.extend_hashes(100).await.unwrap();
        assert_eq!(puzzle_store.len().await.unwrap(), 100);

        // No new puzzle hashes should be added.
        assert_eq!(puzzle_hashes.len(), 0);
    }
}
