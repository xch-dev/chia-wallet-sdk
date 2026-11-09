//! Standard types for working with Chia puzzles and conditions.
//!
//! This crate contains the low-level building blocks used by the rest of the Chia Wallet SDK. It
//! doesn't know how to spend specific kinds of coins (that's the job of `chia-sdk-driver`), but it
//! defines the data that puzzles and spends are made of.
//!
//! ## Conditions
//!
//! When a puzzle runs, it outputs a list of conditions, which tell the blockchain what the spend
//! does (such as creating coins, requiring signatures, or asserting timelocks). The [`Condition`]
//! enum has a variant for each known condition, with a strongly typed struct for each one in the
//! [`conditions`] module. The [`Conditions`] type is a builder for lists of conditions, which is
//! what you usually pass to a puzzle's spend. There are also helpers like [`announcement_id`],
//! [`MessageFlags`], and [`payment_assertion`] for computing the values that go into them.
//!
//! ## Puzzles and curried arguments
//!
//! Many puzzles are built by currying arguments into a shared program (the "mod"). The
//! [`puzzles`] module contains types for the curried arguments (named `*Args`) and solutions
//! (named `*Solution`) of the puzzles used by the SDK, along with the bytecode and hash of
//! puzzles that aren't in the [`chia_puzzles`] crate.
//!
//! The [`Mod`] trait connects an arguments type to its mod, so that [`Mod::curry_tree_hash`] can
//! compute the puzzle hash of the curried puzzle directly from the arguments. This is much faster
//! than allocating the whole puzzle, and is how most puzzle hashes in the SDK are computed.
//!
//! ```rust
//! # use chia_protocol::Bytes32;
//! # use chia_puzzle_types::{cat::CatArgs, standard::StandardArgs};
//! # use chia_bls::PublicKey;
//! # use chia_sdk_types::Mod;
//! let p2_puzzle_hash = StandardArgs::new(PublicKey::default()).curry_tree_hash();
//!
//! // The CAT puzzle wraps an inner puzzle, so it's curried with the inner puzzle hash.
//! let asset_id = Bytes32::new([1; 32]);
//! let cat_puzzle_hash = CatArgs::curry_tree_hash(asset_id, p2_puzzle_hash);
//!
//! assert_ne!(cat_puzzle_hash, p2_puzzle_hash);
//! ```
//!
//! ## Running and compiling puzzles
//!
//! [`run_puzzle`] and [`run_puzzle_with_cost`] run a puzzle with a solution, which is useful for
//! parsing a coin spend's conditions. [`compile_chialisp`](fn@compile_chialisp) and
//! [`compile_rue`](fn@compile_rue) (and the [`compile_chialisp!`] and [`compile_rue!`] macros,
//! which implement [`Mod`] for an arguments type) compile source files at runtime, which is
//! useful for prototyping and tests.
//!
//! ## Other utilities
//!
//! - [`MerkleTree`] builds merkle trees and [`MerkleProof`]s in the format used by puzzles such as
//!   [`P2OneOfManyArgs`](puzzles::P2OneOfManyArgs) and the MIPS `1_of_n` and `m_of_n` puzzles.
//! - [`MAINNET_CONSTANTS`] and [`TESTNET11_CONSTANTS`] are the consensus constants for each
//!   network, and [`default_constants`] builds constants for custom networks (such as
//!   simulators).
//!
//! ## Features
//!
//! - `chip-0035`: `DataLayer` puzzles from CHIP-0035, such as `DelegationLayerArgs` and
//!   `WriterLayerArgs`.
//! - `chip-0037`: Puzzles for signing spends with Ethereum wallets using EIP-712 messages, from
//!   CHIP-0037, such as `P2Eip712MessageArgs` and `P2ControllerPuzzleArgs`.
//! - `action-layer`: The action layer puzzle (`ActionLayerArgs`) and related puzzles.

#![cfg_attr(not(test), warn(missing_docs))]

pub mod puzzles;

mod condition;
mod constants;
mod load_clvm;
mod merkle_tree;
mod payment_assertion;
mod puzzle_mod;
mod run_puzzle;

pub use condition::*;
pub use constants::*;
pub use load_clvm::*;
pub use merkle_tree::*;
pub use payment_assertion::*;
pub use puzzle_mod::*;
pub use run_puzzle::*;

#[doc(hidden)]
pub mod __internals {
    pub use clvm_utils::TreeHash;
}
