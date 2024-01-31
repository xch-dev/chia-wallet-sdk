use chia_bls::{sign, Signature};

use chia_protocol::{CoinSpend, SpendBundle};
use clvm_traits::{FromClvm, FromClvmError};
use clvmr::{reduction::EvalErr, Allocator, NodePtr};
use thiserror::Error;

mod required_signature;

pub use required_signature::*;

use crate::{Condition, SecretKeyStore};

/// An error that occurs while trying to sign a coin spend.
#[derive(Debug, Error)]
pub enum SignSpendError {
    /// An error that occurs while trying to calculate the conditions.
    #[error("{0:?}")]
    Eval(#[from] EvalErr),

    /// An error that occurs while attempting to parse the conditions.
    #[error("{0}")]
    Clvm(#[from] FromClvmError),

    /// An error that indicates that a key is missing.
    #[error("missing key")]
    MissingKey,
}

/// Signs each of the required messages in a coin spend.
pub async fn sign_coin_spend(
    sk_store: &impl SecretKeyStore,
    allocator: &mut Allocator,
    coin_spend: &CoinSpend,
    agg_sig_me_extra_data: [u8; 32],
) -> Result<Signature, SignSpendError> {
    let output = coin_spend
        .puzzle_reveal
        .run(allocator, 0, u64::MAX, &coin_spend.solution)?
        .1;

    let conditions: Vec<Condition<NodePtr>> = FromClvm::from_clvm(allocator, output)?;

    let mut aggregate_signature = Signature::default();

    for condition in conditions {
        let Some(required) = RequiredSignature::try_from_condition(
            &coin_spend.coin,
            condition,
            agg_sig_me_extra_data,
        ) else {
            continue;
        };

        let Some(sk) = sk_store.to_secret_key(required.public_key()).await else {
            return Err(SignSpendError::MissingKey);
        };

        aggregate_signature += &sign(&sk, &required.message());
    }

    Ok(aggregate_signature)
}

/// Signs each of the coin spends in a spend bundle.
pub async fn sign_spend_bundle(
    sk_store: &impl SecretKeyStore,
    allocator: &mut Allocator,
    spend_bundle: &SpendBundle,
    agg_sig_me_extra_data: [u8; 32],
) -> Result<Signature, SignSpendError> {
    let mut aggregate_signature = Signature::default();
    for coin_spend in &spend_bundle.coin_spends {
        let signature =
            sign_coin_spend(sk_store, allocator, coin_spend, agg_sig_me_extra_data).await?;
        aggregate_signature += &signature;
    }
    Ok(aggregate_signature)
}
