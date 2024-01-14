use std::{future::Future, io};

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

#[derive(Error, Debug)]
pub enum SignError {
    #[error("{0}")]
    Io(#[from] io::Error),

    #[error("{0}")]
    Clvm(#[from] FromClvmError),

    #[error("{0}")]
    Eval(#[from] EvalErr),

    #[error("incomplete signature")]
    IncompleteSignature,
}

pub trait KeyStore: Send {
    fn public_key(&self, index: u32) -> impl Future<Output = PublicKey> + Send;

    fn public_keys(&self) -> impl Future<Output = Vec<PublicKey>> + Send;

    fn derive_to_index(&mut self, index: u32) -> impl Future<Output = ()> + Send;
}

pub trait Signer: Send + Sync {
    fn secret_key(&self, index: u32) -> impl Future<Output = SecretKey> + Send;

    fn sign_message(
        &self,
        public_key: &PublicKey,
        message: &[u8],
    ) -> impl Future<Output = Signature> + Send;

    fn sign_coin_spend(
        &self,
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        agg_sig_me_extra_data: [u8; 32],
    ) -> impl Future<Output = Result<Signature, SignError>> + Send {
        async move {
            let output = coin_spend
                .puzzle_reveal
                .run(allocator, 0, u64::MAX, &coin_spend.solution)?
                .1;

            let conditions: Vec<Condition<NodePtr>> = FromClvm::from_clvm(allocator, output)?;

            let mut aggregate_signature = Signature::default();

            for condition in conditions {
                let Some(required) = RequiredSignature::try_from_condition(
                    coin_spend,
                    condition,
                    agg_sig_me_extra_data,
                ) else {
                    continue;
                };

                let signature = self
                    .sign_message(required.public_key(), &required.final_message())
                    .await;

                aggregate_signature += &signature;
            }

            Ok(aggregate_signature)
        }
    }
}
