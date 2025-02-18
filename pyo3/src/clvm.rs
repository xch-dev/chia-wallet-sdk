mod curried_program;
mod output;
mod program;
mod spend;

pub use curried_program::*;
pub use output::*;
pub use program::*;
pub use spend::*;

use std::sync::Arc;

use clvmr::NodePtr;
use num_bigint::BigInt;
use parking_lot::RwLock;
use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyList, PyNone, PyTuple},
};

use crate::{
    bls::{PublicKey, Signature},
    coin::{Coin, CoinSpend},
    puzzles::CatSpend,
    secp::{K1PublicKey, K1Signature, R1PublicKey, R1Signature},
    traits::{IntoPy, IntoRust},
};

#[pyclass]
#[derive(Default)]
pub struct Clvm(Arc<RwLock<chia_sdk_bindings::Clvm>>);

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

fn alloc(clvm: &mut chia_sdk_bindings::Clvm, value: Bound<'_, PyAny>) -> PyResult<NodePtr> {
    if let Ok(_value) = value.downcast::<PyNone>() {
        Ok(NodePtr::NIL)
    } else if let Ok(value) = value.extract::<BigInt>() {
        Ok(clvm.new_bigint(value)?)
    } else if let Ok(value) = value.extract::<bool>() {
        Ok(clvm.new_bool(value)?)
    } else if let Ok(value) = value.extract::<String>() {
        Ok(clvm.new_string(value)?)
    } else if let Ok(value) = value.extract::<Vec<u8>>() {
        Ok(clvm.new_atom(value.into())?)
    } else if let Ok(value) = value.extract::<Program>() {
        Ok(value.node_ptr)
    } else if let Ok(value) = value.extract::<PublicKey>() {
        Ok(clvm.new_atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<Signature>() {
        Ok(clvm.new_atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<K1PublicKey>() {
        Ok(clvm.new_atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<K1Signature>() {
        Ok(clvm.new_atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<R1PublicKey>() {
        Ok(clvm.new_atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<R1Signature>() {
        Ok(clvm.new_atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<CurriedProgram>() {
        let mut args = Vec::new();

        for arg in value.args {
            args.push(arg.node_ptr);
        }

        Ok(clvm.curry(value.program.node_ptr, args)?)
    } else if let Ok(value) = value.downcast::<PyTuple>() {
        if value.len() != 2 {
            return PyResult::Err(PyErr::new::<PyTypeError, _>(
                "Expected a tuple with 2 items",
            ));
        }

        let first = alloc(clvm, value.get_item(0)?)?;
        let rest = alloc(clvm, value.get_item(1)?)?;

        Ok(clvm.new_pair(first, rest)?)
    } else if let Ok(value) = value.downcast::<PyList>() {
        let mut list = Vec::new();

        for item in value.iter() {
            list.push(alloc(clvm, item)?);
        }

        Ok(clvm.new_list(list)?)
    } else {
        PyResult::Err(PyErr::new::<PyTypeError, _>("Unsupported CLVM value type"))
    }
}
