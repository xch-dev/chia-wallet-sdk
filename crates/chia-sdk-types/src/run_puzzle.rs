use clvmr::{Allocator, NodePtr, error::EvalErr, reduction::Reduction, run_program};

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, EvalErr> {
    Ok(run_puzzle_with_cost(allocator, puzzle, solution)?.1)
}

pub fn run_puzzle_with_cost(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<Reduction, EvalErr> {
    run_program(
        allocator,
        &clvmr::ChiaDialect::new(0),
        puzzle,
        solution,
        11_000_000_000,
    )
}
