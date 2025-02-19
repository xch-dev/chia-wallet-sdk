use napi::bindgen_prelude::*;
use napi_derive::napi;

use super::Program;

#[napi(object)]
pub struct Output {
    pub value: Reference<Program>,
    pub cost: BigInt,
}
