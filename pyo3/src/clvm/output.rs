use pyo3::prelude::*;

use super::Program;

#[pyclass(frozen, get_all)]
#[derive(Clone)]
pub struct Output {
    pub value: Program,
    pub cost: u64,
}

#[pymethods]
impl Output {
    #[new]
    pub fn new(value: Program, cost: u64) -> Self {
        Self { value, cost }
    }
}
