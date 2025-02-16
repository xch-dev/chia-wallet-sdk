mod address;
mod bls;
mod mnemonic;
mod traits;
mod utils;

pub(crate) use address::AddressInfo;

use address::*;
use bls::*;
use mnemonic::*;
use utils::*;

use pyo3::prelude::*;

/// A Python module implemented in Rust.
#[pymodule]
fn chia_wallet_sdk_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Address
    m.add_class::<AddressInfo>()?;
    m.add_function(wrap_pyfunction!(encode_address, m)?)?;
    m.add_function(wrap_pyfunction!(decode_address, m)?)?;

    // BLS
    m.add_class::<SecretKey>()?;
    m.add_class::<PublicKey>()?;
    m.add_class::<Signature>()?;

    // Mnemonic
    m.add_function(wrap_pyfunction!(mnemonic_from_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(mnemonic_to_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(verify_mnemonic, m)?)?;
    m.add_function(wrap_pyfunction!(generate_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(generate_mnemonic, m)?)?;
    m.add_function(wrap_pyfunction!(mnemonic_to_seed, m)?)?;

    // Utils
    m.add_function(wrap_pyfunction!(from_hex, m)?)?;
    m.add_function(wrap_pyfunction!(to_hex, m)?)?;
    m.add_function(wrap_pyfunction!(bytes_equal, m)?)?;
    m.add_function(wrap_pyfunction!(tree_hash_atom, m)?)?;
    m.add_function(wrap_pyfunction!(tree_hash_pair, m)?)?;
    m.add_function(wrap_pyfunction!(sha256, m)?)?;

    Ok(())
}
