use bindy::{FromRust, Pyo3Context};
use num_bigint::BigInt;
use pyo3::{pymethods, PyResult};

bindy_macro::bindy_pyo3!("bindings.json");

#[pymethods]
impl Program {
    pub fn to_int(&self) -> PyResult<Option<BigInt>> {
        Ok(Option::<BigInt>::from_rust(
            self.0.to_big_int()?,
            &Pyo3Context,
        )?)
    }
}
