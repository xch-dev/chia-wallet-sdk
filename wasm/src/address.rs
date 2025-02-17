use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{IntoJs, IntoRust};

#[wasm_bindgen(getter_with_clone)]
pub struct AddressInfo {
    #[wasm_bindgen(js_name = "puzzleHash")]
    pub puzzle_hash: Vec<u8>,
    pub prefix: String,
}

#[wasm_bindgen]
impl AddressInfo {
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(js_name = "puzzleHash")] puzzle_hash: Vec<u8>,
        prefix: String,
    ) -> Self {
        Self {
            puzzle_hash,
            prefix,
        }
    }
}

#[wasm_bindgen(js_name = "encodeAddress")]
pub fn encode_address(
    #[wasm_bindgen(js_name = "puzzleHash")] puzzle_hash: Vec<u8>,
    prefix: String,
) -> Result<String, JsError> {
    Ok(chia_sdk_bindings::encode_address(
        puzzle_hash.rust()?,
        prefix,
    )?)
}

#[wasm_bindgen(js_name = "decodeAddress")]
pub fn decode_address(address: String) -> Result<AddressInfo, JsError> {
    Ok(chia_sdk_bindings::decode_address(address)?.js()?)
}
