use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
pub struct Simulator(chia_sdk_bindings::Simulator);
