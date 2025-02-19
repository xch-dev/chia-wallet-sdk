use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{IntoJs, IntoRust};

#[napi]
pub fn from_hex(value: String) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::from_hex(value)?.js()?)
}

#[napi]
pub fn to_hex(value: Uint8Array) -> Result<String> {
    Ok(chia_sdk_bindings::to_hex(value.rust()?))
}

#[napi]
pub fn bytes_equal(lhs: Uint8Array, rhs: Uint8Array) -> Result<bool> {
    Ok(chia_sdk_bindings::bytes_equal(lhs.rust()?, rhs.rust()?))
}

#[napi]
pub fn tree_hash_atom(atom: Uint8Array) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::tree_hash_atom(atom.rust()?).js()?)
}

#[napi]
pub fn tree_hash_pair(first: Uint8Array, rest: Uint8Array) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::tree_hash_pair(first.rust()?, rest.rust()?).js()?)
}

#[napi]
pub fn sha256(value: Uint8Array) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::sha256(value.rust()?).js()?)
}

#[napi]
pub fn curry_tree_hash(program: Uint8Array, args: Vec<Uint8Array>) -> Result<Uint8Array> {
    Ok(chia_sdk_bindings::curry_tree_hash(
        program.rust()?,
        args.into_iter()
            .map(IntoRust::rust)
            .collect::<chia_sdk_bindings::Result<Vec<_>>>()?,
    )
    .js()?)
}
