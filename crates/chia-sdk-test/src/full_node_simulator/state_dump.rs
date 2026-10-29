mod restore;

use chia_protocol::{BlockRecord, Bytes32, CoinSpend, SpendBundle};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{SimulatorError, StateDumpError};

use super::{FullNodeSimulator, SimBlock, SimCoinRecord};

const FORMAT: &str = "chia-wallet-sdk/full-node-simulator-state";
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorStateDump {
    format: String,
    version: u32,
    rng: rand_chacha::ChaCha8Rng,
    height: u32,
    next_timestamp: u64,
    header_hashes: Vec<Bytes32>,
    blocks: Vec<DumpBlock>,
    coins: Vec<(Bytes32, SimCoinRecord)>,
    coin_spends: Vec<(Bytes32, CoinSpend)>,
    coin_hints: Vec<(Bytes32, Bytes32)>,
    farming_puzzle_hash: Bytes32,
    master_secret_key: [u8; 32],
    prefarm_puzzle_hash: Bytes32,
    node_id: Bytes32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DumpBlock {
    header_hash: Bytes32,
    record: BlockRecord,
    additions: Vec<Bytes32>,
    removals: Vec<Bytes32>,
    spends: Vec<CoinSpend>,
    transactions: Vec<SpendBundle>,
    previous_coin_records: Vec<(Bytes32, SimCoinRecord)>,
    added_hints: Vec<Bytes32>,
}

impl FullNodeSimulator {
    pub fn dump_state(&self) -> Result<String, SimulatorError> {
        let canonical_coin_ids = self.canonical_coin_ids()?;
        let blocks = self
            .state
            .header_hashes
            .iter()
            .map(|header_hash| {
                let block = self
                    .state
                    .blocks
                    .get(header_hash)
                    .ok_or(StateDumpError::MissingCanonicalBlock(*header_hash))?;
                Ok(DumpBlock::from_block(*header_hash, block))
            })
            .collect::<Result<Vec<_>, SimulatorError>>()?;

        let coins = self
            .state
            .coins
            .iter()
            .filter(|(coin_id, _)| canonical_coin_ids.contains_key(*coin_id))
            .map(|(coin_id, record)| (*coin_id, *record))
            .collect();
        let coin_spends = self
            .state
            .coin_spends
            .iter()
            .filter(|(coin_id, _)| canonical_coin_ids.contains_key(*coin_id))
            .map(|(coin_id, spend)| (*coin_id, spend.clone()))
            .collect();
        let coin_hints = self
            .state
            .coin_hints
            .iter()
            .filter(|(coin_id, _)| canonical_coin_ids.contains_key(*coin_id))
            .map(|(coin_id, hint)| (*coin_id, *hint))
            .collect();

        let dump = SimulatorStateDump {
            format: FORMAT.to_string(),
            version: VERSION,
            rng: self.rng.clone(),
            height: self.state.height,
            next_timestamp: self.state.next_timestamp,
            header_hashes: self.state.header_hashes.clone(),
            blocks,
            coins,
            coin_spends,
            coin_hints,
            farming_puzzle_hash: self.farming_puzzle_hash,
            master_secret_key: self.master_secret_key.to_bytes(),
            prefarm_puzzle_hash: self.prefarm_puzzle_hash,
            node_id: self.node_id,
        };

        serde_json::to_string_pretty(&dump)
            .map_err(|error| StateDumpError::Serialize(error.to_string()).into())
    }

    pub fn restore_state(&mut self, state_json: &str) -> Result<(), SimulatorError> {
        let dump: SimulatorStateDump = serde_json::from_str(state_json)
            .map_err(|error| StateDumpError::Deserialize(error.to_string()))?;
        let restored = FullNodeSimulator::try_from(dump)?;
        *self = restored;
        Ok(())
    }

    fn canonical_coin_ids(&self) -> Result<IndexMap<Bytes32, ()>, SimulatorError> {
        let mut coin_ids = IndexMap::new();
        for header_hash in &self.state.header_hashes {
            let Some(block) = self.state.blocks.get(header_hash) else {
                return Err(StateDumpError::MissingCanonicalBlock(*header_hash).into());
            };
            let mut block_coin_ids = coin_ids.clone();
            for addition in &block.additions {
                block_coin_ids.insert(*addition, ());
            }
            for removal in &block.removals {
                if !block_coin_ids.contains_key(removal) {
                    return Err(StateDumpError::UnsupportedManualCoinSpend(*removal).into());
                }
            }
            coin_ids = block_coin_ids;
        }
        Ok(coin_ids)
    }
}

impl DumpBlock {
    fn from_block(header_hash: Bytes32, block: &SimBlock) -> Self {
        Self {
            header_hash,
            record: block.record.clone(),
            additions: block.additions.clone(),
            removals: block.removals.clone(),
            spends: block.spends.clone(),
            transactions: block.transactions.clone(),
            previous_coin_records: block
                .delta
                .coins
                .iter()
                .filter_map(|change| change.before.map(|record| (change.coin_id, record)))
                .collect(),
            added_hints: block
                .delta
                .hints
                .iter()
                .filter(|change| change.before.is_none() && change.after.is_some())
                .map(|change| change.coin_id)
                .collect(),
        }
    }
}
