use clvmr::{
    Allocator, ChiaDialect, ENABLE_KECCAK_OPS_OUTSIDE_GUARD, MEMPOOL_MODE, NodePtr, error::EvalErr,
    reduction::Reduction, run_program,
};
use rue_lir::DebugDialect;

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, EvalErr> {
    Ok(run_puzzle_with_cost(allocator, puzzle, solution, u64::MAX, false)?.1)
}

pub fn run_puzzle_with_cost(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
    max_cost: u64,
    mempool_mode: bool,
) -> Result<Reduction, EvalErr> {
    let mut flags = ENABLE_KECCAK_OPS_OUTSIDE_GUARD;

    if mempool_mode {
        flags |= MEMPOOL_MODE;
    }

    if is_debug_dialect_enabled() {
        run_program(
            allocator,
            &DebugDialect::new(flags, true),
            puzzle,
            solution,
            max_cost,
        )
    } else {
        run_program(
            allocator,
            &ChiaDialect::new(flags),
            puzzle,
            solution,
            max_cost,
        )
    }
}

pub fn is_debug_dialect_enabled() -> bool {
    if cfg!(debug_assertions) || cfg!(test) {
        return true;
    }

    match option_env!("DEBUG_CLVM") {
        Some(value) => {
            // Convert the value to lowercase for case-insensitive comparison
            let lowercased_value = value.to_lowercase();
            // Consider "true", "yes", "1" as enabled
            lowercased_value == "true" || lowercased_value == "yes" || lowercased_value == "1"
        }
        None => {
            // Environment variable is not set, so it's not "enabled"
            false
        }
    }
}
