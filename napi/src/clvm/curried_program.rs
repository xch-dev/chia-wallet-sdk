use napi::bindgen_prelude::*;
use napi_derive::napi;

use super::Program;

#[napi(object)]
pub struct CurriedProgram {
    pub program: Reference<Program>,
    pub args: Vec<Reference<Program>>,
}
