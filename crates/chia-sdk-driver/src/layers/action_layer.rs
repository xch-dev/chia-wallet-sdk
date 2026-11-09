//! Layers for action layer singletons and the puzzles around them.
//!
//! An [`ActionLayer`] is a singleton inner puzzle that holds a state and can only be spent by
//! running action puzzles whose hashes are in a merkle tree. The `CATalog` registry, the `XCHandles`
//! registry, and the reward distributor are built on it, and each of their actions has a type in
//! this module, such as [`CatalogRegisterAction`] or [`RewardDistributorStakeAction`].
//!
//! This module also contains the layers those apps use alongside the action layer:
//!
//! - [`PrecommitLayer`], which locks a payment until a singleton either accepts it or refunds it.
//! - [`P2DelegatedBySingletonLayer`], which lets a singleton spend coins, such as reserves and
//!   locked assets, by sending them a message.
//! - [`StateSchedulerLayer`], which sends a new state to another singleton after a timestamp.
//! - [`MOfNLayer`], an m-of-n multisig, and [`ConditionsLayer`], which outputs fixed conditions.

#[allow(clippy::module_inception)]
mod action_layer;
mod actions;
mod conditions_layer;
mod m_of_n_layer;
mod p2_delegated_by_singleton_layer;
mod precommit_layer;
mod singleton_action;
mod state_scheduler_layer;

pub use action_layer::*;
pub use actions::*;
pub use conditions_layer::*;
pub use m_of_n_layer::*;
pub use p2_delegated_by_singleton_layer::*;
pub use precommit_layer::*;
pub use singleton_action::*;
pub use state_scheduler_layer::*;
