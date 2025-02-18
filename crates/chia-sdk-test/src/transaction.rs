use std::collections::HashMap;

use chia_bls::{sign, PublicKey, SecretKey, Signature};
use chia_protocol::CoinSpend;
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
        let RequiredSignature::Bls(required) = required else {
            continue;
        };
        let pk = required.public_key;
        let sk = key_pairs.get(&pk).ok_or(SimulatorError::MissingKey)?;
        aggregated_signature += &sign(sk, required.message());
    }

    Ok(aggregated_signature)
}
