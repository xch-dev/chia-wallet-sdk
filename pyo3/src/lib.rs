#![allow(clippy::too_many_arguments)]

use bindy::{FromRust, IntoRust, Pyo3, Pyo3Context};
use num_bigint::BigInt;
use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyList, PyNone, PyTuple},
};

bindy_macro::bindy_pyo3!("bindings.json");

#[pymethods]
impl Clvm {
    pub fn int(&self, value: BigInt) -> PyResult<Program> {
        Ok(Program::from_rust(
            self.0
                .big_int(IntoRust::<_, _, Pyo3>::into_rust(value, &Pyo3Context)?)?,
            &Pyo3Context,
        )?)
    }
}

#[pymethods]
impl Program {
    pub fn to_int(&self) -> PyResult<Option<BigInt>> {
        Ok(<Option<BigInt> as FromRust<_, _, Pyo3>>::from_rust(
            self.0.to_big_int()?,
            &Pyo3Context,
        )?)
    }
}

pub fn alloc(
    clvm: &chia_sdk_bindings::Clvm,
    value: Bound<'_, PyAny>,
) -> PyResult<chia_sdk_bindings::Program> {
    if let Ok(_value) = value.downcast::<PyNone>() {
        Ok(clvm.nil()?)
    } else if let Ok(value) = value.extract::<BigInt>() {
        Ok(clvm.big_int(value)?)
    } else if let Ok(value) = value.extract::<bool>() {
        Ok(clvm.bool(value)?)
    } else if let Ok(value) = value.extract::<String>() {
        Ok(clvm.string(value)?)
    } else if let Ok(value) = value.extract::<Vec<u8>>() {
        Ok(clvm.atom(value.into())?)
    } else if let Ok(value) = value.extract::<Program>() {
        Ok(value.0)
    } else if let Ok(value) = value.extract::<PublicKey>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<Signature>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<K1PublicKey>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<K1Signature>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<R1PublicKey>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<R1Signature>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<CurriedProgram>() {
        Ok(value.0.program.curry(value.0.args.clone())?)
    } else if let Ok(value) = value.downcast::<PyTuple>() {
        if value.len() != 2 {
            return PyResult::Err(PyErr::new::<PyTypeError, _>(
                "Expected a tuple with 2 items",
            ));
        }

        let first = alloc(clvm, value.get_item(0)?)?;
        let rest = alloc(clvm, value.get_item(1)?)?;

        Ok(clvm.pair(first, rest)?)
    } else if let Ok(value) = value.downcast::<PyList>() {
        let mut list = Vec::new();

        for item in value.iter() {
            list.push(alloc(clvm, item)?);
        }

        Ok(clvm.list(list)?)
    } else {
        PyResult::Err(PyErr::new::<PyTypeError, _>("Unsupported CLVM value type"))
    }
}
