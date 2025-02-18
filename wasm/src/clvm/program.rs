use std::sync::{Arc, RwLock};

use chia_sdk_bindings::Clvm;
use clvmr::NodePtr;
use wasm_bindgen::{prelude::wasm_bindgen, JsError, JsValue};

use crate::{CurriedProgram, IntoJs, Pair};

use super::Output;

#[wasm_bindgen]
#[derive(Clone)]
pub struct Program {
    pub(crate) clvm: Arc<RwLock<Clvm>>,
    pub(crate) node_ptr: NodePtr,
}

#[wasm_bindgen]
impl Program {
    #[wasm_bindgen(getter, js_name = "isAtom")]
    pub fn is_atom(&self) -> bool {
        self.node_ptr.is_atom()
    }

    #[wasm_bindgen(getter, js_name = "isPair")]
    pub fn is_pair(&self) -> bool {
        self.node_ptr.is_pair()
    }

    #[wasm_bindgen]
    pub fn serialize(&self, value: Program) -> Result<Vec<u8>, JsError> {
        Ok(self.clvm.read().unwrap().serialize(value.node_ptr)?.js()?)
    }

    #[wasm_bindgen(js_name = "serializeWithBackrefs")]
    pub fn serialize_with_backrefs(&self, value: Program) -> Result<Vec<u8>, JsError> {
        Ok(self
            .clvm
            .read()
            .unwrap()
            .serialize_with_backrefs(value.node_ptr)?
            .js()?)
    }

    #[wasm_bindgen(js_name = "toNumber")]
    pub fn to_number(&self, value: Program) -> Result<Option<f64>, JsError> {
        Ok(self.clvm.read().unwrap().as_f64(value.node_ptr)?)
    }

    #[wasm_bindgen(js_name = "toBigInt")]
    pub fn to_big_int(&self, value: Program) -> Result<JsValue, JsError> {
        let Some(big_int) = self.clvm.read().unwrap().as_bigint(value.node_ptr)? else {
            return Ok(JsValue::NULL);
        };

        Ok(JsValue::bigint_from_str(&big_int.to_string()))
    }

    #[wasm_bindgen(js_name = "toString")]
    pub fn to_string(&self, value: Program) -> Result<Option<String>, JsError> {
        Ok(self.clvm.read().unwrap().as_string(value.node_ptr)?)
    }

    #[wasm_bindgen(js_name = "toBool")]
    pub fn to_bool(&self, value: Program) -> Result<Option<bool>, JsError> {
        Ok(self.clvm.read().unwrap().as_bool(value.node_ptr)?)
    }

    #[wasm_bindgen(js_name = "toAtom")]
    pub fn to_bytes(&self, value: Program) -> Result<Option<Vec<u8>>, JsError> {
        Ok(self
            .clvm
            .read()
            .unwrap()
            .as_atom(value.node_ptr)?
            .map(IntoJs::js)
            .transpose()?)
    }

    #[wasm_bindgen(js_name = "toPair")]
    pub fn to_pair(&self, value: Program) -> Result<Option<Pair>, JsError> {
        let Some(pair) = self.clvm.read().unwrap().as_pair(value.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(Pair {
            first: Program {
                clvm: self.clvm.clone(),
                node_ptr: pair.0,
            },
            second: Program {
                clvm: self.clvm.clone(),
                node_ptr: pair.1,
            },
        }))
    }

    #[wasm_bindgen(js_name = "toList")]
    pub fn to_list(&self, value: Program) -> Result<Option<Vec<Program>>, JsError> {
        let Some(list) = self.clvm.read().unwrap().as_list(value.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(
            list.into_iter()
                .map(|p| Program {
                    clvm: self.clvm.clone(),
                    node_ptr: p,
                })
                .collect(),
        ))
    }

    #[wasm_bindgen]
    pub fn curry(&self, args: Vec<Program>) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.clvm.clone(),
            node_ptr: self.clvm.write().unwrap().curry(
                self.node_ptr,
                args.into_iter().map(|p| p.node_ptr).collect(),
            )?,
        })
    }

    #[wasm_bindgen]
    pub fn uncurry(&self, value: Program) -> Result<Option<CurriedProgram>, JsError> {
        let Some((program, args)) = self.clvm.read().unwrap().uncurry(value.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(CurriedProgram {
            program: Program {
                clvm: self.clvm.clone(),
                node_ptr: program,
            },
            args: args
                .into_iter()
                .map(|p| Program {
                    clvm: self.clvm.clone(),
                    node_ptr: p,
                })
                .collect(),
        }))
    }

    #[wasm_bindgen]
    pub fn length(&self, value: Program) -> Result<usize, JsError> {
        Ok(self.clvm.read().unwrap().length(value.node_ptr)?)
    }

    #[wasm_bindgen]
    pub fn first(&self, value: Program) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.clvm.clone(),
            node_ptr: self.clvm.read().unwrap().first(value.node_ptr)?,
        })
    }

    #[wasm_bindgen]
    pub fn rest(&self, value: Program) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.clvm.clone(),
            node_ptr: self.clvm.read().unwrap().rest(value.node_ptr)?,
        })
    }

    #[wasm_bindgen]
    pub fn run(
        &self,
        solution: Program,
        #[wasm_bindgen(js_name = "maxCost")] max_cost: u64,
        #[wasm_bindgen(js_name = "mempoolMode")] mempool_mode: bool,
    ) -> Result<Output, JsError> {
        let output = self.clvm.write().unwrap().run(
            self.node_ptr,
            solution.node_ptr,
            max_cost,
            mempool_mode,
        )?;

        Ok(Output {
            value: Program {
                clvm: self.clvm.clone(),
                node_ptr: output.1,
            },
            cost: output.0,
        })
    }

    #[wasm_bindgen(js_name = "treeHash")]
    pub fn tree_hash(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.clvm.write().unwrap().tree_hash(self.node_ptr)?.js()?)
    }
}
