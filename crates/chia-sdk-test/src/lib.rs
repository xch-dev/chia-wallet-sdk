//! Tools for testing Chia wallet code against a simulated blockchain that runs in memory.
//!
//! The main type is [`Simulator`], a lightweight in-memory coin set. You can create coins out of
//! thin air, spend them with real CLVM and BLS signature validation, and then inspect the
//! resulting coin states. Every successful transaction is placed in its own block, so tests are
//! fast and fully deterministic for a given [`SimulatorConfig::seed`].
//!
//! The crate also provides:
//!
//! - Deterministic key pairs for tests: [`BlsPair`], [`BlsPairWithCoin`], [`K1Pair`], and [`R1Pair`].
//! - [`FullNodeSimulator`], a richer simulator that models a mempool, farming, reverts, and reorgs,
//!   and answers queries in the same shape as the full node RPC.
//! - Helpers for signing ([`sign_transaction`]), debugging announcements ([`debug_announcements`]),
//!   measuring CLVM cost ([`Benchmark`]), and converting values to CLVM ([`to_program`] and
//!   [`to_puzzle`]).
//!
//! # Feature flags
//!
//! - `serde`: Derives `serde` traits for simulator data. Enables `Simulator::serialize` and
//!   `Simulator::deserialize`, and the `FullNodeSimulator::dump_state` and
//!   `FullNodeSimulator::restore_state` methods.
//! - `full-node-simulator-http`: Implies `serde`. Adds `FullNodeSimulatorServer`, which serves a
//!   [`FullNodeSimulator`] over a local HTTP server that speaks the full node RPC protocol.
//! - `peer-simulator`: Adds `PeerSimulator`, which serves a [`Simulator`] over the Chia wallet
//!   peer protocol on a local websocket, so you can test code that uses `chia-sdk-client`.
//!
//! # Quick start
//!
//! Fund a standard BLS key, spend its coin to create a new coin, and check the result:
//!
//! ```
//! use chia_protocol::{CoinSpend, Program};
//! use chia_puzzle_types::standard::{StandardArgs, StandardSolution};
//! use chia_sdk_test::{Simulator, to_program};
//! use chia_sdk_types::{
//!     Conditions, Mod,
//!     conditions::{CreateCoin, Memos},
//! };
//! use clvm_utils::CurriedProgram;
//!
//! let mut sim = Simulator::new();
//!
//! // Create a key pair along with a 1,000 mojo coin locked to its standard puzzle hash.
//! let alice = sim.bls(1_000);
//!
//! // The standard puzzle, curried with Alice's public key.
//! let puzzle_reveal = to_program(CurriedProgram {
//!     program: Program::from(StandardArgs::mod_reveal().to_vec()),
//!     args: StandardArgs::new(alice.pk),
//! })?;
//!
//! // Send 900 mojos back to Alice. The remaining 100 mojos are left as a fee.
//! let conditions =
//!     Conditions::new().with(CreateCoin::new(alice.puzzle_hash, 900, Memos::None));
//! let solution = to_program(StandardSolution::from_conditions(conditions))?;
//!
//! // Sign the spend with Alice's secret key, validate it, and add it to a new block.
//! sim.spend_coins(
//!     vec![CoinSpend::new(alice.coin, puzzle_reveal, solution)],
//!     &[alice.sk],
//! )?;
//!
//! assert_eq!(sim.height(), 1);
//! assert!(sim.coin_state(alice.coin.coin_id()).unwrap().spent_height.is_some());
//!
//! let unspent = sim.unspent_coins(alice.puzzle_hash, false);
//! assert_eq!(unspent.len(), 1);
//! assert_eq!(unspent[0].amount, 900);
//! # Ok::<(), anyhow::Error>(())
//! ```

#![cfg_attr(not(test), warn(missing_docs))]

mod announcements;
mod benchmark;
mod error;
mod full_node_simulator;
mod key_pairs;
mod simulator;
mod spend_bundle_validation;
mod transaction;
mod validate_clvm_and_signature;

#[cfg(feature = "chip-0057")]
pub mod silent_payments;

pub use announcements::*;
pub use benchmark::*;
pub use error::*;
pub use full_node_simulator::*;
pub use key_pairs::*;
pub use simulator::*;
pub use transaction::*;

