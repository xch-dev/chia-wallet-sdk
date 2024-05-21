use chia_bls::{sign, PublicKey, SecretKey, Signature};
use chia_client::Peer;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzles::standard::{StandardArgs, STANDARD_PUZZLE_HASH};
use chia_puzzles::DeriveSynthetic;
use chia_sdk_signer::RequiredSignature;
use clvm_utils::{CurriedProgram, ToTreeHash};
use clvmr::Allocator;
use hex_literal::hex;
use once_cell::sync::Lazy;

mod wallet_simulator;

pub use wallet_simulator::*;

pub static SECRET_KEY: Lazy<SecretKey> = Lazy::new(|| {
    SecretKey::from_bytes(&hex!(
        "1b72f8ed55860ea5441729c8e36ce1d6f4c8be9bbcf658502a7a0169f55638b9"
    ))
    .unwrap()
});

pub struct TestWallet {
    pub sk: SecretKey,
    pub pk: PublicKey,
    pub puzzle_hash: Bytes32,
    pub coin: Coin,
    pub sim: WalletSimulator,
    pub peer: Peer,
}

impl TestWallet {
    pub async fn new(amount: u64) -> Self {
        let sk = SECRET_KEY.derive_synthetic();
        let pk = sk.public_key();

        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs { synthetic_key: pk },
        }
        .tree_hash()
        .into();

        let coin = sim.generate_coin(puzzle_hash, amount).await.coin;

        Self {
            sk,
            pk,
            puzzle_hash,
            coin,
            sim,
            peer,
        }
    }

    pub async fn submit(&mut self, coin_spends: Vec<CoinSpend>) -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let required_signatures = RequiredSignature::from_coin_spends(
            &mut allocator,
            &coin_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?;

        let mut aggregated_signature = Signature::default();

        for required in required_signatures {
            aggregated_signature += &sign(&self.sk, required.final_message());
        }

        let spend_bundle = SpendBundle::new(coin_spends, aggregated_signature);
        let ack = self.peer.send_transaction(spend_bundle).await?;

        assert_eq!(ack.status, 1);
        assert_eq!(ack.error, None);

        Ok(())
    }
}
