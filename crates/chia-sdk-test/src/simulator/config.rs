#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulatorConfig {
    pub seed: u64,
    pub timestamp_mode: TimestampMode,
    pub save_spends: bool,
    pub save_hints: bool,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            seed: 1337,
            timestamp_mode: TimestampMode::default(),
            save_spends: true,
            save_hints: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampMode {
    Fixed(u64),
    Increment { start: u64, step: u64 },
    RealTime,
}

impl Default for TimestampMode {
    fn default() -> Self {
        Self::Increment { start: 0, step: 1 }
    }
}
