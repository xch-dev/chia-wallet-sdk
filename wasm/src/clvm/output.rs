use wasm_bindgen::prelude::wasm_bindgen;

use super::Program;

#[wasm_bindgen]
pub struct Output {
    pub(crate) value: Program,
    pub(crate) cost: u64,
}

#[wasm_bindgen]
impl Output {
    #[wasm_bindgen(constructor)]
    pub fn new(value: Program, cost: u64) -> Self {
        Self { value, cost }
    }

    #[wasm_bindgen(getter)]
    pub fn value(&self) -> Program {
        self.value
    }

    #[wasm_bindgen(getter)]
    pub fn cost(&self) -> u64 {
        self.cost
    }
}
