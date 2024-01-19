use std::future::Future;

use chia_bls::{PublicKey, SecretKey, Signature};

use chia_protocol::CoinSpend;
use clvm_traits::{FromClvm, FromClvmError};
use clvmr::{reduction::EvalErr, Allocator, NodePtr};
use thiserror::Error;

mod required_signature;
mod synthetic_key_store;

pub use required_signature::*;
pub use synthetic_key_store::*;

use crate::Condition;

/// An error that occurs while trying to sign a coin spend.
#[derive(Debug, Error)]
pub enum SignSpendError {
    /// An error that occurs while trying to calculate the conditions.
    #[error("{0:?}")]
    Eval(#[from] EvalErr),

    /// An error that occurs while attempting to parse the conditions.
    #[error("{0}")]
    Clvm(#[from] FromClvmError),
}

/// A key store is used to manage sequential key derivations used in a wallet.
pub trait KeyStore: Send {
    /// Gets the public key at a given index, or panics if it hasn't been derived.
    fn public_key(&self, index: u32) -> impl Future<Output = PublicKey> + Send;

    /// Gets a list of all derived public keys.
    fn public_keys(&self) -> impl Future<Output = Vec<PublicKey>> + Send;

    /// Derives keys to a given index.
    fn derive_to_index(&mut self, index: u32) -> impl Future<Output = ()> + Send;
}

/// Responsible for signing messages.
pub trait Signer: Send + Sync {
    /// Gets the secret key at a given index, or panics if it hasn't been derived.
    fn secret_key(&self, index: u32) -> impl Future<Output = SecretKey> + Send;

    /// Signs a message with the corresponding private key.
    fn sign_message(
        &self,
        public_key: &PublicKey,
        message: &[u8],
    ) -> impl Future<Output = Signature> + Send;

    /// Signs each of the required messages in a coin spend.
    fn sign_coin_spend(
        &self,
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        agg_sig_me_extra_data: [u8; 32],
    ) -> impl Future<Output = Result<Signature, SignSpendError>> + Send {
        async move {
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

                let signature = self
                    .sign_message(required.public_key(), &required.message())
                    .await;

                aggregate_signature += &signature;
            }

            Ok(aggregate_signature)
        }
    }
}
