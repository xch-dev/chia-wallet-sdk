use std::io;

use chia_bls::{PublicKey, Signature};
use chia_protocol::CoinSpend;
use clvmr::{serde::node_from_bytes, Allocator};
use thiserror::Error;

use crate::{evaluate_conditions, EvaluateConditionsError};

mod partial_signature;
mod required_signature;

pub use partial_signature::*;
pub use required_signature::*;

#[derive(Error, Debug)]
pub enum SignError {
    #[error("{0}")]
    Io(#[from] io::Error),

    #[error("{0}")]
    Eval(#[from] EvaluateConditionsError),
}

pub trait Signer {
    fn has_public_key(&self, public_key: &PublicKey) -> bool;
    fn sign_message(&self, public_key: &PublicKey, message: &[u8]) -> Signature;

    fn sign_coin_spend(
        &self,
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        agg_sig_me_extra_data: [u8; 32],
    ) -> Result<PartialSignature, SignError> {
        let mut missing_signatures = Vec::new();
        let puzzle = node_from_bytes(allocator, coin_spend.puzzle_reveal.as_slice())?;
        let solution = node_from_bytes(allocator, coin_spend.solution.as_slice())?;
        let conditions = evaluate_conditions(allocator, puzzle, solution)?;

        let signature = conditions
            .into_iter()
            .filter_map(|condition| {
                RequiredSignature::try_from_condition(coin_spend, condition, agg_sig_me_extra_data)
            })
            .filter_map(|required| {
                if self.has_public_key(required.public_key()) {
                    Some(required)
                } else {
                    missing_signatures.push(required);
                    None
                }
            })
            .fold(Signature::default(), |aggregate, required| {
                aggregate + &self.sign_message(required.public_key(), &required.final_message())
            });

        Ok(PartialSignature::new(signature, missing_signatures))
    }

    fn sign_coin_spends(
        &self,
        allocator: &mut Allocator,
        coin_spends: &[CoinSpend],
        agg_sig_me_extra_data: [u8; 32],
    ) -> Result<PartialSignature, SignError> {
        coin_spends
            .iter()
            .try_fold(PartialSignature::default(), |aggregate, coin_spend| {
                Ok(aggregate
                    + self.sign_coin_spend(allocator, coin_spend, agg_sig_me_extra_data)?)
            })
    }
}
