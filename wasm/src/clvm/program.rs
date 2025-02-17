use clvmr::NodePtr;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Program(pub(crate) NodePtr);

#[wasm_bindgen]
impl Program {
    #[wasm_bindgen(getter, js_name = "isAtom")]
    pub fn is_atom(&self) -> bool {
        self.0.is_atom()
    }

    #[wasm_bindgen(getter, js_name = "isPair")]
    pub fn is_pair(&self) -> bool {
        self.0.is_pair()
    }
}
