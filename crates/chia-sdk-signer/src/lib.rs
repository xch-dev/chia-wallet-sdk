//! Calculates the signatures that a Chia transaction needs.
//!
//! On Chia, a coin spend asks for signatures by outputting `AGG_SIG_*` conditions, such as
//! `AGG_SIG_ME`. This crate runs each puzzle with its solution, collects those conditions,
//! and works out the exact message that has to be signed for each one, so that a wallet
//! can sign a transaction without understanding every puzzle in it.
//!
//! The main entry points are [`RequiredSignature::from_coin_spend`] and
//! [`RequiredSignature::from_coin_spends`]. They return a [`RequiredSignature`] for each
//! signature, which is either:
//!
//! * A [`RequiredBlsSignature`], which is signed with a BLS key and aggregated into the
//!   spend bundle's aggregated signature.
//! * A [`RequiredSecpSignature`], which is requested by the `secp256k1_verify` or
//!   `secp256r1_verify` operators, and is placed in the solution instead.
//!
//! The messages depend on the network, so the [`AggSigConstants`] for the network must be
//! provided. They can be created from the consensus constants, such as
//! [`MAINNET_CONSTANTS`](chia_sdk_types::MAINNET_CONSTANTS) or
//! [`TESTNET11_CONSTANTS`](chia_sdk_types::TESTNET11_CONSTANTS).
//!
//! # Security
//!
//! A signature is only safe to make if you trust the puzzle that requested it. The puzzle
//! reveal of each coin spend is checked against the coin's puzzle hash, and `AGG_SIG_*`
//! conditions that consensus would reject are refused, but you should still check that a
//! transaction does what you expect before signing it.
//!
//! # Example
//!
//! Calculate and sign the required signatures for a coin whose puzzle outputs an
//! `AGG_SIG_ME` condition:
//!
//! ```
//! use chia_bls::{SecretKey, Signature, aggregate_verify, sign};
//! use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program, SpendBundle};
//! use chia_sdk_signer::{AggSigConstants, RequiredSignature};
//! use chia_sdk_types::{Conditions, TESTNET11_CONSTANTS};
//! use clvm_traits::{FromClvm, ToClvm, clvm_quote};
//! use clvm_utils::tree_hash;
//! use clvmr::Allocator;
//!
//! let mut allocator = Allocator::new();
//! let secret_key = SecretKey::from_seed(&[1; 32]);
//! let public_key = secret_key.public_key();
//!
//! // A puzzle that ignores its solution and requires a signature from the key.
//! let conditions = Conditions::new().agg_sig_me(public_key, Bytes::new(b"hello".to_vec()));
//! let puzzle = clvm_quote!(conditions).to_clvm(&mut allocator)?;
//! let puzzle_hash = Bytes32::from(tree_hash(&allocator, puzzle));
//!
//! let coin = Coin::new(Bytes32::new([0; 32]), puzzle_hash, 1);
//! let coin_spend = CoinSpend::new(
//!     coin,
//!     Program::from_clvm(&allocator, puzzle)?,
//!     Program::default(),
//! );
//!
//! let constants = AggSigConstants::from(&*TESTNET11_CONSTANTS);
//! let required =
//!     RequiredSignature::from_coin_spends(&mut allocator, &[coin_spend.clone()], &constants)?;
//! assert_eq!(required.len(), 1);
//!
//! let mut aggregated_signature = Signature::default();
//! let mut signed = Vec::new();
//!
//! for item in required {
//!     let RequiredSignature::Bls(item) = item else {
//!         unreachable!("this puzzle doesn't use secp signatures");
//!     };
//!     assert_eq!(item.public_key, public_key);
//!     aggregated_signature += &sign(&secret_key, item.message());
//!     signed.push((item.public_key, item.message()));
//! }
//!
//! assert!(aggregate_verify(&aggregated_signature, signed));
//! let spend_bundle = SpendBundle::new(vec![coin_spend], aggregated_signature);
//! # let _ = spend_bundle;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![cfg_attr(not(test), warn(missing_docs))]

mod bls;
mod error;
mod required_signature;
mod secp;

pub use bls::*;
pub use error::*;
pub use required_signature::*;
pub use secp::*;
