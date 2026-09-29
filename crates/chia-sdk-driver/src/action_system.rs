//! A declarative way to build transactions out of high level [`Action`]s.
//!
//! # Lifecycle
//!
//! 1. Compute [`Deltas::from_actions`] and use it to select coins. For each [`Id`], select at
//!    least `output - input` worth of coins, and at least one coin if [`Deltas::is_needed`] is true.
//! 2. Create [`Spends`] with the change puzzle hash and [`Spends::add`] the selected coins,
//!    including any settlement coins (for example, when taking an offer).
//! 3. [`Spends::apply`] the actions. Each action is applied in order and records the coins it
//!    creates and the conditions it needs.
//! 4. [`Spends::prepare`] creates change for every asset, attaches the transaction-wide conditions
//!    (including the reserved fee and settlement payment assertions) to a single spend, and links
//!    the spends together according to the [`Relation`].
//! 5. Spend every item in [`Spends::unspent`] with its p2 puzzle, and pass the results to
//!    [`Spends::spend`], which returns the [`Outputs`]. Alternatively, use
//!    [`Spends::finish_with_keys`] for standard p2 puzzles. A [`SpendableAsset::RevokedCat`] must
//!    be spent with its hidden puzzle, whose hash is returned by [`SpendableAsset::p2_puzzle_hash`].
//!
//! # Revocation
//!
//! Revocable CATs added with [`Spends::add_for_revocation`] are spent with their hidden puzzle. The
//! actions apply to them like any other CAT, so they can be sent, melted with a TAIL, or left alone
//! (in which case the value is returned to the change puzzle hash). Every coin they create is wrapped
//! in the same revocation layer and hinted with its p2 puzzle hash, so revoked value stays revocable.
//!
//! # Ids
//!
//! [`Id::Xch`] refers to XCH, [`Id::Existing`] refers to an asset id (for CATs) or launcher id (for
//! singletons) of coins added to the spends, and [`Id::New`] refers to an asset created by the action
//! at that index in the list passed to [`Spends::apply`].
//!
//! # Guarantees
//!
//! - Change is calculated from the selected coins and the deltas. If the selected coins can't cover
//!   the outputs, [`DriverError::InsufficientFunds`](crate::DriverError::InsufficientFunds) is returned
//!   rather than building a transaction that would be rejected by the mempool.
//! - Payments created from settlement coins are asserted with a nonce unique to the settlement coin,
//!   so that the settlement spends can't be removed from the transaction.
//! - Building the same actions against the same coins always produces the same coin spends.
//! - With [`Relation::AssertConcurrent`] or [`Relation::CoinAnnouncementRing`], the spends can't be
//!   split into separate transactions. With [`Relation::CoinAnnouncementHub`], every spend requires
//!   the first spend, but the first spend can be included without the others.

mod action;
mod asset;
mod deltas;
mod fungible_spends;
mod id;
mod output;
mod relation;
mod singleton_spends;
mod spend_kind;
mod spendable_asset;
mod spends;

#[cfg(test)]
mod tests;

pub use action::*;
pub use asset::*;
pub use deltas::*;
pub use fungible_spends::*;
pub use id::*;
pub use output::*;
pub use relation::*;
pub use singleton_spends::*;
pub use spend_kind::*;
pub use spendable_asset::*;
pub use spends::*;
