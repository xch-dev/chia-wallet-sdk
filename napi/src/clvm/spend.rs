use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::Program;

#[napi(object)]
pub struct Spend {
    pub puzzle: Reference<Program>,
    pub solution: Reference<Program>,
}
