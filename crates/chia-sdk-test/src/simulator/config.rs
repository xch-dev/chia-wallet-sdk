/// Options for creating a [`Simulator`](crate::Simulator).
///
/// # Example
///
/// ```
/// use chia_sdk_test::{Simulator, SimulatorConfig};
///
/// // Use a different seed and don't keep coin spends around.
/// let sim = Simulator::with_config(SimulatorConfig {
///     seed: 42,
///     save_spends: false,
///     ..Default::default()
/// });
/// assert_eq!(sim.height(), 0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulatorConfig {
    /// The seed for the random number generator used for coin parent ids, key pairs, and header
    /// hashes. Defaults to `1337`.
    pub seed: u64,
    /// The intended timestamp behavior. Defaults to [`TimestampMode::Increment`] starting at 0
    /// with a step of 1.
    ///
    /// This field is currently not read by the [`Simulator`](crate::Simulator), which always
    /// starts at timestamp 0 and increments by one second per block.
    pub timestamp_mode: TimestampMode,
    /// Whether to save the coin spends of each transaction, so they can be looked up with methods
    /// like [`Simulator::coin_spend`](crate::Simulator::coin_spend). Defaults to `true`.
    pub save_spends: bool,
    /// Whether to save hints from the memos of created coins, so coins can be found with
    /// [`Simulator::hinted_coins`](crate::Simulator::hinted_coins) and hint lookups. Defaults to
    /// `true`.
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

/// How block timestamps should advance, as configured by [`SimulatorConfig::timestamp_mode`].
///
/// This setting is currently not read by the [`Simulator`](crate::Simulator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampMode {
    /// Every block has the same timestamp.
    Fixed(u64),
    /// Timestamps start at a value and increase by a fixed amount each block. This is the default,
    /// with `start: 0` and `step: 1`.
    Increment {
        /// The timestamp of the first block, in seconds.
        start: u64,
        /// The number of seconds added for each block.
        step: u64,
    },
    /// Timestamps follow the system clock.
    RealTime,
}

impl Default for TimestampMode {
    fn default() -> Self {
        Self::Increment { start: 0, step: 1 }
    }
}
