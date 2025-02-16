use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{IntoJs, IntoRust};

#[napi]
pub fn mnemonic_from_entropy(entropy: Uint8Array) -> Result<String> {
    Ok(chia_sdk_bindings::mnemonic_from_entropy(entropy.rust()?)?)
}

#[napi]
pub fn mnemonic_to_entropy(mnemonic: String) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::mnemonic_to_entropy(mnemonic)?.js()?)
}

#[napi]
pub fn verify_mnemonic(mnemonic: String) -> Result<bool> {
    Ok(chia_sdk_bindings::verify_mnemonic(mnemonic)?)
}

#[napi]
pub fn generate_bytes(bytes: i64) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::generate_bytes(bytes.try_into().unwrap())?.js()?)
}

#[napi]
pub fn generate_mnemonic(use_24: bool) -> Result<String> {
    Ok(chia_sdk_bindings::generate_mnemonic(use_24)?)
}

#[napi]
pub fn mnemonic_to_seed(mnemonic: String, password: String) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::mnemonic_to_seed(mnemonic, password)?.js()?)
}
