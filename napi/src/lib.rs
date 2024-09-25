#![allow(missing_debug_implementations)]
#![allow(missing_copy_implementations)]
#![allow(clippy::needless_pass_by_value)]

#[macro_use]
extern crate napi_derive;

mod clvm;
mod coin;
mod coin_spend;
mod lineage_proof;
mod nft;
mod nft_mint;
mod spend;
mod traits;

pub use clvm::*;
pub use coin::*;
pub use coin_spend::*;
pub use lineage_proof::*;
pub use nft::*;
pub use nft_mint::*;
pub use spend::*;

use traits::{IntoJs, IntoRust};

#[napi]
pub fn test_roundtrip(value: napi::bindgen_prelude::BigInt) -> napi::bindgen_prelude::BigInt {
    let num: num_bigint::BigInt = value.into_rust().unwrap();
    num.into_js().unwrap()
}
