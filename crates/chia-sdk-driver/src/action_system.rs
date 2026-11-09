//! A declarative way to build transactions out of high level [`Action`]s.
//!
//! Rather than building each coin spend by hand, you describe what the transaction should do (send
//! XCH, issue a CAT, mint an NFT, pay a fee, and so on) and the action system works out which coins
//! create which outputs, what each p2 puzzle has to output, how much change is left over, and how
//! to tie the spends together. You only have to provide the p2 spends (signatures or otherwise) at
//! the end.
//!
//! # Concepts
//!
//! - [`Action`] is a single high level operation, built with constructors such as [`Action::send`],
//!   [`Action::fee`], [`Action::issue_cat`], [`Action::mint_nft`], and [`Action::create_did`]. Every
//!   action implements [`SpendAction`], which knows how to compute its [`Deltas`] and how to apply
//!   itself to the [`Spends`].
//! - [`Id`] names the asset that an action applies to: [`Id::Xch`], an [`Id::Existing`] asset id or
//!   launcher id, or [`Id::New`] for an asset created by an earlier action in the same list.
//! - [`Deltas`] is the total [`Delta`] (input and output amount) of each asset across the actions,
//!   used for coin selection before anything is spent.
//! - [`Spends`] holds every coin being spent. Fungible assets (XCH and each CAT) are tracked as
//!   [`FungibleSpends`], and each singleton (DID, NFT, or option contract) as [`SingletonSpends`].
//!   Each individual spend has a [`SpendKind`], which is either a [`ConditionsSpend`] (the p2
//!   puzzle outputs arbitrary conditions) or a [`SettlementSpend`] (the coin is locked in the
//!   settlement payments puzzle and can only make notarized payments).
//! - [`Relation`] decides how the conditions spends are linked together, so that the transaction
//!   can't be split apart.
//! - [`Outputs`] lists the coins the transaction creates, including change, keyed by [`Id`].
//!
//! # Fungible spends
//!
//! When an action needs a fungible output, it picks the first spend of that asset that can create
//! it without duplicating an existing coin (a spend can't create two coins with the same puzzle hash
//! and amount). If none can, an ephemeral intermediate coin with the same p2 puzzle hash is created
//! from one of the spends, and is spent in the same transaction to create the output instead. Every
//! CAT spend of the same asset is spent together as a single ring by [`Spends::spend`].
//!
//! # Singleton spends
//!
//! Each singleton has a lineage of spends, where each spend creates the next. Actions modify the
//! last spend in the lineage. When the spends are prepared, the singleton is recreated (or melted)
//! with every pending change applied. Some changes need an extra spend: an NFT applies one
//! metadata update per spend, and a DID whose recovery list, verifications, or metadata change is
//! recreated with its current p2 puzzle hash first, so that wallets can sync the update. In those
//! cases the singleton is recreated with its current p2 puzzle hash and spent again in the same
//! transaction. A settlement spend of an NFT can't emit conditions, so an NFT in a settlement coin
//! is first moved to the intermediate puzzle hash to apply updates. A singleton that no action
//! sends elsewhere is sent to the change puzzle hash.
//!
//! # Fees and change
//!
//! [`Action::fee`] removes the amount from the XCH delta. By default the fee is reserved, which
//! means a `RESERVE_FEE` condition for the total is attached to a spend, so the transaction is
//! invalid unless the fee is actually left over. Once every action is applied, [`Spends::prepare`]
//! computes the change of each fungible asset as the selected amount plus the delta input, minus the
//! delta output, and sends it to the change puzzle hash. Settlement coins return what's left after
//! their payments, and the rest is created by a conditions spend, so there can be two change coins
//! (see [`FungibleSpends::create_change`]). If the selected coins don't cover the outputs,
//! [`DriverError::InsufficientFunds`](crate::DriverError::InsufficientFunds) is returned.
//!
//! The transaction-wide conditions (such as the reserved fee and the assertions that tie the spends
//! together) are attached to a single conditions spend. If there's none, an intermediate coin is
//! created to emit them, or with [`ConditionConfig::return_unplaced_conditions`] they're returned in
//! [`Spends::unplaced_conditions`], so that another spend (such as a vault's delegated spend) can
//! emit them.
//!
//! # Example
//!
//! Send XCH to someone and pay a fee, sending the remainder back as change:
//!
//! ```
//! use chia_protocol::Bytes32;
//! use chia_puzzle_types::Memos;
//! use chia_sdk_driver::{Action, Deltas, Id, Relation, SpendContext, Spends};
//! use chia_sdk_test::Simulator;
//! use indexmap::indexmap;
//!
//! let mut sim = Simulator::new();
//! let mut ctx = SpendContext::new();
//!
//! // A coin worth 1000 mojos, locked with the standard puzzle.
//! let alice = sim.bls(1000);
//! let bob_puzzle_hash = Bytes32::new([1; 32]);
//!
//! let actions = [
//!     Action::send(Id::Xch, bob_puzzle_hash, 600, Memos::None),
//!     Action::fee(100),
//! ];
//!
//! // Coin selection: at least 700 mojos of XCH must be spent.
//! let deltas = Deltas::from_actions(&actions);
//! let xch = deltas.get(&Id::Xch).unwrap();
//! assert_eq!(xch.output - xch.input, 700);
//!
//! let mut spends = Spends::new(alice.puzzle_hash);
//! spends.add(alice.coin);
//!
//! let deltas = spends.apply(&mut ctx, &actions)?;
//!
//! // Creates the change, attaches the fee, and spends the coin with the standard puzzle.
//! let outputs = spends.finish_with_keys(
//!     &mut ctx,
//!     &deltas,
//!     Relation::None,
//!     &indexmap! { alice.puzzle_hash => alice.pk },
//! )?;
//!
//! sim.spend_coins(ctx.take(), &[alice.sk])?;
//!
//! assert_eq!(outputs.xch.len(), 2);
//! assert_eq!(outputs.xch[0].puzzle_hash, bob_puzzle_hash);
//! assert_eq!(outputs.xch[1].puzzle_hash, alice.puzzle_hash);
//! assert_eq!(outputs.xch[1].amount, 300);
//! assert_eq!(outputs.reserved_fee, 100);
//! # Ok::<(), anyhow::Error>(())
//! ```
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
//! - Payments made by settlement coins are asserted by a conditions spend, so that the settlement
//!   spends can't be removed from the transaction. Change and other coins created from a settlement
//!   coin are notarized with its coin id as the nonce. Settle actions use the nonce they're given, so
//!   [`Spends::apply`] and [`Spends::prepare`] return
//!   [`DriverError::DuplicateSettlementPayment`](crate::DriverError::DuplicateSettlementPayment) if
//!   two settlement coins with the same puzzle hash would make the same notarized payment, since a
//!   single assertion would be satisfied by either of them.
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
