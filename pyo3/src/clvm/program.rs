use std::sync::Arc;

use clvmr::NodePtr;
use num_bigint::BigInt;
use parking_lot::RwLock;
use pyo3::prelude::*;

use super::{CurriedProgram, Output};

#[pyclass]
#[derive(Clone)]
pub struct Program {
    pub(crate) clvm: Arc<RwLock<chia_sdk_bindings::Clvm>>,
    pub(crate) node_ptr: NodePtr,
}

#[pymethods]
impl Program {
    #[getter]
    pub fn is_atom(&self) -> bool {
        self.node_ptr.is_atom()
    }

    #[getter]
    pub fn is_pair(&self) -> bool {
        self.node_ptr.is_pair()
    }

    pub fn __len__(&self) -> PyResult<usize> {
        Ok(self.clvm.read().length(self.node_ptr)?)
    }

    #[getter]
    pub fn first(&self) -> PyResult<Program> {
        Ok(Program {
            clvm: self.clvm.clone(),
            node_ptr: self.clvm.read().first(self.node_ptr)?,
        })
    }

    #[getter]
    pub fn rest(&self) -> PyResult<Program> {
        Ok(Program {
            clvm: self.clvm.clone(),
            node_ptr: self.clvm.read().rest(self.node_ptr)?,
        })
    }

    pub fn serialize(&self) -> PyResult<Vec<u8>> {
        Ok(self.clvm.read().serialize(self.node_ptr)?.into_bytes())
    }

    pub fn serialize_with_backrefs(&self) -> PyResult<Vec<u8>> {
        Ok(self
            .clvm
            .read()
            .serialize_with_backrefs(self.node_ptr)?
            .into_bytes())
    }

    pub fn tree_hash(&self) -> PyResult<Vec<u8>> {
        Ok(self.clvm.read().tree_hash(self.node_ptr)?.to_vec())
    }

    pub fn to_int(&self) -> PyResult<Option<BigInt>> {
        Ok(self.clvm.read().as_bigint(self.node_ptr)?)
    }

    pub fn to_string(&self) -> PyResult<Option<String>> {
        Ok(self.clvm.read().as_string(self.node_ptr)?)
    }

    pub fn to_bool(&self) -> PyResult<Option<bool>> {
        Ok(self.clvm.read().as_bool(self.node_ptr)?)
    }

    pub fn to_bytes(&self) -> PyResult<Option<Vec<u8>>> {
        Ok(self
            .clvm
            .read()
            .as_atom(self.node_ptr)?
            .map(|bytes| bytes.into_inner()))
    }

    pub fn to_pair(&self) -> PyResult<Option<(Program, Program)>> {
        let Some((first, rest)) = self.clvm.read().as_pair(self.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some((
            Program {
                clvm: self.clvm.clone(),
                node_ptr: first,
            },
            Program {
                clvm: self.clvm.clone(),
                node_ptr: rest,
            },
        )))
    }

    pub fn to_list(&self) -> PyResult<Option<Vec<Program>>> {
        let Some(list) = self.clvm.read().as_list(self.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(
            list.iter()
                .map(|&node_ptr| Program {
                    clvm: self.clvm.clone(),
                    node_ptr,
                })
                .collect(),
        ))
    }

    pub fn curry(&mut self, args: Vec<Program>) -> PyResult<Program> {
        let mut clvm = self.clvm.write();
        let node_ptr = clvm.curry(self.node_ptr, args.iter().map(|p| p.node_ptr).collect())?;
        Ok(Program {
            clvm: self.clvm.clone(),
            node_ptr,
        })
    }

    pub fn uncurry(&self) -> PyResult<Option<CurriedProgram>> {
        let Some((program, args)) = self.clvm.read().uncurry(self.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(CurriedProgram {
            program: Program {
                clvm: self.clvm.clone(),
                node_ptr: program,
            },
            args: args
                .iter()
                .map(|&node_ptr| Program {
                    clvm: self.clvm.clone(),
                    node_ptr,
                })
                .collect(),
        }))
    }

    pub fn run(&self, solution: &Program, max_cost: u64, mempool_mode: bool) -> PyResult<Output> {
        let output =
            self.clvm
                .write()
                .run(self.node_ptr, solution.node_ptr, max_cost, mempool_mode)?;
        Ok(Output {
            value: Program {
                clvm: self.clvm.clone(),
                node_ptr: output.1,
            },
            cost: output.0,
        })
    }
}
