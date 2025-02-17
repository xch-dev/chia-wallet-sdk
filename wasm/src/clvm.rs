mod curried_program;
mod output;
mod pair;
mod program;
mod spend;

pub use curried_program::*;
pub use output::*;
pub use pair::*;
pub use program::*;
pub use spend::*;

use chia_sdk_bindings::Error;
use js_sys::BigInt;
use wasm_bindgen::{prelude::wasm_bindgen, JsError, JsValue};

use crate::{Coin, CoinSpend, IntoJs, IntoRust, PublicKey};

#[wasm_bindgen]
#[derive(Default)]
pub struct Clvm(chia_sdk_bindings::Clvm);

#[wasm_bindgen]
impl Clvm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(js_name = "insertCoinSpend")]
    pub fn insert_coin_spend(
        &mut self,
        #[wasm_bindgen(js_name = "coinSpend")] coin_spend: CoinSpend,
    ) -> Result<(), JsError> {
        self.0.insert_coin_spend(coin_spend.rust()?);
        Ok(())
    }

    #[wasm_bindgen(js_name = "coinSpends")]
    pub fn coin_spends(&mut self) -> Result<Vec<CoinSpend>, JsError> {
        Ok(self
            .0
            .take_coin_spends()
            .into_iter()
            .map(IntoJs::js)
            .collect::<Result<Vec<_>, _>>()?)
    }

    #[wasm_bindgen(js_name = "spendCoin")]
    pub fn spend_coin(&mut self, coin: Coin, spend: Spend) -> Result<(), JsError> {
        let puzzle_reveal = self.0.serialize(spend.puzzle.0)?;
        let solution = self.0.serialize(spend.solution.0)?;
        let coin_spend = chia_sdk_bindings::CoinSpend::new(coin.rust()?, puzzle_reveal, solution);
        self.0.insert_coin_spend(coin_spend);
        Ok(())
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

    #[wasm_bindgen]
    pub fn run(
        &mut self,
        puzzle: Program,
        solution: Program,
        #[wasm_bindgen(js_name = "maxCost")] max_cost: u64,
        #[wasm_bindgen(js_name = "mempoolMode")] mempool_mode: bool,
    ) -> Result<Output, JsError> {
        let output = self.0.run(puzzle.0, solution.0, max_cost, mempool_mode)?;

        Ok(Output {
            value: Program(output.1),
            cost: output.0,
        })
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

    #[wasm_bindgen(js_name = "delegatedSpend")]
    pub fn delegated_spend(&mut self, conditions: Vec<Program>) -> Result<Spend, JsError> {
        let spend = self
            .0
            .delegated_spend(conditions.into_iter().map(|p| p.0).collect())?;

        Ok(Spend {
            puzzle: Program(spend.puzzle),
            solution: Program(spend.solution),
        })
    }

    #[wasm_bindgen(js_name = "standardSpend")]
    pub fn standard_spend(
        &mut self,
        #[wasm_bindgen(js_name = "syntheticKey")] synthetic_key: PublicKey,
        #[wasm_bindgen(js_name = "delegatedSpend")] delegated_spend: Spend,
    ) -> Result<Spend, JsError> {
        let spend = self.0.standard_spend(
            synthetic_key.0,
            chia_sdk_bindings::Spend::new(delegated_spend.puzzle.0, delegated_spend.solution.0),
        )?;

        Ok(Spend {
            puzzle: Program(spend.puzzle),
            solution: Program(spend.solution),
        })
    }
}
