use wasm_bindgen::prelude::wasm_bindgen;

use super::Program;

#[wasm_bindgen]
pub struct Pair {
    pub(crate) first: Program,
    pub(crate) rest: Program,
}

#[wasm_bindgen]
impl Pair {
    #[wasm_bindgen(constructor)]
    pub fn new(first: Program, rest: Program) -> Self {
        Self { first, rest }
    }

    #[wasm_bindgen(getter)]
    pub fn first(&self) -> Program {
        self.first.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn rest(&self) -> Program {
        self.rest.clone()
    }
}
