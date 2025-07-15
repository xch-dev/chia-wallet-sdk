#![allow(clippy::too_many_arguments)]
#![allow(unused_extern_crates)]

extern crate alloc;

use std::fmt::Display;

use bindy::{FromRust, WasmContext};
use js_sys::Array;
use wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsError, JsValue};
use wasm_bindgen_derive::{try_from_js_array, TryFromJsValue};

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

#[wasm_bindgen(js_name = "setPanicHook")]
pub fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Attempts to unpack a JS value into a typed value,
/// returning `None` if the JS value is `undefined`.
pub fn try_from_js_option<T>(val: impl Into<JsValue>) -> Result<Option<T>, String>
where
    for<'a> T: TryFrom<&'a JsValue>,
    for<'a> <T as TryFrom<&'a JsValue>>::Error: Display,
{
    let js_val = val.into();
    if js_val.is_undefined() {
        return Ok(None);
    }
    T::try_from(&js_val)
        .map(Some)
        .map_err(|err| format!("{err}"))
}

/// Attempts to unpack a JS array into a vector of typed values.
pub fn try_from_js_option_array<T>(val: impl Into<JsValue>) -> Result<Option<Vec<T>>, String>
where
    for<'a> T: TryFrom<&'a JsValue>,
    for<'a> <T as TryFrom<&'a JsValue>>::Error: Display,
{
    let js_val = val.into();
    if js_val.is_undefined() {
        return Ok(None);
    }
    let array: &Array = js_val.dyn_ref().ok_or("The argument must be an array")?;
    let length: usize = array.length().try_into().map_err(|err| format!("{err}"))?;
    let mut typed_array = Vec::<T>::with_capacity(length);
    for (idx, js) in array.iter().enumerate() {
        let typed_elem =
            T::try_from(&js).map_err(|err| format!("Failed to cast item {idx}: {err}"))?;
        typed_array.push(typed_elem);
    }
    Ok(Some(typed_array))
}
