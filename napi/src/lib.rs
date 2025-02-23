#![allow(unsafe_code)]
#![allow(clippy::wildcard_imports)]

use bindy::{FromRust, IntoRust, NapiParamContext, NapiReturnContext};
use napi::{bindgen_prelude::BigInt, Env, Result};
use napi_derive::napi;

bindy_macro::bindy_napi!("bindings.json");

#[napi]
impl Clvm {
    #[napi]
    pub fn int(&self, env: Env, value: f64) -> Result<Program> {
        Ok(Program::from_rust(
            self.0.f64(value)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn big_int(&self, env: Env, value: BigInt) -> Result<Program> {
        Ok(Program::from_rust(
            self.0.big_int(value.into_rust(&NapiParamContext)?)?,
            &NapiReturnContext(env),
        )?)
    }
}

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
