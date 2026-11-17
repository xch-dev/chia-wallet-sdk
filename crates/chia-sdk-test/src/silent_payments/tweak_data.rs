//! [`TweakData`] from a block of the simulator.

use chia_sdk_driver::silent_payments::{TweakData, tweak_data_from_block_spends};

use crate::Simulator;

/// The [`TweakData`] of the simulator's block at `height`, from [`tweak_data_from_block_spends`].
/// It is empty if the block has no eligible spends.
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
