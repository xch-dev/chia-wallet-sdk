use napi::bindgen_prelude::*;
use napi_derive::napi;

use super::Program;

#[napi(object)]
pub struct Pair {
    pub first: Reference<Program>,
    pub rest: Reference<Program>,
}
