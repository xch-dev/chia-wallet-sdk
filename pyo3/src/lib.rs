use bindy::{FromRust, IntoRust, Pyo3Context};
use num_bigint::BigInt;
use pyo3::{pymethods, PyResult};

bindy_macro::bindy_pyo3!("bindings.json");

#[pymethods]
impl Clvm {
    pub fn int(&self, value: BigInt) -> PyResult<Program> {
        Ok(Program::from_rust(
            self.0.big_int(value.into_rust(&Pyo3Context)?)?,
            &Pyo3Context,
        )?)
    }
}

#[pymethods]
impl Program {
    pub fn to_int(&self) -> PyResult<Option<BigInt>> {
        Ok(Option::<BigInt>::from_rust(
            self.0.to_big_int()?,
            &Pyo3Context,
        )?)
    }
}
