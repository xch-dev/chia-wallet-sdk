use clvmr::{
    Allocator, ENABLE_KECCAK_OPS_OUTSIDE_GUARD, NodePtr, error::EvalErr, reduction::Reduction,
    run_program,
};
use rue_lir::DebugDialect;

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
        &DebugDialect::new(ENABLE_KECCAK_OPS_OUTSIDE_GUARD, true),
        puzzle,
        solution,
        11_000_000_000,
    )
}
