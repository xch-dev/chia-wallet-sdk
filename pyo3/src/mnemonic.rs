use pyo3::prelude::*;

use crate::traits::{IntoJs, IntoRust};

#[pyfunction]
pub fn mnemonic_from_entropy(entropy: Vec<u8>) -> PyResult<String> {
    Ok(chia_sdk_bindings::mnemonic_from_entropy(entropy.rust()?)?)
}

#[pyfunction]
pub fn mnemonic_to_entropy(mnemonic: String) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::mnemonic_to_entropy(mnemonic)?.js()?)
}

#[pyfunction]
pub fn verify_mnemonic(mnemonic: String) -> PyResult<bool> {
    Ok(chia_sdk_bindings::verify_mnemonic(mnemonic)?)
}

#[pyfunction]
pub fn generate_bytes(bytes: i64) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::generate_bytes(bytes.try_into().unwrap())?.js()?)
}

#[pyfunction]
pub fn generate_mnemonic(use_24: bool) -> PyResult<String> {
    Ok(chia_sdk_bindings::generate_mnemonic(use_24)?)
}

#[pyfunction]
pub fn mnemonic_to_seed(mnemonic: String, password: String) -> PyResult<Vec<u8>> {
    Ok(chia_sdk_bindings::mnemonic_to_seed(mnemonic, password)?.js()?)
}
