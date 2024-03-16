use std::{collections::HashSet, str::FromStr, sync::Arc};

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
    connect_peer, create_tls_connector, load_ssl_cert, migrate, CoinStore, HardenedKeyStore,
    PuzzleStore, UnhardenedKeyStore,
};
use sqlx::SqlitePool;
use thiserror::Error;

struct Wallet {
    peer: Arc<Peer>,
    hardened_keys: HardenedKeyStore,
    unhardened_keys: UnhardenedKeyStore,
    standard_coins: CoinStore,
}

#[derive(Debug, Error)]
enum WalletError {
    #[error("peer error: {0}")]
    Peer(#[from] chia_client::Error<()>),
}

impl Wallet {
    async fn initial_sync(&self) -> Result<(), WalletError> {
        let mut hardened_phs = HashSet::new();
        let mut unhardened_phs = HashSet::new();
        let mut puzzle_hashes = Vec::new();

        for puzzle_hash in self.hardened_keys.puzzle_hashes().await {
            hardened_phs.insert(Bytes32::new(puzzle_hash));
            puzzle_hashes.push(Bytes32::new(puzzle_hash));
        }

        for puzzle_hash in self.unhardened_keys.puzzle_hashes().await {
            unhardened_phs.insert(Bytes32::new(puzzle_hash));
            puzzle_hashes.push(Bytes32::new(puzzle_hash));
        }

        for puzzle_hashes in puzzle_hashes.chunks(10000) {
            let coin_states = self
                .peer
                .register_for_ph_updates(puzzle_hashes.to_vec(), 0)
                .await?;

            self.apply_updates(coin_states).await?;
        }

        Ok(())
    }

    async fn apply_updates(&self, coin_states: Vec<CoinState>) -> Result<(), WalletError> {
        let mut p2_puzzle_hashes: HashSet<Bytes32> = self
            .hardened_keys
            .puzzle_hashes()
            .await
            .into_iter()
            .map(Bytes32::new)
            .collect();

        p2_puzzle_hashes.extend(
            self.unhardened_keys
                .puzzle_hashes()
                .await
                .into_iter()
                .map(Bytes32::new),
        );

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
    let cert = load_ssl_cert("wallet.key", "wallet.crt");
    let tls_connector = create_tls_connector(&cert);
    let peer = connect_peer("localhost:56342", tls_connector).await?;

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

    let root_sk = master_to_wallet_hardened_intermediate(&sk);
    let hardened_keys = HardenedKeyStore::new(pool.clone(), root_sk, DEFAULT_HIDDEN_PUZZLE_HASH);

    let root_pk = master_to_wallet_unhardened_intermediate(&sk.public_key());
    let unhardened_keys =
        UnhardenedKeyStore::new(pool.clone(), root_pk, DEFAULT_HIDDEN_PUZZLE_HASH);

    let standard_coins = CoinStore::new(pool.clone());

    let wallet = Wallet {
        peer,
        hardened_keys,
        unhardened_keys,
        standard_coins,
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
