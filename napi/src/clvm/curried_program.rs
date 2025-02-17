use napi::bindgen_prelude::*;
use napi_derive::napi;

use super::Program;

#[napi]
pub struct CurriedProgram {
    pub(crate) program: Reference<Program>,
    pub(crate) args: Vec<Reference<Program>>,
}

#[napi]
impl CurriedProgram {
    #[napi(constructor)]
    pub fn new(program: Reference<Program>, args: Vec<Reference<Program>>) -> Self {
        Self { program, args }
    }

    #[napi(getter)]
    pub fn program(&self, env: Env) -> Result<Reference<Program>> {
        self.program.clone(env)
    }

    #[napi(getter)]
    pub fn args(&self, env: Env) -> Result<Vec<Reference<Program>>> {
        self.args
            .iter()
            .map(|arg| arg.clone(env))
            .collect::<Result<Vec<_>>>()
    }
}
