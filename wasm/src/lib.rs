#![allow(clippy::too_many_arguments)]

use bindy::{FromRust, WasmContext};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

bindy_macro::bindy_wasm!("bindings.json");

#[wasm_bindgen]
impl Clvm {
    #[wasm_bindgen(js_name = "boundCheckedNumber")]
    pub fn bound_checked_number(&self, value: f64) -> Result<Program, JsError> {
        Ok(Program::from_rust(self.0.f64(value)?, &WasmContext)?)
    }
}

#[wasm_bindgen]
impl Program {
    #[wasm_bindgen(js_name = "toBoundCheckedNumber")]
    pub fn to_bound_checked_number(&self) -> Result<Option<f64>, JsError> {
        Ok(self.0.to_small_int()?)
    }
}
