use wasm_bindgen::prelude::wasm_bindgen;

use super::Program;

#[wasm_bindgen]
pub struct CurriedProgram {
    pub(crate) program: Program,
    pub(crate) args: Vec<Program>,
}

#[wasm_bindgen]
impl CurriedProgram {
    #[wasm_bindgen(constructor)]
    pub fn new(program: Program, args: Vec<Program>) -> Self {
        Self { program, args }
    }

    #[wasm_bindgen(getter)]
    pub fn program(&self) -> Program {
        self.program.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn args(&self) -> Vec<Program> {
        self.args.clone()
    }
}
