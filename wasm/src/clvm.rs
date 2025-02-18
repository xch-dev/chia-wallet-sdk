mod curried_program;
mod output;
mod pair;
mod program;
mod spend;

use std::sync::{Arc, RwLock};

use clvmr::NodePtr;
pub use curried_program::*;
pub use output::*;
pub use pair::*;
pub use program::*;
pub use spend::*;

use chia_sdk_bindings::Error;
use js_sys::BigInt;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{CatSpend, Coin, CoinSpend, IntoJs, IntoRust, PublicKey};

#[wasm_bindgen]
#[derive(Default)]
pub struct Clvm(Arc<RwLock<chia_sdk_bindings::Clvm>>);

#[wasm_bindgen]
impl Clvm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(js_name = "insertCoinSpend")]
    pub fn insert_coin_spend(
        &self,
        #[wasm_bindgen(js_name = "coinSpend")] coin_spend: CoinSpend,
    ) -> Result<(), JsError> {
        self.0
            .write()
            .unwrap()
            .insert_coin_spend(coin_spend.rust()?);
        Ok(())
    }

    #[wasm_bindgen(js_name = "coinSpends")]
    pub fn coin_spends(&self) -> Result<Vec<CoinSpend>, JsError> {
        Ok(self
            .0
            .write()
            .unwrap()
            .take_coin_spends()
            .into_iter()
            .map(IntoJs::js)
            .collect::<Result<Vec<_>, _>>()?)
    }

    #[wasm_bindgen(js_name = "spendCoin")]
    pub fn spend_coin(&self, coin: Coin, spend: Spend) -> Result<(), JsError> {
        let mut clvm = self.0.write().unwrap();
        let puzzle_reveal = clvm.serialize(spend.puzzle.node_ptr)?;
        let solution = clvm.serialize(spend.solution.node_ptr)?;
        let coin_spend = chia_sdk_bindings::CoinSpend::new(coin.rust()?, puzzle_reveal, solution);
        clvm.insert_coin_spend(coin_spend);
        Ok(())
    }

    #[wasm_bindgen]
    pub fn deserialize(&self, value: Vec<u8>) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self.0.write().unwrap().deserialize(value.rust()?)?,
        })
    }

    #[wasm_bindgen(js_name = "deserializeWithBackrefs")]
    pub fn deserialize_with_backrefs(&self, value: Vec<u8>) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self
                .0
                .write()
                .unwrap()
                .deserialize_with_backrefs(value.rust()?)?,
        })
    }

    #[wasm_bindgen]
    pub fn number(&self, value: f64) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self.0.write().unwrap().new_f64(value)?,
        })
    }

    #[wasm_bindgen(js_name = "bigInt")]
    pub fn big_int(&self, value: BigInt) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self
                .0
                .write()
                .unwrap()
                .new_bigint(String::from(value.to_string(10).map_err(Error::Range)?).parse()?)?,
        })
    }

    #[wasm_bindgen]
    pub fn string(&self, value: String) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self.0.write().unwrap().new_string(value)?,
        })
    }

    #[wasm_bindgen]
    pub fn bool(&self, value: bool) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self.0.write().unwrap().new_bool(value)?,
        })
    }

    #[wasm_bindgen]
    pub fn atom(&self, value: Vec<u8>) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self.0.write().unwrap().new_atom(value.rust()?)?,
        })
    }

    #[wasm_bindgen]
    pub fn nil(&self) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: NodePtr::NIL,
        })
    }

    #[wasm_bindgen]
    pub fn pair(&self, first: Program, rest: Program) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self
                .0
                .write()
                .unwrap()
                .new_pair(first.node_ptr, rest.node_ptr)?,
        })
    }

    #[wasm_bindgen]
    pub fn list(&self, items: Vec<Program>) -> Result<Program, JsError> {
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr: self
                .0
                .write()
                .unwrap()
                .new_list(items.into_iter().map(|p| p.node_ptr).collect())?,
        })
    }

    #[wasm_bindgen(js_name = "delegatedSpend")]
    pub fn delegated_spend(&self, conditions: Vec<Program>) -> Result<Spend, JsError> {
        let spend = self
            .0
            .write()
            .unwrap()
            .delegated_spend(conditions.into_iter().map(|p| p.node_ptr).collect())?;

        Ok(Spend {
            puzzle: Program {
                clvm: self.0.clone(),
                node_ptr: spend.puzzle,
            },
            solution: Program {
                clvm: self.0.clone(),
                node_ptr: spend.solution,
            },
        })
    }

    #[wasm_bindgen(js_name = "standardSpend")]
    pub fn standard_spend(
        &self,
        #[wasm_bindgen(js_name = "syntheticKey")] synthetic_key: PublicKey,
        #[wasm_bindgen(js_name = "delegatedSpend")] delegated_spend: Spend,
    ) -> Result<Spend, JsError> {
        let spend = self.0.write().unwrap().standard_spend(
            synthetic_key.0,
            chia_sdk_bindings::Spend::new(
                delegated_spend.puzzle.node_ptr,
                delegated_spend.solution.node_ptr,
            ),
        )?;

        Ok(Spend {
            puzzle: Program {
                clvm: self.0.clone(),
                node_ptr: spend.puzzle,
            },
            solution: Program {
                clvm: self.0.clone(),
                node_ptr: spend.solution,
            },
        })
    }

    #[wasm_bindgen(js_name = "spendStandardCoin")]
    pub fn spend_standard_coin(
        &self,
        coin: &Coin,
        synthetic_key: &PublicKey,
        spend: &Spend,
    ) -> Result<(), JsError> {
        let mut clvm = self.0.write().unwrap();
        clvm.spend_standard_coin(
            coin.clone().rust()?,
            synthetic_key.0,
            chia_sdk_bindings::Spend::new(spend.puzzle.node_ptr, spend.solution.node_ptr),
        )?;
        Ok(())
    }

    #[wasm_bindgen(js_name = "spendCatCoins")]
    pub fn spend_cat_coins(&self, cat_spends: Vec<CatSpend>) -> Result<(), JsError> {
        let mut clvm = self.0.write().unwrap();

        clvm.spend_cat_coins(
            cat_spends
                .into_iter()
                .map(|item| {
                    chia_sdk_bindings::Result::Ok(chia_sdk_bindings::CatSpend::new(
                        item.cat.clone().rust()?,
                        chia_sdk_bindings::Spend::new(
                            item.spend.puzzle.node_ptr,
                            item.spend.solution.node_ptr,
                        ),
                    ))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?;

        Ok(())
    }
}
