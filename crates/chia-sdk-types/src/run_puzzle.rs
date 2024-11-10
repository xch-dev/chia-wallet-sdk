use clvmr::{
    chia_dialect::{ENABLE_KECCAK, ENABLE_KECCAK_OPS_OUTSIDE_GUARD},
    reduction::{EvalErr, Reduction},
    Allocator, NodePtr,
};

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, EvalErr> {
    let Reduction(_cost, output) = clvmr::run_program(
        allocator,
        &clvmr::ChiaDialect::new(ENABLE_KECCAK | ENABLE_KECCAK_OPS_OUTSIDE_GUARD),
        puzzle,
        solution,
        11_000_000_000,
    )?;
    Ok(output)
}
