mod curried_program;
mod output;
mod program;
mod spend;
mod value;

pub use curried_program::*;
pub use output::*;
pub use program::*;
pub use spend::*;

use value::*;

use std::sync::Arc;

use parking_lot::RwLock;
use pyo3::prelude::*;

use crate::{
    bls::PublicKey,
    coin::{Coin, CoinSpend},
    puzzles::CatSpend,
    traits::{IntoPy, IntoRust},
};

#[pyclass]
#[derive(Default)]
pub struct Clvm(pub(crate) Arc<RwLock<chia_sdk_bindings::Clvm>>);

#[pymethods]
impl Clvm {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(&mut self, value: Bound<'_, PyAny>) -> PyResult<Program> {
        let mut clvm = self.0.write();
        let node_ptr = alloc(&mut clvm, value)?;
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr,
        })
    }

    pub fn deserialize(&mut self, value: Vec<u8>) -> PyResult<Program> {
        let mut clvm = self.0.write();
        let node_ptr = clvm.deserialize(value.into())?;
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr,
        })
    }

    pub fn deserialize_with_backrefs(&mut self, value: Vec<u8>) -> PyResult<Program> {
        let mut clvm = self.0.write();
        let node_ptr = clvm.deserialize_with_backrefs(value.into())?;
        Ok(Program {
            clvm: self.0.clone(),
            node_ptr,
        })
    }

    pub fn insert_coin_spend(&mut self, coin_spend: &CoinSpend) -> PyResult<()> {
        let mut clvm = self.0.write();
        clvm.insert_coin_spend(coin_spend.clone().rust()?);
        Ok(())
    }

    pub fn coin_spends(&mut self) -> PyResult<Vec<CoinSpend>> {
        let mut clvm = self.0.write();
        Ok(clvm
            .take_coin_spends()
            .into_iter()
            .map(|cs| cs.py())
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn spend_coin(&mut self, coin: &Coin, spend: &Spend) -> PyResult<()> {
        let mut clvm = self.0.write();
        let puzzle_reveal = clvm.serialize(spend.puzzle.node_ptr)?;
        let solution = clvm.serialize(spend.solution.node_ptr)?;
        let coin_spend =
            chia_sdk_bindings::CoinSpend::new(coin.clone().rust()?, puzzle_reveal, solution);
        clvm.insert_coin_spend(coin_spend);
        Ok(())
    }

    pub fn delegated_spend(&mut self, conditions: Vec<Program>) -> PyResult<Spend> {
        let mut clvm = self.0.write();
        let spend = clvm.delegated_spend(conditions.into_iter().map(|p| p.node_ptr).collect())?;
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

    pub fn standard_spend(
        &mut self,
        synthetic_key: &PublicKey,
        delegated_spend: &Spend,
    ) -> PyResult<Spend> {
        let mut clvm = self.0.write();
        let spend = clvm.standard_spend(
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

    pub fn spend_standard_coin(
        &mut self,
        coin: &Coin,
        synthetic_key: &PublicKey,
        spend: &Spend,
    ) -> PyResult<()> {
        let mut clvm = self.0.write();
        clvm.spend_standard_coin(
            coin.clone().rust()?,
            synthetic_key.0,
            chia_sdk_bindings::Spend::new(spend.puzzle.node_ptr, spend.solution.node_ptr),
        )?;
        Ok(())
    }

    pub fn spend_cat_coins(&mut self, cat_spends: Vec<Bound<'_, CatSpend>>) -> PyResult<()> {
        let mut clvm = self.0.write();

        clvm.spend_cat_coins(
            cat_spends
                .into_iter()
                .map(|item| {
                    let item = item.borrow();
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
