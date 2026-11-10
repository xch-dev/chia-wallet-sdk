//! Curried arguments, solutions, and bytecode for the puzzles used by the SDK.
//!
//! Types ending in `Args` are the curried arguments of a puzzle and implement [`Mod`](crate::Mod),
//! so you can compute the puzzle hash with [`Mod::curry_tree_hash`](crate::Mod::curry_tree_hash).
//! Types ending in `Solution` are the solutions passed when spending the puzzle. Both convert to
//! and from CLVM with [`ToClvm`](clvm_traits::ToClvm) and [`FromClvm`](clvm_traits::FromClvm).
//!
//! Puzzles that don't take curried arguments are represented by marker structs (such as
//! [`SettlementPayment`] and [`SingletonLauncher`]) that only implement [`Mod`](crate::Mod).
//! The bytecode and hash of puzzles that aren't in the [`chia_puzzles`] crate are exported as
//! constants, such as [`P2_CURRIED_PUZZLE`] and [`P2_CURRIED_PUZZLE_HASH`].
//!
//! This includes the building blocks of MIPS (Managed Inner Puzzle Spec, CHIP-0043) vaults:
//! the `1_of_n`, `m_of_n`, and `n_of_n` puzzles that combine members, member puzzles for each
//! kind of key (such as [`BlsMember`] and [`K1Member`]), and restrictions that limit what
//! a member can do (such as [`Timelock`]).
//!
//! Other puzzle families include CAT makers (such as [`DefaultCatMakerArgs`]), which compute the
//! puzzle hash of an asset from an inner puzzle hash, and the Secure the Mint pre-launcher
//! ([`SecureTheMintLauncherArgs`]), which commits to an NFT before it's minted. Some families are
//! behind feature flags:
//!
//! - `chip-0008`: The splitter puzzle (`SplitterArgs`).
//! - `chip-0021`: The NFT fusion puzzles (`FusionSingletonArgs` and `P2FusionArgs`).
//! - `chip-0035`: The `DataLayer` puzzles (such as `DelegationLayerArgs`).
//! - `chip-0037`: The EIP-712 puzzles (such as `P2Eip712MessageArgs`).
//! - `chip-0052`: The partial offer puzzle (`PartialOfferArgs`).
//! - `action-layer`: The action layer puzzle (`ActionLayerArgs`) and its actions.
//! - `auction`: The English NFT auction actions (such as `AuctionBidActionArgs`).
//!
//! ## Example
//!
//! ```rust
//! # use chia_protocol::Bytes32;
//! # use chia_sdk_types::{
//! #     Mod,
//! #     puzzles::{P2CurriedArgs, P2CurriedSolution, P2_CURRIED_PUZZLE_HASH},
//! # };
//! # use clvm_utils::TreeHash;
//! // Lock a coin so that it can only be spent by revealing a puzzle with a specific hash.
//! let args = P2CurriedArgs::new(Bytes32::new([1; 32]));
//! let puzzle_hash = args.curry_tree_hash();
//!
//! assert_eq!(P2CurriedArgs::mod_hash(), P2_CURRIED_PUZZLE_HASH);
//! assert_ne!(puzzle_hash, P2_CURRIED_PUZZLE_HASH);
//! # let _ = P2CurriedSolution::new(TreeHash::new([2; 32]), ());
//! ```

mod augmented_condition;
mod cat_makers;
mod everything_with_singleton;
mod mips;
mod mods;
mod option_contract;
mod p2_curried;
mod p2_delegated_conditions;
mod p2_one_of_many;
mod p2_parent;
mod p2_singleton;
mod revocation;
mod secure_the_mint;

pub use augmented_condition::*;
pub use cat_makers::*;
pub use everything_with_singleton::*;
pub use mips::*;
pub use mods::*;
pub use option_contract::*;
pub use p2_curried::*;
pub use p2_delegated_conditions::*;
pub use p2_one_of_many::*;
pub use p2_parent::*;
pub use p2_singleton::*;
pub use revocation::*;
pub use secure_the_mint::*;

#[cfg(feature = "pooling")]
mod pooling;

#[cfg(feature = "pooling")]
pub use pooling::*;

#[cfg(feature = "chip-0008")]
mod splitter;

#[cfg(feature = "chip-0008")]
pub use splitter::*;

#[cfg(feature = "chip-0021")]
mod fusion;

#[cfg(feature = "chip-0021")]
pub use fusion::*;

#[cfg(feature = "chip-0052")]
mod partial_offer;

#[cfg(feature = "chip-0052")]
pub use partial_offer::*;

#[cfg(feature = "auction")]
mod auction;

#[cfg(feature = "auction")]
pub use auction::*;

#[cfg(feature = "chip-0035")]
mod datalayer;

#[cfg(feature = "chip-0035")]
pub use datalayer::*;

#[cfg(feature = "chip-0037")]
mod eip712;
#[cfg(feature = "chip-0037")]
mod p2_controller_puzzle;
#[cfg(feature = "chip-0037")]
mod p2_eip712_message;

#[cfg(feature = "chip-0037")]
pub use eip712::*;
#[cfg(feature = "chip-0037")]
pub use p2_controller_puzzle::*;
#[cfg(feature = "chip-0037")]
pub use p2_eip712_message::*;

#[cfg(feature = "action-layer")]
mod action_layer;

#[cfg(feature = "action-layer")]
pub use action_layer::*;

#[cfg(test)]
mod tests {
    /// Asserts that the tree hash of the serialized puzzle `$puzzle` equals `$puzzle_hash`.
    /// Must be used in a function that returns a [`Result`].
    #[macro_export]
    macro_rules! assert_puzzle_hash {
        ($puzzle:ident => $puzzle_hash:ident) => {
            let mut a = clvmr::Allocator::new();
            let ptr = clvmr::serde::node_from_bytes(&mut a, &$puzzle)?;
            let hash = clvm_utils::tree_hash(&mut a, ptr);
            assert_eq!($puzzle_hash, hash);
        };
    }
}
