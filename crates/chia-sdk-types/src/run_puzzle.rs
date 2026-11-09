use clvmr::{
    Allocator, ChiaDialect, ClvmFlags, MEMPOOL_MODE, NodePtr, error::EvalErr, reduction::Reduction,
    run_program,
};
use rue_lir::DebugDialect;

use crate::MAINNET_CONSTANTS;

/// Runs a puzzle with a solution and returns the output, which is usually a list of conditions.
///
/// This uses the cost limit of a mainnet block ([`max_block_cost_clvm`]) and doesn't enable
/// mempool mode, so it's lenient enough to run any puzzle that could be included on chain. See
/// [`run_puzzle_with_cost`] for more control, and [`is_debug_dialect_enabled`] for which dialect
/// is used.
///
/// [`max_block_cost_clvm`]:
/// chia_consensus::consensus_constants::ConsensusConstants::max_block_cost_clvm
///
/// ## Example
///
/// ```rust
/// # use chia_sdk_types::{Condition, Conditions, run_puzzle};
/// # use clvm_traits::{FromClvm, ToClvm, clvm_quote};
/// # use clvmr::{Allocator, NodePtr};
/// let mut allocator = Allocator::new();
///
/// // A quoted program returns its value without evaluating it.
/// let puzzle = clvm_quote!(Conditions::new().reserve_fee(100)).to_clvm(&mut allocator)?;
/// let output = run_puzzle(&mut allocator, puzzle, NodePtr::NIL)?;
///
/// let conditions = Vec::<Condition>::from_clvm(&allocator, output)?;
/// assert_eq!(conditions, vec![Condition::reserve_fee(100)]);
/// # Ok::<(), anyhow::Error>(())
/// ```
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

/// Runs a puzzle with a solution, a cost limit, and optionally in mempool mode, and returns the
/// cost along with the output.
///
/// A `max_cost` of `0` means there is no limit.
/// Mempool mode ([`MEMPOOL_MODE`](clvmr::MEMPOOL_MODE)) is stricter than block validation. It
/// rejects unknown operators (which are otherwise allowed so that new ones can be added in soft
/// forks), limits the heap size, and requires integer arguments to operators to be canonical
/// (without redundant leading zeros), among other limits. The
/// `ENABLE_KECCAK_OPS_OUTSIDE_GUARD` flag is always set, so the `keccak256` operator can be used
/// outside of the softfork guard. See [`is_debug_dialect_enabled`] for which dialect is used.
///
/// ## Example
///
/// ```rust
/// # use chia_sdk_types::run_puzzle_with_cost;
/// # use clvm_traits::{ToClvm, clvm_quote};
/// # use clvmr::{Allocator, NodePtr, reduction::Reduction};
/// let mut allocator = Allocator::new();
/// let puzzle = clvm_quote!(42).to_clvm(&mut allocator)?;
///
/// let Reduction(cost, output) =
///     run_puzzle_with_cost(&mut allocator, puzzle, NodePtr::NIL, 1_000_000, true)?;
/// assert!(cost > 0);
/// assert_eq!(allocator.small_number(output), Some(42));
///
/// // The puzzle fails if it costs more than the limit.
/// assert!(run_puzzle_with_cost(&mut allocator, puzzle, NodePtr::NIL, cost - 1, true).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run_puzzle_with_cost(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
    max_cost: u64,
    mempool_mode: bool,
) -> Result<Reduction, EvalErr> {
    let mut flags = ClvmFlags::ENABLE_KECCAK_OPS_OUTSIDE_GUARD;

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

/// Returns whether [`run_puzzle`] and [`run_puzzle_with_cost`] use the debug dialect.
///
/// The debug dialect behaves like the normal Chia dialect, except that it also implements the
/// debug print operator emitted by Rue's `debug` statements, which prints the value to stderr.
///
/// It's enabled when this crate is compiled with debug assertions (such as in the default `dev`
/// profile), or when the `DEBUG_CLVM` environment variable is set to `true`, `yes`, or `1`
/// (case-insensitive) at compile time. The variable is read with [`option_env!`], so setting it
/// when the program runs has no effect.
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
