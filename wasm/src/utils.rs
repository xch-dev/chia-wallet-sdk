use js_sys::{Array, Uint8Array};
use wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsError, UnwrapThrowExt};

use crate::{IntoJs, IntoRust};

#[wasm_bindgen(js_name = "fromHex")]
pub fn from_hex(value: String) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::from_hex(value)?.js()?)
}

#[wasm_bindgen(js_name = "toHex")]
pub fn to_hex(value: Vec<u8>) -> Result<String, JsError> {
    Ok(chia_sdk_bindings::to_hex(value.rust()?))
}

#[wasm_bindgen(js_name = "bytesEqual")]
pub fn bytes_equal(lhs: Vec<u8>, rhs: Vec<u8>) -> Result<bool, JsError> {
    Ok(chia_sdk_bindings::bytes_equal(lhs.rust()?, rhs.rust()?))
}

#[wasm_bindgen(js_name = "treeHashAtom")]
pub fn tree_hash_atom(value: Vec<u8>) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::tree_hash_atom(value.rust()?).js()?)
}

#[wasm_bindgen(js_name = "treeHashPair")]
pub fn tree_hash_pair(first: Vec<u8>, rest: Vec<u8>) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::tree_hash_pair(first.rust()?, rest.rust()?).js()?)
}

#[wasm_bindgen(js_name = "sha256")]
pub fn sha256(value: Vec<u8>) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::sha256(value.rust()?).js()?)
}

#[wasm_bindgen(js_name = "curryTreeHash")]
pub fn curry_tree_hash(program: Vec<u8>, args: Array) -> Result<Vec<u8>, JsError> {
    let args: Vec<Vec<u8>> = args
        .values()
        .into_iter()
        .map(|item| item.unwrap_throw().unchecked_ref::<Uint8Array>().to_vec())
        .collect();

    Ok(chia_sdk_bindings::curry_tree_hash(
        program.rust()?,
        args.into_iter()
            .map(IntoRust::rust)
            .collect::<chia_sdk_bindings::Result<Vec<_>>>()?,
    )
    .js()?)
}
