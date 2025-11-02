use clvmr::{
    Allocator, ENABLE_KECCAK_OPS_OUTSIDE_GUARD, NodePtr, error::EvalErr, reduction::Reduction,
};
use rue_lir::DebugDialect;

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, EvalErr> {
    let Reduction(_cost, output) = clvmr::run_program(
        allocator,
        &DebugDialect::new(ENABLE_KECCAK_OPS_OUTSIDE_GUARD, true),
        puzzle,
        solution,
        11_000_000_000,
    )?;
    Ok(output)
}
