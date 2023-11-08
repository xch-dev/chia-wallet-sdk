use chia_bls::{sign, Signature};
use chia_protocol::CoinSpend;
use clvm_traits::{Result, ToClvm};
use clvmr::Allocator;

use crate::{evaluate_conditions, KeyPair};

mod partial_signature;
mod required_signature;

pub use partial_signature::*;
pub use required_signature::*;

pub fn sign_coin_spend(
    allocator: &mut Allocator,
    coin_spend: &CoinSpend,
    key_pairs: &[KeyPair],
    agg_sig_me_extra_data: [u8; 32],
) -> Result<PartialSignature> {
    let mut missing_signatures = Vec::new();
    let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
    let solution = coin_spend.solution.to_clvm(allocator)?;
    let conditions = evaluate_conditions(allocator, puzzle, solution)?;

    let signature = conditions
        .into_iter()
        .filter_map(|condition| {
            RequiredSignature::try_from_condition(coin_spend, condition, agg_sig_me_extra_data)
        })
        .filter_map(|required| {
            match key_pairs
                .iter()
                .find(|key_pair| &key_pair.public_key == required.public_key())
            {
                Some(key_pair) => Some((required, key_pair)),
                None => {
                    missing_signatures.push(required);
                    None
                }
            }
        })
        .fold(Signature::default(), |aggregate, (required, key_pair)| {
            aggregate + &sign(&key_pair.secret_key, required.final_message())
        });

    Ok(PartialSignature {
        signature,
        missing_signatures,
    })
}

pub fn sign_coin_spends(
    allocator: &mut Allocator,
    coin_spends: &[CoinSpend],
    key_pairs: &[KeyPair],
    agg_sig_me_extra_data: [u8; 32],
) -> Result<PartialSignature> {
    coin_spends
        .iter()
        .try_fold(PartialSignature::default(), |aggregate, coin_spend| {
            Ok(aggregate
                + sign_coin_spend(allocator, coin_spend, key_pairs, agg_sig_me_extra_data)?)
        })
}
