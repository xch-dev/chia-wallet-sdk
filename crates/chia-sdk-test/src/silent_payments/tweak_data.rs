//! Build a [`TweakData`] from a single simulator block.
//!
//! This module ships a thin adapter over the canonical real-block builder
//! [`chia_sdk_driver::silent_payments::tweak_data_from_block_spends`]. The
//! adapter exists so that simulator-driven tests (in this crate and across
//! the binding suites) can construct `TweakData` from a height without
//! plumbing `block_spends` / `block_outputs` through every call site.
//! Production receive callers go directly through
//! `tweak_data_from_block_spends`.
//!
//! See the driver-side module-level docs for the grouping algorithm
//! (per-spend Pass-1 singletons + Pass-2 `AssertConcurrentSpend` SCC), the
//! BLS12-381 identity-element guard, and the non-standard-puzzle skip
//! rule. The adapter inherits all of those behaviours.

use chia_sdk_driver::silent_payments::{TweakData, tweak_data_from_block_spends};

use crate::Simulator;

/// Construct a [`TweakData`] from one block of the simulator's history.
///
/// Thin adapter over [`tweak_data_from_block_spends`] — fetches the block's
/// coin spends and outputs via the simulator's `block_spends` / `block_outputs`
/// accessors and forwards them as slices.
///
/// Returns an empty `TweakData` when the block contains no standard-puzzle
/// spends or when `height` is out of range (the simulator's `block_spends`
/// returns an empty `Vec` for any unused height).
///
/// **For test / binding-suite consumers only** — production receive callers
/// construct `Vec<CoinSpend>` + `Vec<Coin>` from their RPC source and call
/// [`tweak_data_from_block_spends`] directly, skipping the `Simulator`
/// dependency.
#[must_use]
pub fn tweak_data_from_simulator_block(sim: &Simulator, height: u32) -> TweakData {
    let spends = sim.block_spends(height);
    let outputs = sim.block_outputs(height);
    tweak_data_from_block_spends(&spends, &outputs).expect(
        "tweak_data_from_block_spends never errors on simulator-shaped inputs (all spends are \
         well-formed by construction)",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Defensive guard: empty block → empty `TweakData`, no panic.
    #[test]
    fn tweak_data_empty_block_returns_empty_tweak_data() {
        let sim = Simulator::new();
        let td = tweak_data_from_simulator_block(&sim, 0);
        assert!(td.tweak_points.is_empty(), "no spends → no tweak_points");
        assert!(td.outputs.is_empty(), "no outputs → empty outputs");
    }

    /// Defensive guard: out-of-range height → empty `TweakData`, no panic.
    #[test]
    fn tweak_data_genesis_height_is_safe() {
        let sim = Simulator::new();
        let td = tweak_data_from_simulator_block(&sim, 9999);
        assert!(td.tweak_points.is_empty());
        assert!(td.outputs.is_empty());
    }
}
