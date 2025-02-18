use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{Coin, IntoJs, IntoRust, LineageProof, PublicKey, Spend};

#[napi(object)]
pub struct Cat {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
    pub asset_id: Uint8Array,
    pub p2_puzzle_hash: Uint8Array,
}

#[napi(object)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Reference<Spend>,
}

#[napi]
pub fn standard_puzzle_hash(synthetic_key: &PublicKey) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::standard_puzzle_hash(synthetic_key.0).js()?)
}

#[napi]
pub fn cat_puzzle_hash(asset_id: Uint8Array, inner_puzzle_hash: Uint8Array) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::cat_puzzle_hash(asset_id.rust()?, inner_puzzle_hash.rust()?).js()?)
}
