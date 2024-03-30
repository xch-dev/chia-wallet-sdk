use std::str::FromStr;

use bip39::Mnemonic;
use chia_bls::{
    derive_keys::{
        master_to_wallet_hardened_intermediate, master_to_wallet_unhardened_intermediate,
    },
    DerivableKey, PublicKey, SecretKey,
};
use chia_wallet::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, DeriveSynthetic};
use chia_wallet_sdk::sqlite::{SqliteKeyStore, SQLITE_MIGRATOR};
use sqlx::SqlitePool;

// This is for simulator testing purposes only. Do not use this mnemonic on mainnet.
const MNEMONIC: &str = "
    setup update spoil lazy square course ring tell
    hard eager industry ticket guess amused build reunion
    woman system cause afraid first material machine morning
";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = SqlitePool::connect(":memory:").await?;
    SQLITE_MIGRATOR.run(&pool).await?;

    let seed = Mnemonic::from_str(MNEMONIC)?.to_seed("");
    let root_sk = SecretKey::from_seed(&seed);

    let mut tx = pool.begin().await?;

    let int_pk = master_to_wallet_unhardened_intermediate(&root_sk.public_key());
    let int_sk = master_to_wallet_hardened_intermediate(&root_sk);

    // Block here to satisfy borrow checker.
    {
        let mut unhardened_key_store = SqliteKeyStore::new(&mut tx, false);
        let unhardened_pks: Vec<PublicKey> = (0..100)
            .map(|index| {
                int_pk
                    .derive_unhardened(index)
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
            })
            .collect();
        unhardened_key_store
            .extend_keys(0, unhardened_pks.as_slice())
            .await?;
    }

    // Block here to satisfy borrow checker.
    {
        let mut hardened_key_store = SqliteKeyStore::new(&mut tx, true);
        let hardened_pks: Vec<PublicKey> = (0..100)
            .map(|index| {
                int_sk
                    .derive_hardened(index)
                    .public_key()
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
            })
            .collect();
        hardened_key_store
            .extend_keys(0, hardened_pks.as_slice())
            .await?;
    }

    tx.commit().await?;

    let mut conn = pool.acquire().await?;
    let mut unhardened_key_store = SqliteKeyStore::new(&mut conn, false);

    println!(
        "First unhardened puzzle hash: {}",
        unhardened_key_store
            .puzzle_hash(0)
            .await?
            .expect("missing puzzle hash")
    );

    Ok(())
}
