use clvmr::NodePtr;
use num_bigint::BigInt;
use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyList, PyNone, PyTuple},
};

use crate::{
    bls::{PublicKey, Signature},
    secp::{K1PublicKey, K1Signature, R1PublicKey, R1Signature},
};

use super::{CurriedProgram, Program};

pub fn alloc(clvm: &mut chia_sdk_bindings::Clvm, value: Bound<'_, PyAny>) -> PyResult<NodePtr> {
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
