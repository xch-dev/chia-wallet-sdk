use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{IntoJs, IntoRust};

#[wasm_bindgen(js_name = "mnemonicFromEntropy")]
pub fn mnemonic_from_entropy(entropy: Vec<u8>) -> Result<String, JsError> {
    Ok(chia_sdk_bindings::mnemonic_from_entropy(entropy.rust()?)?)
}

#[wasm_bindgen(js_name = "mnemonicToEntropy")]
pub fn mnemonic_to_entropy(mnemonic: String) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::mnemonic_to_entropy(mnemonic)?.js()?)
}

#[wasm_bindgen(js_name = "verifyMnemonic")]
pub fn verify_mnemonic(mnemonic: String) -> Result<bool, JsError> {
    Ok(chia_sdk_bindings::verify_mnemonic(mnemonic)?)
}

#[wasm_bindgen(js_name = "generateBytes")]
pub fn generate_bytes(bytes: i64) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::generate_bytes(bytes.try_into().unwrap())?.js()?)
}

#[wasm_bindgen(js_name = "generateMnemonic")]
pub fn generate_mnemonic(use_24: bool) -> Result<String, JsError> {
    Ok(chia_sdk_bindings::generate_mnemonic(use_24)?)
}

#[wasm_bindgen(js_name = "mnemonicToSeed")]
pub fn mnemonic_to_seed(mnemonic: String, password: String) -> Result<Vec<u8>, JsError> {
    Ok(chia_sdk_bindings::mnemonic_to_seed(mnemonic, password)?.js()?)
}
