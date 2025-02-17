use napi::bindgen_prelude::*;
use napi_derive::napi;

use super::Program;

#[napi]
pub struct Pair {
    pub(crate) first: Reference<Program>,
    pub(crate) second: Reference<Program>,
}

#[napi]
impl Pair {
    #[napi(constructor)]
    pub fn new(first: Reference<Program>, second: Reference<Program>) -> Self {
        Self { first, second }
    }

    #[napi(getter)]
    pub fn first(&self, env: Env) -> Result<Reference<Program>> {
        self.first.clone(env)
    }

    #[napi(getter)]
    pub fn second(&self, env: Env) -> Result<Reference<Program>> {
        self.second.clone(env)
    }
}
