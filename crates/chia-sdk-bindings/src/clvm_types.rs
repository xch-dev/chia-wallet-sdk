use crate::Program;

#[derive(Clone)]
pub struct Spend {
    pub puzzle: Program,
    pub solution: Program,
}

#[derive(Clone)]
pub struct Output {
    pub value: Program,
    pub cost: u64,
}

#[derive(Clone)]
pub struct Pair {
    pub first: Program,
    pub rest: Program,
}

#[derive(Clone)]
pub struct CurriedProgram {
    pub program: Program,
    pub args: Vec<Program>,
}
