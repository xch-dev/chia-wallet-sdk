use chia_bls::{PublicKey, SecretKey};
use chia_wallet::{standard::standard_puzzle_hash, DeriveSynthetic};
use sqlx::SqlitePool;

use crate::{KeyStore, PuzzleStore};

pub struct HardenedKeyStore {
    pool: SqlitePool,
    hidden_puzzle_hash: [u8; 32],
}

impl HardenedKeyStore {
    pub fn new(pool: SqlitePool, hidden_puzzle_hash: [u8; 32]) -> Self {
        Self {
            pool,
            hidden_puzzle_hash,
        }
    }

    pub async fn derive_to_index(&self, index: u32, intermediate_sk: &SecretKey) {
        let mut tx = self.pool.begin().await.unwrap();

        let count = sqlx::query!("SELECT COUNT(*) AS `count` FROM `hardened_keys`")
            .fetch_one(&self.pool)
            .await
            .unwrap()
            .count as u32;

        for i in count..index {
            let sk = intermediate_sk
                .derive_hardened(i)
                .derive_synthetic(&self.hidden_puzzle_hash);
            let pk = sk.public_key();
            let p2_puzzle_hash = standard_puzzle_hash(&pk);

            let pk_bytes = pk.to_bytes().to_vec();
            let p2_puzzle_hash_bytes = p2_puzzle_hash.to_vec();

            sqlx::query!(
                "
                INSERT INTO `hardened_keys` (
                    `index`,
                    `public_key`,
                    `p2_puzzle_hash`
                )
                VALUES (?, ?, ?)
                ",
                i,
                pk_bytes,
                p2_puzzle_hash_bytes
            )
            .execute(&mut *tx)
            .await
            .unwrap();
        }

        tx.commit().await.unwrap();
    }
}

impl KeyStore for HardenedKeyStore {
    async fn count(&self) -> u32 {
        sqlx::query!("SELECT COUNT(*) AS `count` FROM `hardened_keys`")
            .fetch_one(&self.pool)
            .await
            .unwrap()
            .count as u32
    }

    async fn public_key(&self, index: u32) -> Option<PublicKey> {
        sqlx::query!(
            "SELECT `public_key` FROM `hardened_keys` WHERE `index` = ?",
            index
        )
        .fetch_optional(&self.pool)
        .await
        .unwrap()
        .map(|row| {
            let bytes = row.public_key.try_into().unwrap();
            PublicKey::from_bytes(&bytes).unwrap()
        })
    }

    async fn public_key_index(&self, public_key: &PublicKey) -> Option<u32> {
        let public_key = public_key.to_bytes().to_vec();
        sqlx::query!(
            "SELECT `index` FROM `hardened_keys` WHERE `public_key` = ?",
            public_key
        )
        .fetch_optional(&self.pool)
        .await
        .unwrap()
        .map(|row| row.index as u32)
    }
}

impl PuzzleStore for HardenedKeyStore {
    async fn puzzle_hash(&self, index: u32) -> Option<[u8; 32]> {
        sqlx::query!(
            "SELECT `p2_puzzle_hash` FROM `hardened_keys` WHERE `index` = ?",
            index
        )
        .fetch_optional(&self.pool)
        .await
        .unwrap()
        .map(|row| row.p2_puzzle_hash.try_into().unwrap())
    }

    async fn puzzle_hash_index(&self, puzzle_hash: [u8; 32]) -> Option<u32> {
        let puzzle_hash = puzzle_hash.to_vec();
        sqlx::query!(
            "SELECT `index` FROM `hardened_keys` WHERE `p2_puzzle_hash` = ?",
            puzzle_hash
        )
        .fetch_optional(&self.pool)
        .await
        .unwrap()
        .map(|row| row.index as u32)
    }

    async fn puzzle_hashes(&self) -> Vec<[u8; 32]> {
        sqlx::query!("SELECT `p2_puzzle_hash` FROM `hardened_keys`")
            .fetch_all(&self.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.p2_puzzle_hash.try_into().unwrap())
            .collect()
    }
}
