#![allow(clippy::too_many_arguments)]

use bindy::{FromRust, IntoRust, WasmContext};
use js_sys::BigInt;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

bindy_macro::bindy_wasm!("bindings.json");

#[wasm_bindgen]
impl Clvm {
    #[wasm_bindgen]
    pub fn int(&self, value: f64) -> Result<Program, JsError> {
        Ok(Program::from_rust(self.0.f64(value)?, &WasmContext)?)
    }

    #[wasm_bindgen(js_name = "bigInt")]
    pub fn big_int(&self, value: BigInt) -> Result<Program, JsError> {
        Ok(Program::from_rust(
            self.0.big_int(value.into_rust(&WasmContext)?)?,
            &WasmContext,
        )?)
    }
}

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
