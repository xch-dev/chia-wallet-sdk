use std::str::FromStr;

use bip39::Mnemonic;
use chia_bls::{sign, PublicKey, SecretKey, Signature};
use chia_client::Peer;
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_puzzles::{
    standard::{StandardArgs, STANDARD_PUZZLE_HASH},
    DeriveSynthetic,
};
use clvm_utils::{CurriedProgram, ToTreeHash};
use clvmr::Allocator;
use once_cell::sync::Lazy;

use crate::{RequiredSignature, SpendContext, WalletSimulator};

const MNEMONIC: &str = "setup update spoil lazy square course ring tell hard eager industry ticket guess amused build reunion woman system cause afraid first material machine morning";

pub static SECRET_KEY: Lazy<SecretKey> =
    Lazy::new(|| SecretKey::from_seed(&Mnemonic::from_str(MNEMONIC).unwrap().to_seed("")));

pub struct TestWallet<'a> {
    pub sk: SecretKey,
    pub pk: PublicKey,
    pub puzzle_hash: Bytes32,
    pub coin: Coin,
    pub ctx: SpendContext<'a>,
    pub sim: WalletSimulator,
    pub peer: Peer,
}

impl<'a> TestWallet<'a> {
    pub async fn new(allocator: &'a mut Allocator, amount: u64) -> Self {
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
        let ctx = SpendContext::new(allocator);

        Self {
            sk,
            pk,
            puzzle_hash,
            coin,
            ctx,
            sim,
            peer,
        }
    }

    pub async fn submit(&mut self) -> anyhow::Result<()> {
        let coin_spends = self.ctx.take_spends();

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
