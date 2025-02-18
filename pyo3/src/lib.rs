mod address;
mod bls;
mod clvm;
mod coin;
mod mnemonic;
mod puzzles;
mod secp;
mod traits;
mod utils;

pub(crate) use address::AddressInfo;

use address::*;
use bls::*;
use clvm::*;
use coin::*;
use mnemonic::*;
use puzzles::*;
use secp::*;
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

    // CLVM
    m.add_class::<Clvm>()?;
    m.add_class::<Program>()?;
    m.add_class::<CurriedProgram>()?;
    m.add_class::<Output>()?;
    m.add_class::<Spend>()?;

    // Coin
    m.add_class::<Coin>()?;
    m.add_class::<CoinState>()?;
    m.add_class::<CoinSpend>()?;
    m.add_class::<SpendBundle>()?;

    // Mnemonic
    m.add_function(wrap_pyfunction!(mnemonic_from_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(mnemonic_to_entropy, m)?)?;
    m.add_function(wrap_pyfunction!(verify_mnemonic, m)?)?;
    m.add_function(wrap_pyfunction!(generate_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(generate_mnemonic, m)?)?;
    m.add_function(wrap_pyfunction!(mnemonic_to_seed, m)?)?;

    // Puzzles
    m.add_function(wrap_pyfunction!(standard_puzzle_hash, m)?)?;
    m.add_function(wrap_pyfunction!(cat_puzzle_hash, m)?)?;

    // SECP
    m.add_class::<K1SecretKey>()?;
    m.add_class::<K1PublicKey>()?;
    m.add_class::<K1Signature>()?;
    m.add_class::<R1SecretKey>()?;
    m.add_class::<R1PublicKey>()?;
    m.add_class::<R1Signature>()?;

    // Utils
    m.add_function(wrap_pyfunction!(from_hex, m)?)?;
    m.add_function(wrap_pyfunction!(to_hex, m)?)?;
    m.add_function(wrap_pyfunction!(bytes_equal, m)?)?;
    m.add_function(wrap_pyfunction!(tree_hash_atom, m)?)?;
    m.add_function(wrap_pyfunction!(tree_hash_pair, m)?)?;
    m.add_function(wrap_pyfunction!(sha256, m)?)?;
    m.add_function(wrap_pyfunction!(curry_tree_hash, m)?)?;

    Ok(())
}
