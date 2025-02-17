use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::Program;

#[napi]
pub struct Spend {
    pub(crate) puzzle: Reference<Program>,
    pub(crate) solution: Reference<Program>,
}

#[napi]
impl Spend {
    #[napi(constructor)]
    pub fn new(puzzle: Reference<Program>, solution: Reference<Program>) -> Self {
        Self { puzzle, solution }
    }

    #[napi(getter)]
    pub fn puzzle(&self, env: Env) -> Result<Reference<Program>> {
        self.puzzle.clone(env)
    }

    #[napi(getter)]
    pub fn solution(&self, env: Env) -> Result<Reference<Program>> {
        self.solution.clone(env)
    }
}
