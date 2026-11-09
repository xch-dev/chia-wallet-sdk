use chia_protocol::Bytes32;
use chia_sdk_types::TESTNET11_CONSTANTS;

/// Options for a [`PeerSimulator`](crate::PeerSimulator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerSimulatorConfig {
    /// The header hash that coin and puzzle state requests must use when they don't specify a
    /// previous height. Other values are rejected as a reorg. Defaults to the testnet11 genesis
    /// challenge.
    pub genesis_challenge: Bytes32,
    /// The maximum number of coin and puzzle subscriptions per peer. Requests that would exceed it
    /// are rejected. Defaults to 200,000.
    pub max_subscriptions: usize,
    /// The maximum number of coin states that a puzzle state request collects before it is split
    /// into batches. Defaults to 100,000.
    pub max_response_coins: usize,
    /// If a puzzle state request finds more coin states than this, the response is unfinished and
    /// ends before the height of the last coin state found. Defaults to 30,000.
    pub puzzle_state_batch_size: usize,
}

impl Default for PeerSimulatorConfig {
    fn default() -> Self {
        Self {
            genesis_challenge: TESTNET11_CONSTANTS.genesis_challenge,
            max_subscriptions: 200_000,
            max_response_coins: 100_000,
            puzzle_state_batch_size: 30_000,
        }
    }
}
