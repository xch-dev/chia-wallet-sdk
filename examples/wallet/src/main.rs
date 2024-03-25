use std::{collections::HashSet, str::FromStr};

use bip39::Mnemonic;
use chia_bls::{
    derive_keys::{
        master_to_wallet_hardened_intermediate, master_to_wallet_unhardened_intermediate,
    },
    SecretKey,
};
use chia_client::Peer;
use chia_protocol::{Bytes32, CoinState, NodeType};
use chia_wallet::standard::DEFAULT_HIDDEN_PUZZLE_HASH;
use chia_wallet_sdk::{
    connect_peer, create_tls_connector, load_ssl_cert, migrate, unused_indices, CoinStore,
    HardenedKeyStore, PuzzleStore, SqliteCoinStore, UnhardenedKeyStore,
};
use sqlx::SqlitePool;
use thiserror::Error;

struct Wallet {
    peer: Peer,
    unhardened_keys: UnhardenedKeyStore,
    hardened_keys: HardenedKeyStore,
    intermediate_sk: SecretKey,
    standard_coins: SqliteCoinStore,
    derivation_size: u32,
}

#[derive(Debug, Error)]
enum WalletError {
    #[error("peer error: {0}")]
    Peer(#[from] chia_client::Error<()>),
}

impl Wallet {
    async fn initial_sync(&self) -> Result<(), WalletError> {
        self.unhardened_keys
            .derive_to_index(self.derivation_size)
            .await;

        self.hardened_keys
            .derive_to_index(self.derivation_size, &self.intermediate_sk)
            .await;

        for puzzle_hashes in self.unhardened_keys.puzzle_hashes().await.chunks(10000) {
            let coin_states = self
                .peer
                .register_for_ph_updates(
                    puzzle_hashes.iter().map(|ph| Bytes32::new(*ph)).collect(),
                    0,
                )
                .await?;

            self.apply_updates(coin_states).await?;
        }

        for puzzle_hashes in self.hardened_keys.puzzle_hashes().await.chunks(10000) {
            let coin_states = self
                .peer
                .register_for_ph_updates(
                    puzzle_hashes.iter().map(|ph| Bytes32::new(*ph)).collect(),
                    0,
                )
                .await?;

            self.apply_updates(coin_states).await?;
        }

        loop {
            match unused_indices(&self.unhardened_keys, &self.standard_coins).await {
                Ok(indices) => {
                    if indices.len() < self.derivation_size as usize {
                        if !self
                            .derive_unhardened_to(indices.end + self.derivation_size)
                            .await?
                        {
                            break;
                        }
                        continue;
                    }
                    break;
                }
                Err(index) => {
                    if !self.derive_unhardened_to(index).await? {
                        break;
                    }
                }
            }
        }

        loop {
            match unused_indices(&self.hardened_keys, &self.standard_coins).await {
                Ok(indices) => {
                    if indices.len() < self.derivation_size as usize {
                        if !self
                            .derive_hardened_to(indices.end + self.derivation_size)
                            .await?
                        {
                            break;
                        }
                        continue;
                    }
                    break;
                }
                Err(index) => {
                    if !self.derive_hardened_to(index).await? {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn derive_unhardened_to(&self, index: u32) -> Result<bool, WalletError> {
        self.unhardened_keys.derive_to_index(index).await;
        let puzzle_hashes = self.unhardened_keys.puzzle_hashes().await;

        let coin_states = self
            .peer
            .register_for_ph_updates(
                puzzle_hashes.iter().map(|ph| Bytes32::new(*ph)).collect(),
                0,
            )
            .await?;
        let found = !coin_states.is_empty();

        self.apply_updates(coin_states).await?;

        Ok(found)
    }

    async fn derive_hardened_to(&self, index: u32) -> Result<bool, WalletError> {
        self.hardened_keys
            .derive_to_index(index, &self.intermediate_sk)
            .await;
        let puzzle_hashes = self.hardened_keys.puzzle_hashes().await;

        let coin_states = self
            .peer
            .register_for_ph_updates(
                puzzle_hashes.iter().map(|ph| Bytes32::new(*ph)).collect(),
                0,
            )
            .await?;
        let found = !coin_states.is_empty();

        self.apply_updates(coin_states).await?;

        Ok(found)
    }

    async fn apply_updates(&self, coin_states: Vec<CoinState>) -> Result<(), WalletError> {
        let p2_puzzle_hashes: HashSet<Bytes32> = self
            .unhardened_keys
            .puzzle_hashes()
            .await
            .into_iter()
            .chain(self.hardened_keys.puzzle_hashes().await)
            .map(Bytes32::new)
            .collect();

        let mut standard_coins = Vec::new();

        for coin_state in coin_states {
            if p2_puzzle_hashes.contains(&coin_state.coin.puzzle_hash) {
                standard_coins.push(coin_state);
            } else {
                println!(
                    "Found hinted coin state for unknown puzzle hash {:?}, with id {:?}",
                    coin_state.coin.puzzle_hash,
                    coin_state.coin.coin_id()
                );
            }
        }

        self.standard_coins.apply_updates(standard_coins).await;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cert = load_ssl_cert("wallet.key", "wallet.crt")?;
    let tls_connector = create_tls_connector(&cert)?;
    let peer = connect_peer("localhost:38926", tls_connector).await?;

    peer.send_handshake("simulator0".to_string(), NodeType::Wallet)
        .await?;

    let pool = SqlitePool::connect("sqlite://wallet.sqlite?mode=rwc").await?;
    migrate(&pool).await?;

    let mnemonic = Mnemonic::from_str(
        "
        destroy name laptop bleak august silent supreme tide
        cry velvet tooth edge result human common grab
        brush play walnut heavy harvest upper fat just
        ",
    )?;
    let seed = mnemonic.to_seed("");
    let sk = SecretKey::from_seed(&seed);

    let intermediate_pk = master_to_wallet_unhardened_intermediate(&sk.public_key());
    let unhardened_keys =
        UnhardenedKeyStore::new(pool.clone(), intermediate_pk, DEFAULT_HIDDEN_PUZZLE_HASH);

    let intermediate_sk = master_to_wallet_hardened_intermediate(&sk);
    let hardened_keys = HardenedKeyStore::new(pool.clone(), DEFAULT_HIDDEN_PUZZLE_HASH);

    let standard_coins = SqliteCoinStore::new(pool.clone());

    let wallet = Wallet {
        peer,
        unhardened_keys,
        hardened_keys,
        intermediate_sk,
        standard_coins,
        derivation_size: 500,
    };

    wallet.initial_sync().await?;

    println!(
        "Synced {} balance",
        wallet
            .standard_coins
            .unspent_coins()
            .await
            .iter()
            .fold(0u128, |acc, coin| acc + coin.amount as u128)
    );

    Ok(())
}
