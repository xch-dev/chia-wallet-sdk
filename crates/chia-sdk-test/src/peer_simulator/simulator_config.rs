use chia_consensus::consensus_constants::ConsensusConstants;
use chia_sdk_types::TESTNET11_CONSTANTS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatorConfig {
    pub constants: ConsensusConstants,
    pub max_subscriptions: usize,
    pub max_response_coins: usize,
    pub puzzle_state_batch_size: usize,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            constants: TESTNET11_CONSTANTS.clone(),
            max_subscriptions: 200_000,
            max_response_coins: 100_000,
            puzzle_state_batch_size: 30_000,
        }
    }
}
