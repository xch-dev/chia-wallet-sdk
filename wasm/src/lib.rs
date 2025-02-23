use bindy::{FromRust, WasmContext};
use js_sys::BigInt;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

bindy_macro::bindy_wasm!("bindings.json");

#[wasm_bindgen]
impl Program {
    #[wasm_bindgen(js_name = "toInt")]
    pub fn to_int(&self) -> Result<Option<f64>, JsError> {
        Ok(self.0.to_small_int()?)
    }

    #[wasm_bindgen(js_name = "toBigInt")]
    pub fn to_big_int(&self) -> Result<Option<BigInt>, JsError> {
        Ok(Option::<BigInt>::from_rust(
            self.0.to_big_int()?,
            &WasmContext,
        )?)
    }
}
