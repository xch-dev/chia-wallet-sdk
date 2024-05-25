use std::collections::HashMap;

use chia_bls::{sign, PublicKey, SecretKey, Signature};
use chia_client::Peer;
use chia_protocol::{CoinSpend, SpendBundle, TransactionAck};
use chia_sdk_signer::RequiredSignature;
use clvmr::Allocator;
use thiserror::Error;

use crate::Simulator;

#[derive(Debug, Error)]
#[error("missing key")]
pub struct KeyError;

pub async fn test_transaction(
    peer: &Peer,
    coin_spends: Vec<CoinSpend>,
    secret_keys: &[SecretKey],
) -> anyhow::Result<TransactionAck> {
    let mut allocator = Allocator::new();

    let required_signatures =
        RequiredSignature::from_coin_spends(&mut allocator, &coin_spends, Simulator::AGG_SIG_ME)?;

    let key_pairs = HashMap::<PublicKey, &SecretKey>::from_iter(
        secret_keys.iter().map(|sk| (sk.public_key(), sk)),
    );

    let mut aggregated_signature = Signature::default();

    for required in required_signatures {
        let sk = key_pairs.get(&required.public_key()).ok_or(KeyError)?;
        aggregated_signature += &sign(sk, required.final_message());
    }

    Ok(peer
        .send_transaction(SpendBundle::new(coin_spends, aggregated_signature))
        .await?)
}
