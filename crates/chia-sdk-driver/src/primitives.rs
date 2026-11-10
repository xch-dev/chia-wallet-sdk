//! Primitives are high level types for the assets and coins that live on the Chia blockchain.
//!
//! Each primitive wraps a [`Coin`](chia_protocol::Coin) along with the information needed to
//! construct its outer puzzle, such as a [`Cat`], [`Did`], [`Nft`], or [`Vault`]. They are built
//! on top of [`Layer`](crate::Layer)s, and provide methods to create, spend, and parse them, so you
//! don't need to construct puzzles and solutions by hand.
//!
//! Most primitives leave the inner (p2) puzzle up to you, so they can be owned by any puzzle such
//! as the [`StandardLayer`](crate::StandardLayer) or a vault. Children can typically be parsed from
//! the parent coin spend (for example, with [`Cat::parse_children`] or [`Nft::parse_child`]), which
//! is how wallets discover and sync coins.

mod bulletin;
mod cat;
mod cat_maker;
mod clawback;
mod clawback_v2;
mod did;
mod intermediate_launcher;
mod launcher;
mod mips;
mod nft;
mod option;
mod secure_the_bag;
mod singleton;
mod streamed_asset;
mod vault;

pub use bulletin::*;
pub use cat::*;
pub use cat_maker::*;
pub use clawback::*;
pub use clawback_v2::*;
pub use did::*;
pub use intermediate_launcher::*;
pub use launcher::*;
pub use mips::*;
pub use nft::*;
pub use option::*;
pub use secure_the_bag::*;
pub use singleton::*;
pub use streamed_asset::*;
pub use vault::*;

#[cfg(feature = "chip-0021")]
mod fusion;

#[cfg(feature = "chip-0021")]
pub use fusion::*;

#[cfg(feature = "chip-0052")]
mod partial_offer;

#[cfg(feature = "chip-0052")]
pub use partial_offer::*;

#[cfg(feature = "pooling")]
mod plot_nft;

#[cfg(feature = "pooling")]
pub use plot_nft::*;

#[cfg(feature = "auction")]
mod auction;

#[cfg(feature = "auction")]
pub use auction::*;

#[cfg(feature = "chip-0035")]
mod datalayer;

#[cfg(feature = "chip-0035")]
pub use datalayer::*;

#[cfg(feature = "action-layer")]
mod action_layer;

#[cfg(feature = "action-layer")]
pub use action_layer::*;

#[cfg(feature = "action-layer")]
mod p2_parent_coin;

#[cfg(feature = "action-layer")]
pub use p2_parent_coin::*;
