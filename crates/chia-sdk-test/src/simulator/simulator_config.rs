use chia_protocol::Bytes32;
use hex_literal::hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulatorConfig {
    pub genesis_challenge: Bytes32,
    pub max_subscriptions: usize,
    pub max_response_coins: usize,
    pub puzzle_state_batch_size: usize,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            genesis_challenge: Bytes32::new(hex!(
                "ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb"
            )),
            max_subscriptions: 200_000,
            max_response_coins: 100_000,
            puzzle_state_batch_size: 30_000,
        }
    }
}
