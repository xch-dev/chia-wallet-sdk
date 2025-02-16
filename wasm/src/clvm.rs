use chia_sdk_bindings::Error;
use clvmr::NodePtr;
use js_sys::BigInt;
use wasm_bindgen::{prelude::wasm_bindgen, JsError, JsValue};

use crate::{IntoJs, IntoRust};

#[wasm_bindgen]
#[derive(Default)]
pub struct Clvm(chia_sdk_bindings::Clvm);

#[wasm_bindgen]
impl Clvm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen]
    pub fn serialize(&mut self, value: Program) -> Result<Vec<u8>, JsError> {
        Ok(self.0.serialize(value.0)?.js()?)
    }

    #[wasm_bindgen(js_name = "serializeWithBackrefs")]
    pub fn serialize_with_backrefs(&mut self, value: Program) -> Result<Vec<u8>, JsError> {
        Ok(self.0.serialize_with_backrefs(value.0)?.js()?)
    }

    #[wasm_bindgen]
    pub fn deserialize(&mut self, value: Vec<u8>) -> Result<Program, JsError> {
        Ok(Program(self.0.deserialize(value.rust()?)?))
    }

    #[wasm_bindgen(js_name = "deserializeWithBackrefs")]
    pub fn deserialize_with_backrefs(&mut self, value: Vec<u8>) -> Result<Program, JsError> {
        Ok(Program(self.0.deserialize_with_backrefs(value.rust()?)?))
    }

    #[wasm_bindgen(js_name = "treeHash")]
    pub fn tree_hash(&mut self, value: Program) -> Result<Vec<u8>, JsError> {
        Ok(self.0.tree_hash(value.0)?.js()?)
    }

    #[wasm_bindgen]
    pub fn number(&mut self, value: f64) -> Result<Program, JsError> {
        Ok(Program(self.0.new_f64(value)?))
    }

    #[wasm_bindgen(js_name = "bigInt")]
    pub fn big_int(&mut self, value: BigInt) -> Result<Program, JsError> {
        Ok(Program(self.0.new_bigint(
            String::from(value.to_string(10).map_err(Error::Range)?).parse()?,
        )?))
    }

    #[wasm_bindgen]
    pub fn string(&mut self, value: String) -> Result<Program, JsError> {
        Ok(Program(self.0.new_string(value)?))
    }

    #[wasm_bindgen]
    pub fn bool(&mut self, value: bool) -> Result<Program, JsError> {
        Ok(Program(self.0.new_bool(value)?))
    }

    #[wasm_bindgen]
    pub fn atom(&mut self, value: Vec<u8>) -> Result<Program, JsError> {
        Ok(Program(self.0.new_atom(value.rust()?)?))
    }

    #[wasm_bindgen]
    pub fn pair(&mut self, first: Program, rest: Program) -> Result<Program, JsError> {
        Ok(Program(self.0.new_pair(first.0, rest.0)?))
    }

    #[wasm_bindgen]
    pub fn list(&mut self, items: Vec<Program>) -> Result<Program, JsError> {
        Ok(Program(
            self.0.new_list(items.into_iter().map(|p| p.0).collect())?,
        ))
    }

    #[wasm_bindgen]
    pub fn curry(&mut self, program: Program, args: Vec<Program>) -> Result<Program, JsError> {
        Ok(Program(self.0.curry(
            program.0,
            args.into_iter().map(|p| p.0).collect(),
        )?))
    }

    #[wasm_bindgen(js_name = "toNumber")]
    pub fn to_number(&mut self, value: Program) -> Result<Option<f64>, JsError> {
        Ok(self.0.as_f64(value.0)?)
    }

    #[wasm_bindgen(js_name = "toBigInt")]
    pub fn to_big_int(&mut self, value: Program) -> Result<JsValue, JsError> {
        let Some(big_int) = self.0.as_bigint(value.0)? else {
            return Ok(JsValue::NULL);
        };

        Ok(JsValue::bigint_from_str(&big_int.to_string()))
    }

    #[wasm_bindgen(js_name = "toString")]
    pub fn to_string(&mut self, value: Program) -> Result<Option<String>, JsError> {
        Ok(self.0.as_string(value.0)?)
    }

    #[wasm_bindgen(js_name = "toBool")]
    pub fn to_bool(&mut self, value: Program) -> Result<Option<bool>, JsError> {
        Ok(self.0.as_bool(value.0)?)
    }

    #[wasm_bindgen(js_name = "toAtom")]
    pub fn to_atom(&mut self, value: Program) -> Result<Option<Vec<u8>>, JsError> {
        Ok(self.0.as_atom(value.0)?.map(IntoJs::js).transpose()?)
    }

    #[wasm_bindgen(js_name = "toPair")]
    pub fn to_pair(&mut self, value: Program) -> Result<Option<Pair>, JsError> {
        let Some(pair) = self.0.as_pair(value.0)? else {
            return Ok(None);
        };

        Ok(Some(Pair {
            first: Program(pair.0),
            second: Program(pair.1),
        }))
    }

    #[wasm_bindgen(js_name = "toList")]
    pub fn to_list(&mut self, value: Program) -> Result<Option<Vec<Program>>, JsError> {
        let Some(list) = self.0.as_list(value.0)? else {
            return Ok(None);
        };

        Ok(Some(list.into_iter().map(Program).collect()))
    }

    #[wasm_bindgen]
    pub fn uncurry(&mut self, value: Program) -> Result<Option<CurriedProgram>, JsError> {
        let Some((program, args)) = self.0.uncurry(value.0)? else {
            return Ok(None);
        };

        Ok(Some(CurriedProgram {
            program: Program(program),
            args: args.into_iter().map(Program).collect(),
        }))
    }

    #[wasm_bindgen]
    pub fn length(&mut self, value: Program) -> Result<usize, JsError> {
        Ok(self.0.length(value.0)?)
    }

    #[wasm_bindgen]
    pub fn first(&mut self, value: Program) -> Result<Program, JsError> {
        Ok(Program(self.0.first(value.0)?))
    }

    #[wasm_bindgen]
    pub fn rest(&mut self, value: Program) -> Result<Program, JsError> {
        Ok(Program(self.0.rest(value.0)?))
    }
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Program(NodePtr);

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

#[wasm_bindgen]
pub struct Pair {
    first: Program,
    second: Program,
}

#[wasm_bindgen]
impl Pair {
    #[wasm_bindgen(getter)]
    pub fn first(&self) -> Program {
        self.first
    }

    #[wasm_bindgen(getter)]
    pub fn second(&self) -> Program {
        self.second
    }
}

#[wasm_bindgen]
pub struct CurriedProgram {
    program: Program,
    args: Vec<Program>,
}

#[wasm_bindgen]
impl CurriedProgram {
    #[wasm_bindgen(getter)]
    pub fn program(&self) -> Program {
        self.program
    }

    #[wasm_bindgen(getter)]
    pub fn args(&self) -> Vec<Program> {
        self.args.clone()
    }
}
