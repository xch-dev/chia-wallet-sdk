//! High-level primitives for singletons built on the action layer (enabled by the `action-layer` feature).
//!
//! An action-layer singleton keeps a small piece of state in its inner puzzle and is spent by running
//! one or more *actions*: curried puzzles whose hashes are committed to in a Merkle tree. Each action
//! receives the current state, returns the new state, and outputs conditions. A finalizer then recreates
//! the singleton with the final state. See [`ActionLayer`](crate::ActionLayer) for the layer itself.
//!
//! The apps in this module are:
//!
//! - [`CatalogRegistry`]: the `CATalog` registry, which registers CATs and mints an NFT for each one.
//! - [`XchandlesRegistry`]: the `XCHandles` registry, which registers, extends, updates, and expires handles.
//! - [`RewardDistributor`]: a singleton that distributes CAT rewards from a [`Reserve`] to entries,
//!   which are either managed by a manager singleton or created by staking NFTs or CATs.
//!
//! Supporting primitives:
//!
//! - [`Slot`]: a 0-amount coin that stores one value for its controller singleton. Slots let a
//!   registry keep an unbounded amount of data (such as a sorted linked list of registered assets)
//!   outside of its own puzzle.
//! - [`PrecommitCoin`]: a CAT that commits to a value (such as a handle and its secret) and
//!   a payment before a registration, so the registration can't be front-run.
//! - [`Reserve`]: a CAT that holds funds on behalf of a controller singleton.
//! - [`UniquenessPrelauncher`]: a coin that creates a singleton launcher whose id is tied to a value.
//! - [`StateScheduler`]: a singleton that pushes a schedule of new states to another singleton.
//! - [`MedievalVault`]: an m-of-n BLS multisig singleton that can send approval messages to the other apps.
//! - [`CatMaker`](crate::CatMaker): puzzles that compute the CAT (or XCH) puzzle hash used for payments.
//!
//! The `launch_*` functions (such as [`launch_catalog_registry`]) launch these singletons from an
//! [`Offer`](crate::Offer).

mod catalog_registry;
mod catalog_registry_info;
mod catalog_registry_prefix;
mod launch_drivers;
mod medieval_vault;
mod medieval_vault_info;
mod precommit_coin;
mod reserve;
mod reward_distributor;
mod reward_distributor_info;
mod reward_distributor_prefix;
mod slot;
mod state_scheduler;
mod state_scheduler_info;
mod uniqueness_prelauncher;
mod xchandles_registry;
mod xchandles_registry_info;
mod xchandles_registry_prefix;

pub use catalog_registry::*;
pub use catalog_registry_info::*;
pub use catalog_registry_prefix::*;
pub use launch_drivers::*;
pub use medieval_vault::*;
pub use medieval_vault_info::*;
pub use precommit_coin::*;
pub use reserve::*;
pub use reward_distributor::*;
pub use reward_distributor_info::*;
pub use reward_distributor_prefix::*;
pub use slot::*;
pub use state_scheduler::*;
pub use state_scheduler_info::*;
pub use uniqueness_prelauncher::*;
pub use xchandles_registry::*;
pub use xchandles_registry_info::*;
pub use xchandles_registry_prefix::*;
