use pyo3::prelude::*;

use crate::traits::{IntoJs, IntoRust};

#[pyclass]
pub struct AddressInfo {
    pub puzzle_hash: Vec<u8>,
    pub prefix: String,
}

#[pymethods]
impl AddressInfo {
    #[new]
    pub fn new(puzzle_hash: Vec<u8>, prefix: String) -> Self {
        Self {
            puzzle_hash,
            prefix,
        }
    }
}

#[pyfunction]
pub fn encode_address(puzzle_hash: Vec<u8>, prefix: String) -> PyResult<String> {
    Ok(chia_sdk_bindings::encode_address(
        puzzle_hash.rust()?,
        prefix,
    )?)
}

#[pyfunction]
pub fn decode_address(address: String) -> PyResult<AddressInfo> {
    Ok(chia_sdk_bindings::decode_address(address)?.js()?)
}
