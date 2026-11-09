//! General purpose helpers used throughout the Chia Wallet SDK.
//!
//! This crate has no knowledge of puzzles or the network. It provides:
//!
//! * [`Address`] and [`Bech32`] for encoding and decoding bech32m strings, such as
//!   `xch1...` wallet addresses, along with the [`Bech32Error`] type.
//! * [`select_coins`], which picks a set of coins whose total covers an amount, and
//!   the [`CoinSelectionError`] type.
//! * [`parse_hex`], which decodes a hex string with or without a `0x` prefix.
//!
//! # Example
//!
//! Decode an address, then select coins to pay an amount to it:
//!
//! ```
//! use chia_protocol::{Bytes32, Coin};
//! use chia_sdk_utils::{Address, select_coins};
//!
//! let address = Address::decode("xch1a0t57qn6uhe7tzjlxlhwy2qgmuxvvft8gnfzmg5detg0q9f3yc3s2apz0h")?;
//! assert_eq!(address.prefix, "xch");
//!
//! let coins = vec![
//!     Coin::new(Bytes32::new([1; 32]), address.puzzle_hash, 300),
//!     Coin::new(Bytes32::new([2; 32]), address.puzzle_hash, 700),
//!     Coin::new(Bytes32::new([3; 32]), address.puzzle_hash, 1000),
//! ];
//!
//! // An exact match is preferred when one exists.
//! let selected = select_coins(coins, 700)?;
//! assert_eq!(selected.len(), 1);
//! assert_eq!(selected[0].amount, 700);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![cfg_attr(not(test), warn(missing_docs))]

mod bech32;
mod coin_selection;
mod hex;

pub use bech32::*;
pub use coin_selection::*;
pub use hex::*;
