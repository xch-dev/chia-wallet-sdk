use pyo3::prelude::*;

use super::Program;

#[pyclass(frozen, get_all)]
#[derive(Clone)]
pub struct Spend {
    pub puzzle: Program,
    pub solution: Program,
}

#[pymethods]
impl Spend {
    #[new]
    pub fn new(puzzle: Program, solution: Program) -> Self {
        Self { puzzle, solution }
    }
}
