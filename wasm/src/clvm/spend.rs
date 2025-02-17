use wasm_bindgen::prelude::wasm_bindgen;

use super::Program;

#[wasm_bindgen]
pub struct Spend {
    pub(crate) puzzle: Program,
    pub(crate) solution: Program,
}

#[wasm_bindgen]
impl Spend {
    #[wasm_bindgen(constructor)]
    pub fn new(puzzle: Program, solution: Program) -> Self {
        Self { puzzle, solution }
    }

    #[wasm_bindgen(getter)]
    pub fn puzzle(&self) -> Program {
        self.puzzle
    }

    #[wasm_bindgen(getter)]
    pub fn solution(&self) -> Program {
        self.solution
    }
}
