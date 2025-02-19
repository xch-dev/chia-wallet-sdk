use wasm_bindgen::prelude::wasm_bindgen;

use super::Program;

#[wasm_bindgen]
pub struct Pair {
    pub(crate) first: Program,
    pub(crate) second: Program,
}

#[wasm_bindgen]
impl Pair {
    #[wasm_bindgen(constructor)]
    pub fn new(first: Program, second: Program) -> Self {
        Self { first, second }
    }

    #[wasm_bindgen(getter)]
    pub fn first(&self) -> Program {
        self.first.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn second(&self) -> Program {
        self.second.clone()
    }
}
