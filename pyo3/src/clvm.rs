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
    secp::{K1PublicKey, K1Signature, R1PublicKey, R1Signature},
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
}

#[pyclass]
#[derive(Clone)]
pub struct Program {
    clvm: Arc<RwLock<chia_sdk_bindings::Clvm>>,
    node_ptr: NodePtr,
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

    pub fn to_atom(&self) -> PyResult<Option<Vec<u8>>> {
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
}

#[pyclass(frozen, get_all)]
#[derive(Clone)]
pub struct CurriedProgram {
    pub program: Program,
    pub args: Vec<Program>,
}

#[pymethods]
impl CurriedProgram {
    #[new]
    pub fn new(program: Program, args: Vec<Program>) -> Self {
        Self { program, args }
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
