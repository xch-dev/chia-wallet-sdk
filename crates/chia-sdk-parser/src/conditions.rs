use chia_sdk_types::conditions::Condition;
use clvm_traits::FromClvm;
use clvmr::{reduction::Reduction, Allocator, NodePtr};

use crate::ParseError;

pub fn parse_conditions(
    allocator: &mut Allocator,
    conditions: NodePtr,
) -> Result<Vec<Condition<NodePtr>>, ParseError> {
    Vec::<NodePtr>::from_clvm(allocator, conditions)?
        .into_iter()
        .map(|condition| Ok(Condition::from_clvm(allocator, condition)?))
        .collect()
}

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, ParseError> {
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
) -> Result<Vec<Condition<NodePtr>>, ParseError> {
    let output = run_puzzle(allocator, puzzle, solution)?;
    parse_conditions(allocator, output)
}