#[cfg(feature = "full-node-simulator-http")]
mod full_node_simulator_http;

#[cfg(feature = "full-node-simulator-http")]
pub use full_node_simulator_http::*;

pub(crate) use validate_clvm_and_signature::validate_clvm_and_signature;

#[cfg(feature = "peer-simulator")]
mod peer_simulator;

#[cfg(feature = "peer-simulator")]
pub use peer_simulator::*;

use chia_protocol::{Bytes32, Program};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::Allocator;

/// Converts any value that implements [`ToClvm`] into a serialized [`Program`].
///
/// This is useful for building solutions (and puzzle reveals) without managing an [`Allocator`].
///
/// # Errors
///
/// Returns an error if the value can't be converted to CLVM.
///
/// # Example
///
/// ```
/// use chia_sdk_test::to_program;
///
/// // The list `(1 2)`.
/// let program = to_program([1, 2])?;
/// assert_eq!(program.to_vec(), vec![0xff, 0x01, 0xff, 0x02, 0x80]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn to_program(value: impl ToClvm<Allocator>) -> anyhow::Result<Program> {
    let mut allocator = Allocator::new();
    let ptr = value.to_clvm(&mut allocator)?;
    Ok(Program::from_clvm(&allocator, ptr)?)
}

/// Converts a value into a puzzle, returning both its tree hash and its serialized [`Program`].
///
/// The tree hash is the puzzle hash that coins locked to the puzzle use, and the program is the
/// puzzle reveal needed to spend them.
///
/// # Errors
///
/// Returns an error if the value can't be converted to CLVM.
///
/// # Example
///
/// The puzzle `1` returns its solution unchanged, so it can be spent with any list of
/// conditions and needs no signature:
///
/// ```
/// use chia_protocol::CoinSpend;
/// use chia_sdk_test::{Simulator, to_program, to_puzzle};
/// use chia_sdk_types::conditions::{CreateCoin, Memos};
/// use clvmr::NodePtr;
///
/// let mut sim = Simulator::new();
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
/// let coin = sim.new_coin(puzzle_hash, 1);
///
/// let solution = to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 1, Memos::None)])?;
/// sim.spend_coins(vec![CoinSpend::new(coin, puzzle_reveal, solution)], &[])?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn to_puzzle(value: impl ToClvm<Allocator>) -> anyhow::Result<(Bytes32, Program)> {
    let mut allocator = Allocator::new();
    let ptr = value.to_clvm(&mut allocator)?;
    let puzzle_reveal = Program::from_clvm(&allocator, ptr)?;
    let puzzle_hash = tree_hash(&allocator, ptr);
    Ok((puzzle_hash.into(), puzzle_reveal))
}

/// Asserts that the result of a spend matches the expectation `to_pass`.
///
/// Pass `true` if the spend should succeed, or `false` if it should fail with any
/// [`SimulatorError`]. This makes it easy to write a test that runs both a valid and an invalid
/// version of the same spend.
///
/// # Panics
///
/// Panics if `to_pass` is `true` and `result` is an error (the message includes the error), or
/// if `to_pass` is `false` and `result` is `Ok`.
///
/// # Example
///
/// ```
/// use chia_protocol::CoinSpend;
/// use chia_sdk_test::{Simulator, expect_spend, to_program, to_puzzle};
/// use chia_sdk_types::conditions::{CreateCoin, Memos};
/// use clvmr::NodePtr;
///
/// let mut sim = Simulator::new();
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
/// let coin = sim.new_coin(puzzle_hash, 1);
///
/// // Creating more value than the coin holds is invalid.
/// let solution = to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 2, Memos::None)])?;
/// let spend = CoinSpend::new(coin, puzzle_reveal.clone(), solution);
/// expect_spend(sim.spend_coins(vec![spend], &[]), false);
///
/// let solution = to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 1, Memos::None)])?;
/// let spend = CoinSpend::new(coin, puzzle_reveal, solution);
/// expect_spend(sim.spend_coins(vec![spend], &[]), true);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn expect_spend<T>(result: Result<T, SimulatorError>, to_pass: bool) {
    if let Err(error) = result {
        assert!(!to_pass, "Expected spend to pass, but got {error}");
    } else if !to_pass {
        panic!("Expected spend to fail");
    }
}
