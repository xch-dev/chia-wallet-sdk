use pyo3::prelude::*;

use crate::{
    bls::PublicKey,
    clvm::Spend,
    coin::{Coin, LineageProof},
    traits::{IntoPy, IntoRust},
};

#[pyclass(get_all, frozen)]
#[derive(Clone)]
pub struct Cat {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
    pub asset_id: Vec<u8>,
    pub p2_puzzle_hash: Vec<u8>,
}

#[pyclass(get_all, frozen)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
}

#[pyfunction]
pub fn standard_puzzle_hash(synthetic_key: &PublicKey) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::standard_puzzle_hash(synthetic_key.0).py()?)
}

#[pyfunction]
pub fn cat_puzzle_hash(asset_id: Vec<u8>, inner_puzzle_hash: Vec<u8>) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::cat_puzzle_hash(asset_id.rust()?, inner_puzzle_hash.rust()?).py()?)
}
