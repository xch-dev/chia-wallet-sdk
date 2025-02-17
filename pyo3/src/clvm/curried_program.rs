use pyo3::prelude::*;

use super::Program;

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
