use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{Coin, IntoJs, IntoRust, LineageProof, Program, PublicKey, Spend};

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

#[napi(object)]
pub struct Nft {
    pub coin: Coin,
    pub lineage_proof: LineageProof,
    pub info: NftInfo,
}

#[napi(object)]
pub struct NftInfo {
    pub launcher_id: Uint8Array,
    pub metadata: Reference<Program>,
    pub metadata_updater_puzzle_hash: Uint8Array,
    pub current_owner: Option<Uint8Array>,
    pub royalty_puzzle_hash: Uint8Array,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Uint8Array,
    pub p2_puzzle: Option<Reference<Program>>,
}

#[napi(object)]
pub struct NftMetadata {
    pub edition_number: BigInt,
    pub edition_total: BigInt,
    pub data_uris: Vec<String>,
    pub data_hash: Option<Uint8Array>,
    pub metadata_uris: Vec<String>,
    pub metadata_hash: Option<Uint8Array>,
    pub license_uris: Vec<String>,
    pub license_hash: Option<Uint8Array>,
}

#[napi(object)]
pub struct NftMint {
    pub metadata: Reference<Program>,
    pub metadata_updater_puzzle_hash: Uint8Array,
    pub p2_puzzle_hash: Uint8Array,
    pub royalty_puzzle_hash: Uint8Array,
    pub royalty_ten_thousandths: u16,
    pub owner: Option<DidOwner>,
}

#[napi(object)]
pub struct DidOwner {
    pub did_id: Uint8Array,
    pub inner_puzzle_hash: Uint8Array,
}

#[napi(object)]
pub struct MintedNfts {
    pub nfts: Vec<Nft>,
    pub parent_conditions: Vec<Reference<Program>>,
}

#[napi]
pub fn standard_puzzle_hash(synthetic_key: &PublicKey) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::standard_puzzle_hash(synthetic_key.0).js()?)
}

#[napi]
pub fn cat_puzzle_hash(asset_id: Uint8Array, inner_puzzle_hash: Uint8Array) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::cat_puzzle_hash(asset_id.rust()?, inner_puzzle_hash.rust()?).js()?)
}
