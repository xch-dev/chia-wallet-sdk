use chia_protocol::{BlockRecord, Bytes32, CoinSpend};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::SimulatorError;

use super::{FullNodeSimulator, SimBlock, SimCoinRecord, ValidatedBundle};

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
    transactions: Vec<chia_protocol::SpendBundle>,
    previous_coin_records: Vec<(Bytes32, SimCoinRecord)>,
    added_hints: Vec<Bytes32>,
}

impl FullNodeSimulator {
    pub fn dump_state(&self) -> Result<String, SimulatorError> {
        let canonical_coin_ids = self.canonical_coin_ids()?;
        let blocks = self
            .header_hashes
            .iter()
            .map(|header_hash| {
                let block = self.blocks.get(header_hash).ok_or_else(|| {
                    SimulatorError::Custom(format!(
                        "missing canonical block {}",
                        hex::encode(header_hash.to_bytes())
                    ))
                })?;
                Ok(DumpBlock::from_block(*header_hash, block))
            })
            .collect::<Result<Vec<_>, SimulatorError>>()?;

        let coins = self
            .coins
            .iter()
            .filter(|(coin_id, _)| canonical_coin_ids.contains_key(*coin_id))
            .map(|(coin_id, record)| (*coin_id, *record))
            .collect();
        let coin_spends = self
            .coin_spends
            .iter()
            .filter(|(coin_id, _)| canonical_coin_ids.contains_key(*coin_id))
            .map(|(coin_id, spend)| (*coin_id, spend.clone()))
            .collect();
        let coin_hints = self
            .coin_hints
            .iter()
            .filter(|(coin_id, _)| canonical_coin_ids.contains_key(*coin_id))
            .map(|(coin_id, hint)| (*coin_id, *hint))
            .collect();

        let dump = SimulatorStateDump {
            format: FORMAT.to_string(),
            version: VERSION,
            rng: self.rng.clone(),
            height: self.height,
            next_timestamp: self.next_timestamp,
            header_hashes: self.header_hashes.clone(),
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
            .map_err(|error| SimulatorError::Custom(error.to_string()))
    }

    pub fn restore_state(&mut self, state_json: &str) -> Result<(), SimulatorError> {
        let dump: SimulatorStateDump = serde_json::from_str(state_json)
            .map_err(|error| SimulatorError::Custom(error.to_string()))?;
        let restored = FullNodeSimulator::try_from(dump)?;
        *self = restored;
        Ok(())
    }

    fn canonical_coin_ids(&self) -> Result<IndexMap<Bytes32, ()>, SimulatorError> {
        let mut coin_ids = IndexMap::new();
        for header_hash in &self.header_hashes {
            let Some(block) = self.blocks.get(header_hash) else {
                return Err(SimulatorError::Custom(format!(
                    "missing canonical block {}",
                    hex::encode(header_hash.to_bytes())
                )));
            };
            let mut block_coin_ids = coin_ids.clone();
            for addition in &block.additions {
                block_coin_ids.insert(*addition, ());
            }
            for removal in &block.removals {
                if !block_coin_ids.contains_key(removal) {
                    return Err(SimulatorError::Custom(format!(
                        "cannot dump state with canonical spend of unsupported manual coin {}",
                        hex::encode(removal.to_bytes())
                    )));
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
            previous_coin_records: block.previous_coin_records.clone(),
            added_hints: block.added_hints.clone(),
        }
    }

    fn into_block(self) -> Result<(Bytes32, SimBlock), SimulatorError> {
        if self.header_hash != self.record.header_hash {
            return Err(SimulatorError::Custom(format!(
                "block key {} does not match record header hash {}",
                hex::encode(self.header_hash.to_bytes()),
                hex::encode(self.record.header_hash.to_bytes())
            )));
        }

        Ok((
            self.header_hash,
            SimBlock {
                record: self.record,
                additions: self.additions,
                removals: self.removals,
                spends: self.spends,
                transactions: self.transactions,
                previous_coin_records: self.previous_coin_records,
                added_hints: self.added_hints,
            },
        ))
    }
}

impl TryFrom<SimulatorStateDump> for FullNodeSimulator {
    type Error = SimulatorError;

    fn try_from(dump: SimulatorStateDump) -> Result<Self, Self::Error> {
        if dump.format != FORMAT {
            return Err(SimulatorError::Custom(format!(
                "unsupported full node simulator state format {}",
                dump.format
            )));
        }
        if dump.version != VERSION {
            return Err(SimulatorError::Custom(format!(
                "unsupported full node simulator state version {}",
                dump.version
            )));
        }
        if dump.height as usize != dump.header_hashes.len() {
            return Err(SimulatorError::Custom(format!(
                "height {} does not match {} header hashes",
                dump.height,
                dump.header_hashes.len()
            )));
        }

        let mut blocks = IndexMap::new();
        for block in dump.blocks {
            let (header_hash, block) = block.into_block()?;
            blocks.insert(header_hash, block);
        }
        for header_hash in &dump.header_hashes {
            if !blocks.contains_key(header_hash) {
                return Err(SimulatorError::Custom(format!(
                    "missing block for canonical header {}",
                    hex::encode(header_hash.to_bytes())
                )));
            }
        }

        let master_secret_key = chia_bls::SecretKey::from_bytes(&dump.master_secret_key)
            .map_err(|error| SimulatorError::Custom(error.to_string()))?;

        Ok(Self {
            rng: dump.rng,
            height: dump.height,
            next_timestamp: dump.next_timestamp,
            header_hashes: dump.header_hashes,
            blocks,
            orphaned_blocks: IndexMap::new(),
            coins: dump.coins.into_iter().collect(),
            coin_spends: dump.coin_spends.into_iter().collect(),
            coin_hints: dump.coin_hints.into_iter().collect(),
            mempool: IndexMap::<Bytes32, ValidatedBundle>::new(),
            farming_puzzle_hash: dump.farming_puzzle_hash,
            master_secret_key,
            prefarm_puzzle_hash: dump.prefarm_puzzle_hash,
            node_id: dump.node_id,
            events: Vec::new(),
        })
    }
}
