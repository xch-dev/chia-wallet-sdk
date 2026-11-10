use std::collections::HashMap;

use chia_bls::{PublicKey, SecretKey, Signature, sign};
use chia_protocol::CoinSpend;
use chia_sdk_signer::{AggSigConstants, RequiredSignature};
use chia_sdk_types::TESTNET11_CONSTANTS;
use clvmr::Allocator;

use crate::SimulatorError;

/// Signs coin spends for the [`Simulator`](crate::Simulator) with the given BLS secret keys.
///
/// This finds the BLS signatures that the coin spends require, using the testnet11
/// `AGG_SIG_ME` additional data, signs each one with the matching secret key, and returns the
/// aggregate signature. Required secp256k1 and secp256r1 signatures are skipped, so they must be
/// handled separately. If no signatures are required, this returns the default (identity)
/// signature.
///
/// The [`FullNodeSimulator`](crate::FullNodeSimulator) uses different additional data (see
/// [`FullNodeSimulator::get_aggsig_additional_data`](crate::FullNodeSimulator::get_aggsig_additional_data)),
/// so signatures from this function aren't valid there.
///
/// # Errors
///
/// Returns [`SimulatorError::MissingKey`] if no secret key matches a required public key, or
/// [`SimulatorError::Signer`] if the required signatures can't be calculated.
///
/// # Example
///
/// ```
/// use chia_protocol::{Bytes, CoinSpend, SpendBundle};
/// use chia_sdk_test::{Simulator, sign_transaction, to_program, to_puzzle};
/// use chia_sdk_types::conditions::AggSigMe;
///
/// let mut sim = Simulator::new();
/// let alice = sim.bls(0);
///
/// // A coin whose puzzle requires a signature from Alice.
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
/// let coin = sim.new_coin(puzzle_hash, 1);
/// let solution = to_program([AggSigMe::new(alice.pk, Bytes::from(b"hello".to_vec()))])?;
/// let coin_spends = vec![CoinSpend::new(coin, puzzle_reveal, solution)];
///
/// let signature = sign_transaction(&coin_spends, &[alice.sk])?;
/// sim.new_transaction(SpendBundle::new(coin_spends, signature))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
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
