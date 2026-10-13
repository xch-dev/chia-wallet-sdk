use std::sync::atomic::{AtomicBool, Ordering};

use clvmr::{
    Allocator, ChiaDialect, ENABLE_KECCAK_OPS_OUTSIDE_GUARD, MEMPOOL_MODE, NodePtr, error::EvalErr,
    reduction::Reduction, run_program,
};
use rue_lir::DebugDialect;

use crate::MAINNET_CONSTANTS;

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, EvalErr> {
    Ok(run_puzzle_with_cost(
        allocator,
        puzzle,
        solution,
        MAINNET_CONSTANTS.max_block_cost_clvm,
        false,
    )?
    .1)
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

static DEBUG_DIALECT_ENABLED: AtomicBool = AtomicBool::new(false);

/// Opts in to running puzzles with Rue's debug dialect, which supports debug print operators.
///
/// This applies to the whole process. It's disabled by default, regardless of build profile, so
/// that puzzles run with the same dialect as mainnet unless debugging is explicitly requested.
pub fn set_debug_dialect_enabled(enabled: bool) {
    DEBUG_DIALECT_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether puzzles are run with Rue's debug dialect.
///
/// This is enabled either by [`set_debug_dialect_enabled`], or by setting the `DEBUG_CLVM`
/// environment variable to `true`, `yes`, or `1` when compiling this crate. Note that `DEBUG_CLVM`
/// is read with `option_env!`, so it must be set at build time and has no effect at runtime.
pub fn is_debug_dialect_enabled() -> bool {
    DEBUG_DIALECT_ENABLED.load(Ordering::Relaxed)
        || option_env!("DEBUG_CLVM")
            .is_some_and(|value| matches!(value.to_lowercase().as_str(), "true" | "yes" | "1"))
}
