use pyo3::prelude::*;

use crate::{
    bls::PublicKey,
    traits::{IntoPy, IntoRust},
};

#[pyfunction]
pub fn standard_puzzle_hash(synthetic_key: &PublicKey) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::standard_puzzle_hash(synthetic_key.0).py()?)
}

#[pyfunction]
pub fn cat_puzzle_hash(asset_id: Vec<u8>, inner_puzzle_hash: Vec<u8>) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::cat_puzzle_hash(asset_id.rust()?, inner_puzzle_hash.rust()?).py()?)
}
