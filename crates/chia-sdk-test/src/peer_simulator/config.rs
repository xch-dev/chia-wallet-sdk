use chia_protocol::Bytes32;
use chia_sdk_types::TESTNET11_CONSTANTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerSimulatorConfig {
    pub genesis_challenge: Bytes32,
    pub max_subscriptions: usize,
    pub max_response_coins: usize,
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
