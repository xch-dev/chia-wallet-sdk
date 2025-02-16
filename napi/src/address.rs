use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{IntoJs, IntoRust};

#[napi]
pub struct AddressInfo {
    pub puzzle_hash: Uint8Array,
    pub prefix: String,
}

#[napi]
pub fn encode_address(puzzle_hash: Uint8Array, prefix: String) -> Result<String> {
    Ok(chia_sdk_bindings::encode_address(
        puzzle_hash.rust()?,
        prefix,
    )?)
}

#[napi]
pub fn decode_address(address: String) -> Result<AddressInfo> {
    Ok(chia_sdk_bindings::decode_address(address)?.js()?)
}
