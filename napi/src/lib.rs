#![allow(unsafe_code)]
#![allow(clippy::wildcard_imports)]

use bindy::{FromRust, NapiReturnContext};
use napi::{bindgen_prelude::BigInt, Env, Result};
use napi_derive::napi;

bindy_macro::bindy_napi!("bindings.json");

#[napi]
impl Program {
    #[napi]
    pub fn to_int(&self) -> Result<Option<f64>> {
        Ok(self.0.to_small_int()?)
    }

    #[napi]
    pub fn to_big_int(&self, env: Env) -> Result<Option<BigInt>> {
        Ok(Option::<BigInt>::from_rust(
            self.0.to_big_int()?,
            &NapiReturnContext(env),
        )?)
    }
}
