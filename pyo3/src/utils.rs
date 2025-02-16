use pyo3::prelude::*;

use crate::traits::{IntoPy, IntoRust};

#[pyfunction]
pub fn from_hex(value: String) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::from_hex(value)?.py()?)
}

#[pyfunction]
pub fn to_hex(value: Vec<u8>) -> PyResult<String> {
    Ok(chia_sdk_bindings::to_hex(value.rust()?)?)
}

#[pyfunction]
pub fn bytes_equal(lhs: Vec<u8>, rhs: Vec<u8>) -> PyResult<bool> {
    Ok(chia_sdk_bindings::bytes_equal(lhs.rust()?, rhs.rust()?)?)
}

#[pyfunction]
pub fn tree_hash_atom(value: Vec<u8>) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::tree_hash_atom(value.rust()?)?.py()?)
}

#[pyfunction]
pub fn tree_hash_pair(first: Vec<u8>, rest: Vec<u8>) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::tree_hash_pair(first.rust()?, rest.rust()?)?.py()?)
}

#[pyfunction]
pub fn sha256(value: Vec<u8>) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::sha256(value.rust()?)?.py()?)
}
