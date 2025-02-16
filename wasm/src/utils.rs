use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{IntoJs, IntoRust};

#[wasm_bindgen(js_name = "fromHex")]
pub fn from_hex(value: String) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::from_hex(value)?.js()?)
}

#[wasm_bindgen(js_name = "toHex")]
pub fn to_hex(value: Vec<u8>) -> Result<String, JsError> {
    Ok(chia_sdk_bindings::to_hex(value.rust()?)?)
}

#[wasm_bindgen(js_name = "bytesEqual")]
pub fn bytes_equal(lhs: Vec<u8>, rhs: Vec<u8>) -> Result<bool, JsError> {
    Ok(chia_sdk_bindings::bytes_equal(lhs.rust()?, rhs.rust()?)?)
}

#[wasm_bindgen(js_name = "treeHashAtom")]
pub fn tree_hash_atom(value: Vec<u8>) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::tree_hash_atom(value.rust()?)?.js()?)
}

#[wasm_bindgen(js_name = "treeHashPair")]
pub fn tree_hash_pair(first: Vec<u8>, rest: Vec<u8>) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::tree_hash_pair(first.rust()?, rest.rust()?)?.js()?)
}

#[wasm_bindgen(js_name = "sha256")]
pub fn sha256(value: Vec<u8>) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::sha256(value.rust()?)?.js()?)
}
