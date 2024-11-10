use std::collections::HashMap;

use chia_bls::{sign, PublicKey, SecretKey, Signature};
use chia_protocol::{CoinSpend, SpendBundle, TransactionAck};
use chia_sdk_client::Peer;
use chia_sdk_signer::{AggSigConstants, RequiredSignature};
use chia_sdk_types::TESTNET11_CONSTANTS;
use clvmr::Allocator;

use crate::SimulatorError;

pub fn sign_transaction(
    coin_spends: &[CoinSpend],
    secret_keys: &[SecretKey],
) -> Result<Signature, SimulatorError> {
    let mut allocator = Allocator::new();

    let required_signatures = RequiredSignature::from_coin_spends(
        &mut allocator,
        coin_spends,
        &AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data),
    )?;

    let key_pairs = secret_keys
        .iter()
        .map(|sk| (sk.public_key(), sk))
        .collect::<HashMap<PublicKey, &SecretKey>>();

    let mut aggregated_signature = Signature::default();

    for required in required_signatures {
        let pk = required.public_key();
        let sk = key_pairs.get(&pk).ok_or(SimulatorError::MissingKey)?;
        aggregated_signature += &sign(sk, required.final_message());
    }

    Ok(aggregated_signature)
}

pub async fn test_transaction_raw(
    peer: &Peer,
    coin_spends: Vec<CoinSpend>,
    secret_keys: &[SecretKey],
) -> anyhow::Result<TransactionAck> {
    let aggregated_signature = sign_transaction(&coin_spends, secret_keys)?;

    Ok(peer
        .send_transaction(SpendBundle::new(coin_spends, aggregated_signature))
        .await?)
}

/// Signs and tests a transaction with the given coin spends and secret keys.
///
/// # Panics
/// Will panic if the transaction could not be submitted or was not successful.
pub async fn test_transaction(peer: &Peer, coin_spends: Vec<CoinSpend>, secret_keys: &[SecretKey]) {
    let ack = test_transaction_raw(peer, coin_spends, secret_keys)
        .await
        .expect("could not submit transaction");

    assert_eq!(ack.error, None);
    assert_eq!(ack.status, 1);
}
