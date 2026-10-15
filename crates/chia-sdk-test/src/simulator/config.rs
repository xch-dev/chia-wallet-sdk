use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulatorConfig {
    pub seed: u64,
    pub timestamp_mode: TimestampMode,
    pub save_spends: bool,
    pub save_hints: bool,
    /// Validates transactions with the stricter mempool rules that mainnet nodes apply before
    /// accepting a transaction (unknown operators and conditions are rejected, for example).
    /// Disable this to validate with the more lenient rules that only apply to blocks.
    pub mempool_mode: bool,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            seed: 1337,
            timestamp_mode: TimestampMode::default(),
            save_spends: true,
            save_hints: true,
            mempool_mode: true,
        }
    }
}

/// Determines the timestamps of blocks created by the simulator.
///
/// [`Simulator::pass_time`](crate::Simulator::pass_time) and
/// [`Simulator::set_next_timestamp`](crate::Simulator::set_next_timestamp) override the timestamp
/// of the next block regardless of the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampMode {
    /// The genesis block has the given timestamp, and time doesn't advance between blocks.
    Fixed(u64),
    /// The genesis block has the timestamp `start`, and each block advances it by `step`.
    Increment { start: u64, step: u64 },
    /// Each block uses the current system time, without going backwards.
    RealTime,
}

impl Default for TimestampMode {
    fn default() -> Self {
        Self::Increment { start: 0, step: 1 }
    }
}

impl TimestampMode {
    pub(crate) fn genesis_timestamp(self) -> u64 {
        match self {
            Self::Fixed(timestamp) => timestamp,
            Self::Increment { start, .. } => start,
            Self::RealTime => now(),
        }
    }

    pub(crate) fn next_timestamp(self, previous: u64) -> u64 {
        match self {
            Self::Fixed(_) => previous,
            Self::Increment { step, .. } => previous.saturating_add(step),
            Self::RealTime => now().max(previous),
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_secs()
}
