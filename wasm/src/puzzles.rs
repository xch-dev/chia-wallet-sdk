use wasm_bindgen::prelude::*;

use crate::{
    bls::PublicKey,
    clvm::Spend,
    coin::{Coin, LineageProof},
    traits::{IntoJs, IntoRust},
};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Cat {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
    pub asset_id: Vec<u8>,
    pub p2_puzzle_hash: Vec<u8>,
}

#[wasm_bindgen(getter_with_clone)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
}

#[wasm_bindgen]
pub fn standard_puzzle_hash(synthetic_key: &PublicKey) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::standard_puzzle_hash(synthetic_key.0).js()?)
}

#[wasm_bindgen]
pub fn cat_puzzle_hash(asset_id: Vec<u8>, inner_puzzle_hash: Vec<u8>) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::cat_puzzle_hash(asset_id.rust()?, inner_puzzle_hash.rust()?).js()?)
}
