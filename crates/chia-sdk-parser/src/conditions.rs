use chia_sdk_types::conditions::Condition;
use clvm_traits::{FromClvm, FromClvmError};
use clvmr::{
    reduction::{EvalErr, Reduction},
    Allocator, NodePtr,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConditionError {
    #[error("eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("clvm error: {0}")]
    Clvm(#[from] FromClvmError),
}

pub fn parse_conditions(
    allocator: &mut Allocator,
    conditions: NodePtr,
) -> Result<Vec<Condition<NodePtr>>, FromClvmError> {
    Vec::<NodePtr>::from_clvm(allocator, conditions)?
        .into_iter()
        .map(|condition| Condition::from_clvm(allocator, condition))
        .collect()
}

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, EvalErr> {
    let Reduction(_cost, output) = clvmr::run_program(
        allocator,
        &clvmr::ChiaDialect::new(0),
        puzzle,
        solution,
        11_000_000_000,
    )?;
    Ok(output)
}

pub fn puzzle_conditions(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<Vec<Condition<NodePtr>>, ConditionError> {
    let output = run_puzzle(allocator, puzzle, solution)?;
    Ok(parse_conditions(allocator, output)?)
}
