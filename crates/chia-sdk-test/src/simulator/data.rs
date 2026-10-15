use chia_protocol::{Bytes32, CoinSpend, CoinState};
use indexmap::{IndexMap, IndexSet};
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SimulatorData {
    pub(crate) rng: ChaCha8Rng,
    /// The height of the peak block. Transactions are included in the block after it.
    pub(crate) height: u32,
    pub(crate) next_timestamp: u64,
    pub(crate) header_hashes: Vec<Bytes32>,
    pub(crate) coin_states: IndexMap<Bytes32, CoinState>,
    pub(crate) coin_spends: IndexMap<Bytes32, CoinSpend>,
    /// The timestamp of each block, keyed by height.
    pub(crate) block_timestamps: IndexMap<u32, u64>,
    pub(crate) hinted_coins: IndexMap<Bytes32, IndexSet<Bytes32>>,
}

impl SimulatorData {
    pub(crate) fn new(rng: ChaCha8Rng, genesis_timestamp: u64, next_timestamp: u64) -> Self {
        Self {
            rng,
            height: 0,
            next_timestamp,
            header_hashes: vec![Bytes32::default()],
            coin_states: IndexMap::new(),
            coin_spends: IndexMap::new(),
            block_timestamps: IndexMap::from([(0, genesis_timestamp)]),
            hinted_coins: IndexMap::new(),
        }
    }
}
