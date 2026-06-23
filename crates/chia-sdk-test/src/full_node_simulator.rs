use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use bip39::Mnemonic;
use chia_bls::{SecretKey, master_to_wallet_hardened};
use chia_consensus::{
    conditions::{ELIGIBLE_FOR_DEDUP, ELIGIBLE_FOR_FF},
    fast_forward::fast_forward_singleton,
    flags::COMPUTE_FINGERPRINT,
    validation_error::ErrorCode,
};
use chia_protocol::{BlockRecord, Bytes32, ClassgroupElement, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainState, BlockchainStateResponse, CoinRecord,
    GetBlockRecordResponse, GetBlockRecordsResponse, GetBlockSpendsResponse, GetCoinRecordResponse,
    GetCoinRecordsResponse, GetMempoolItemResponse, GetMempoolItemsResponse,
    GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, MempoolItem, MempoolMinFees,
    PushTxResponse, SyncState,
};
use chia_sdk_types::default_constants;
use chia_sha2::Sha256;
use clvmr::{
    Allocator, ENABLE_KECCAK_OPS_OUTSIDE_GUARD, serde::node_from_bytes, serde::node_to_bytes,
};
use hex_literal::hex;
use indexmap::{IndexMap, IndexSet};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::{SimulatorError, validate_clvm_and_signature};

mod chain;
mod fast_forward;
mod mempool;
mod validation;

const BLOCK_REWARD_AMOUNT: u64 = 2_000_000_000_000;
const PREFARM_WALLET_INDEX: u32 = 1;

const SIMULATOR_GENESIS_CHALLENGE: Bytes32 = Bytes32::new(hex!(
    "eb8c4d20b322be8d9fddbf9412016bdffe9a2901d7edb0e364e94266d0e095f7"
));

#[derive(Debug, Clone)]
pub struct FullNodeSimulator {
    rng: ChaCha8Rng,
    height: u32,
    next_timestamp: u64,
    header_hashes: Vec<Bytes32>,
    blocks: IndexMap<Bytes32, SimBlock>,
    orphaned_blocks: IndexMap<Bytes32, SimBlock>,
    coins: IndexMap<Bytes32, SimCoinRecord>,
    coin_spends: IndexMap<Bytes32, CoinSpend>,
    coin_hints: IndexMap<Bytes32, Bytes32>,
    mempool: IndexMap<Bytes32, ValidatedBundle>,
    autofarm: bool,
    farming_puzzle_hash: Bytes32,
    master_secret_key: SecretKey,
    prefarm_puzzle_hash: Bytes32,
    node_id: Bytes32,
    events: Vec<FullNodeSimulatorEvent>,
}

#[derive(Debug, Clone)]
struct SimBlock {
    record: BlockRecord,
    additions: Vec<Bytes32>,
    removals: Vec<Bytes32>,
    spends: Vec<CoinSpend>,
    transactions: Vec<SpendBundle>,
    previous_coin_records: Vec<(Bytes32, SimCoinRecord)>,
    added_hints: Vec<Bytes32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SimCoinRecord {
    coin: Coin,
    coinbase: bool,
    confirmed_block_index: u32,
    spent_block_index: Option<u32>,
    timestamp: u64,
}

#[derive(Debug, Clone)]
struct ValidatedBundle {
    spend_bundle: SpendBundle,
    removals: Vec<Bytes32>,
    additions: Vec<(Coin, Option<Bytes32>)>,
    spends: IndexMap<Bytes32, ValidatedSpend>,
    cost: u64,
    fee: u64,
}

#[derive(Debug, Clone)]
struct ValidatedSpend {
    coin_spend: CoinSpend,
    flags: u32,
    fingerprint: Option<Bytes32>,
    additions: Vec<(Coin, Option<Bytes32>)>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FullNodeSimulatorEvent {
    Block {
        height: u32,
        header_hash: Bytes32,
        previous_header_hash: Bytes32,
        additions: Vec<CoinRecord>,
        removals: Vec<CoinRecord>,
    },
    Reorg {
        fork_height: u32,
        old_peak_hash: Bytes32,
        new_peak_hash: Bytes32,
        reverted_header_hashes: Vec<Bytes32>,
        new_header_hashes: Vec<Bytes32>,
    },
}

impl Default for FullNodeSimulator {
    fn default() -> Self {
        Self::with_seed(1337)
    }
}

impl FullNodeSimulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_seed(seed: u64) -> Self {
        Self::with_secret_key_and_rng(
            Self::secret_key_from_seed(seed),
            ChaCha8Rng::seed_from_u64(seed),
        )
    }

    pub fn with_secret_key(root_secret_key: SecretKey) -> Self {
        let mut seed = [0; 32];
        seed.copy_from_slice(&root_secret_key.to_bytes());
        Self::with_secret_key_and_rng(root_secret_key, ChaCha8Rng::from_seed(seed))
    }

    fn with_secret_key_and_rng(root_secret_key: SecretKey, mut rng: ChaCha8Rng) -> Self {
        let prefarm_secret_key =
            master_to_wallet_hardened(&root_secret_key, PREFARM_WALLET_INDEX).derive_synthetic();
        let prefarm_puzzle_hash =
            StandardArgs::curry_tree_hash(prefarm_secret_key.public_key()).into();
        let mut node_id = [0; 32];
        rng.fill(&mut node_id);

        let genesis_height = 1;
        let genesis_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let genesis_hash = Bytes32::default();
        let prefarm_coins = vec![
            Self::reward_coin(
                genesis_hash,
                genesis_height,
                0,
                prefarm_puzzle_hash,
                18375000000000000000,
            ),
            Self::reward_coin(
                genesis_hash,
                genesis_height,
                1,
                prefarm_puzzle_hash,
                2625000000000000000,
            ),
        ];
        let genesis_record = Self::make_block_record(
            genesis_hash,
            Bytes32::default(),
            genesis_height,
            genesis_timestamp,
            Bytes32::default(),
            0,
            0,
            prefarm_puzzle_hash,
            prefarm_coins.clone(),
        );
        let additions = prefarm_coins.iter().map(Coin::coin_id).collect::<Vec<_>>();
        let mut blocks = IndexMap::new();
        blocks.insert(
            genesis_hash,
            SimBlock {
                record: genesis_record,
                additions: additions.clone(),
                removals: Vec::new(),
                spends: Vec::new(),
                transactions: Vec::new(),
                previous_coin_records: Vec::new(),
                added_hints: Vec::new(),
            },
        );
        let mut coins = IndexMap::new();
        for coin in prefarm_coins {
            coins.insert(
                coin.coin_id(),
                SimCoinRecord {
                    coin,
                    coinbase: true,
                    confirmed_block_index: genesis_height,
                    spent_block_index: None,
                    timestamp: genesis_timestamp,
                },
            );
        }

        Self {
            rng,
            height: genesis_height,
            next_timestamp: genesis_timestamp.saturating_add(1),
            header_hashes: vec![genesis_hash],
            blocks,
            orphaned_blocks: IndexMap::new(),
            coins,
            coin_spends: IndexMap::new(),
            coin_hints: IndexMap::new(),
            mempool: IndexMap::new(),
            autofarm: true,
            farming_puzzle_hash: prefarm_puzzle_hash,
            master_secret_key: root_secret_key,
            prefarm_puzzle_hash,
            node_id: node_id.into(),
            events: Vec::new(),
        }
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn header_hash(&self) -> Bytes32 {
        self.header_hashes.last().copied().unwrap_or_default()
    }

    pub fn header_hash_of(&self, height: u32) -> Option<Bytes32> {
        self.header_hashes
            .get((height as usize).saturating_sub(1))
            .copied()
    }

    pub fn drain_events(&mut self) -> Vec<FullNodeSimulatorEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn insert_coin(&mut self, coin: Coin) {
        self.insert_coin_record(coin, false, self.height, self.next_timestamp);
    }

    pub fn new_coin(&mut self, puzzle_hash: Bytes32, amount: u64) -> Coin {
        let mut parent_coin_info = [0; 32];
        self.rng.fill(&mut parent_coin_info);
        let coin = Coin::new(parent_coin_info.into(), puzzle_hash, amount);
        self.insert_coin(coin);
        coin
    }

    pub fn get_farming_ph(&self) -> Bytes32 {
        self.farming_puzzle_hash
    }

    pub fn get_master_secret_key(&self) -> SecretKey {
        self.master_secret_key.clone()
    }

    pub fn get_prefarm_puzzle_hash(&self) -> Bytes32 {
        self.prefarm_puzzle_hash
    }

    pub fn set_farming_ph(&mut self, puzzle_hash: Bytes32) {
        self.farming_puzzle_hash = puzzle_hash;
    }

    pub fn get_autofarm(&self) -> bool {
        self.autofarm
    }

    pub fn set_autofarm(&mut self, autofarm: bool) {
        self.autofarm = autofarm;
    }

    pub fn get_blockchain_state(&self) -> BlockchainStateResponse {
        let peak = self
            .blocks
            .get(&self.header_hash())
            .map(|block| block.record.clone())
            .unwrap_or_else(|| {
                Self::make_block_record(
                    Bytes32::default(),
                    Bytes32::default(),
                    0,
                    0,
                    Bytes32::default(),
                    0,
                    0,
                    self.farming_puzzle_hash,
                    Vec::new(),
                )
            });

        BlockchainStateResponse {
            blockchain_state: Some(BlockchainState {
                average_block_time: 1,
                block_max_cost: 11_000_000_000,
                difficulty: 1,
                genesis_challenge_initialized: true,
                mempool_cost: self.mempool.values().map(|item| item.cost).sum(),
                mempool_fees: self.mempool.values().map(|item| item.fee).sum(),
                mempool_max_total_cost: 110_000_000_000,
                mempool_min_fees: MempoolMinFees { cost_5000000: 0 },
                mempool_size: self.mempool.len() as u32,
                node_id: self.node_id,
                peak,
                space: 0,
                sub_slot_iters: 1,
                sync: SyncState {
                    sync_mode: false,
                    sync_progress_height: self.height,
                    sync_tip_height: self.height,
                    synced: true,
                },
            }),
            error: None,
            success: true,
        }
    }

    pub fn get_network_info(&self) -> GetNetworkInfoResponse {
        GetNetworkInfoResponse {
            network_name: Some("simulator0".to_string()),
            network_prefix: Some("txch".to_string()),
            genesis_challenge: Some(SIMULATOR_GENESIS_CHALLENGE),
            error: None,
            success: true,
        }
    }

    pub fn get_aggsig_additional_data(&self) -> Bytes32 {
        SIMULATOR_GENESIS_CHALLENGE
    }

    pub fn get_block_record(&self, header_hash: Bytes32) -> GetBlockRecordResponse {
        GetBlockRecordResponse {
            block_record: self
                .blocks
                .get(&header_hash)
                .or_else(|| self.orphaned_blocks.get(&header_hash))
                .map(|block| block.record.clone()),
            error: None,
            success: true,
        }
    }

    pub fn get_block_record_by_height(&self, height: u32) -> GetBlockRecordResponse {
        let block_record = self
            .header_hash_of(height)
            .and_then(|header_hash| self.blocks.get(&header_hash))
            .map(|block| block.record.clone());

        GetBlockRecordResponse {
            block_record,
            error: None,
            success: true,
        }
    }

    pub fn get_block_records(&self, start: u32, end: u32) -> GetBlockRecordsResponse {
        let block_records = (start..end)
            .filter_map(|height| self.get_block_record_by_height(height).block_record)
            .collect();

        GetBlockRecordsResponse {
            block_records: Some(block_records),
            error: None,
            success: true,
        }
    }

    pub fn get_additions_and_removals(&self, header_hash: Bytes32) -> AdditionsAndRemovalsResponse {
        let Some(block) = self
            .blocks
            .get(&header_hash)
            .or_else(|| self.orphaned_blocks.get(&header_hash))
        else {
            return AdditionsAndRemovalsResponse {
                additions: None,
                removals: None,
                error: Some("block not found".to_string()),
                success: false,
            };
        };

        AdditionsAndRemovalsResponse {
            additions: Some(self.records_for_ids(&block.additions)),
            removals: Some(self.records_for_ids(&block.removals)),
            error: None,
            success: true,
        }
    }

    pub fn get_block_spends(&self, header_hash: Bytes32) -> GetBlockSpendsResponse {
        GetBlockSpendsResponse {
            block_spends: self
                .blocks
                .get(&header_hash)
                .or_else(|| self.orphaned_blocks.get(&header_hash))
                .map(|block| block.spends.clone()),
            error: None,
            success: true,
        }
    }

    pub fn get_coin_record_by_name(&self, name: Bytes32) -> GetCoinRecordResponse {
        GetCoinRecordResponse {
            coin_record: self.coins.get(&name).map(|record| record.to_coin_record()),
            error: None,
            success: true,
        }
    }

    pub fn get_coin_records_by_names(
        &self,
        names: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        self.records_response(
            self.coins
                .iter()
                .filter(|(coin_id, _)| names.contains(coin_id))
                .map(|(_, record)| *record),
            start_height,
            end_height,
            include_spent_coins,
        )
    }

    pub fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        self.get_coin_records_by_hints(vec![hint], start_height, end_height, include_spent_coins)
    }

    pub fn get_coin_records_by_hints(
        &self,
        hints: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        let hints: HashSet<Bytes32> = hints.into_iter().collect();
        self.records_response(
            self.coins
                .iter()
                .filter(|(coin_id, _)| {
                    self.coin_hints
                        .get(*coin_id)
                        .is_some_and(|hint| hints.contains(hint))
                })
                .map(|(_, record)| *record),
            start_height,
            end_height,
            include_spent_coins,
        )
    }

    pub fn get_coin_records_by_parent_ids(
        &self,
        parent_ids: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        let parent_ids: HashSet<Bytes32> = parent_ids.into_iter().collect();
        self.records_response(
            self.coins
                .values()
                .filter(|record| parent_ids.contains(&record.coin.parent_coin_info))
                .copied(),
            start_height,
            end_height,
            include_spent_coins,
        )
    }

    pub fn get_coin_records_by_puzzle_hash(
        &self,
        puzzle_hash: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        self.get_coin_records_by_puzzle_hashes(
            vec![puzzle_hash],
            start_height,
            end_height,
            include_spent_coins,
        )
    }

    pub fn get_coin_records_by_puzzle_hashes(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        let puzzle_hashes: HashSet<Bytes32> = puzzle_hashes.into_iter().collect();
        let matching_count = self
            .coins
            .values()
            .filter(|record| puzzle_hashes.contains(&record.coin.puzzle_hash))
            .count();
        eprintln!(
            "[DEBUG-SIM-RESTORE] simulator.get_coin_records_by_puzzle_hashes puzzle_hashes={:?} include_spent_coins={:?} total_coins={} matching_coins={}",
            puzzle_hashes,
            include_spent_coins,
            self.coins.len(),
            matching_count,
        );
        self.records_response(
            self.coins
                .values()
                .filter(|record| puzzle_hashes.contains(&record.coin.puzzle_hash))
                .copied(),
            start_height,
            end_height,
            include_spent_coins,
        )
    }

    pub fn get_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: Option<u32>,
    ) -> GetPuzzleAndSolutionResponse {
        let coin_solution = self.coin_spends.get(&coin_id).and_then(|spend| {
            let record = self.coins.get(&coin_id)?;
            if height.is_none() || record.spent_block_index == height {
                Some(spend.clone())
            } else {
                None
            }
        });

        GetPuzzleAndSolutionResponse {
            coin_solution,
            error: None,
            success: true,
        }
    }

    pub fn push_tx(&mut self, mut spend_bundle: SpendBundle) -> PushTxResponse {
        let max_fast_forward_attempts: usize = 64;
        let mut fast_forward_attempts: usize = 0;

        loop {
            let tx_id = spend_bundle.name();
            if self.mempool.contains_key(&tx_id) {
                if self.autofarm {
                    self.farm_block(1);
                }
                return PushTxResponse {
                    status: "SUCCESS".to_string(),
                    error: None,
                    success: true,
                };
            }

            let validated = match self.validate_bundle(spend_bundle.clone()) {
                Ok(validated) => validated,
                Err(SimulatorError::Validation(ErrorCode::DoubleSpend))
                    if fast_forward_attempts < max_fast_forward_attempts =>
                {
                    let Some(rewritten) = self.try_fast_forward_settled_bundle(&spend_bundle)
                    else {
                        return PushTxResponse {
                            status: "FAILED".to_string(),
                            error: Some(
                                SimulatorError::Validation(ErrorCode::DoubleSpend).to_string(),
                            ),
                            success: false,
                        };
                    };
                    fast_forward_attempts = fast_forward_attempts.saturating_add(1);
                    spend_bundle = rewritten;
                    continue;
                }
                Err(error) => {
                    return PushTxResponse {
                        status: "FAILED".to_string(),
                        error: Some(error.to_string()),
                        success: false,
                    };
                }
            };

            match self.insert_mempool_item(tx_id, validated.clone()) {
                Ok(()) => {
                    if self.autofarm {
                        self.farm_block(1);
                    }

                    return PushTxResponse {
                        status: "SUCCESS".to_string(),
                        error: None,
                        success: true,
                    };
                }
                Err(SimulatorError::Validation(ErrorCode::MempoolConflict))
                    if fast_forward_attempts < max_fast_forward_attempts =>
                {
                    let Some(rewritten) = self.try_fast_forward_bundle(&validated) else {
                        return PushTxResponse {
                            status: "FAILED".to_string(),
                            error: Some(
                                SimulatorError::Validation(ErrorCode::MempoolConflict).to_string(),
                            ),
                            success: false,
                        };
                    };
                    fast_forward_attempts = fast_forward_attempts.saturating_add(1);
                    spend_bundle = rewritten;
                }
                Err(error) => {
                    return PushTxResponse {
                        status: "FAILED".to_string(),
                        error: Some(error.to_string()),
                        success: false,
                    };
                }
            }
        }
    }

    pub fn get_mempool_item_by_tx_id(&self, tx_id: Bytes32) -> GetMempoolItemResponse {
        GetMempoolItemResponse {
            mempool_item: self
                .mempool
                .get(&tx_id)
                .map(ValidatedBundle::to_mempool_item),
            error: None,
            success: true,
        }
    }

    pub fn get_mempool_items_by_coin_name(&self, coin_name: Bytes32) -> GetMempoolItemsResponse {
        GetMempoolItemsResponse {
            mempool_items: Some(
                self.mempool
                    .values()
                    .filter(|item| item.removals.contains(&coin_name))
                    .map(ValidatedBundle::to_mempool_item)
                    .collect(),
            ),
            error: None,
            success: true,
        }
    }

    pub fn farm_block(&mut self, blocks: u32) -> Vec<BlockRecord> {
        let count = blocks.max(1);
        let mut records = Vec::new();
        for _ in 0..count {
            records.push(self.create_block_from_mempool());
        }
        records
    }

    pub fn revert_blocks(&mut self, blocks: u32) -> Vec<Bytes32> {
        let reverted = self.revert_canonical_blocks(blocks);
        self.requeue_transactions(reverted.iter().flat_map(|block| block.transactions.clone()));
        reverted
            .iter()
            .map(|block| block.record.header_hash)
            .collect()
    }

    pub fn reorg_blocks(
        &mut self,
        num_of_blocks_to_rev: u32,
        num_of_new_blocks: u32,
    ) -> Vec<BlockRecord> {
        let old_peak_hash = self.header_hash();
        let reverted = self.revert_canonical_blocks(num_of_blocks_to_rev);
        let fork_height = self.height;
        let reverted_header_hashes = reverted
            .iter()
            .map(|block| block.record.header_hash)
            .collect::<Vec<_>>();

        for block in reverted {
            self.orphaned_blocks.insert(block.record.header_hash, block);
        }

        let mut records = Vec::new();
        let mut new_header_hashes = Vec::new();
        for _ in 0..num_of_new_blocks {
            let record = self.create_block_from_mempool();
            new_header_hashes.push(record.header_hash);
            records.push(record);
        }

        self.prune_mempool();
        self.events.push(FullNodeSimulatorEvent::Reorg {
            fork_height,
            old_peak_hash,
            new_peak_hash: self.header_hash(),
            reverted_header_hashes,
            new_header_hashes,
        });

        records
    }

    fn insert_coin_record(&mut self, coin: Coin, coinbase: bool, height: u32, timestamp: u64) {
        self.coins.insert(
            coin.coin_id(),
            SimCoinRecord {
                coin,
                coinbase,
                confirmed_block_index: height,
                spent_block_index: None,
                timestamp,
            },
        );
    }

    fn records_for_ids(&self, coin_ids: &[Bytes32]) -> Vec<CoinRecord> {
        coin_ids
            .iter()
            .filter_map(|coin_id| self.coins.get(coin_id))
            .map(|record| record.to_coin_record())
            .collect()
    }

    fn records_response(
        &self,
        records: impl IntoIterator<Item = SimCoinRecord>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        let include_spent = include_spent_coins.unwrap_or(false);
        let records = records
            .into_iter()
            .filter(|record| include_spent || record.spent_block_index.is_none())
            .filter(|record| {
                start_height.is_none_or(|start| record.confirmed_block_index >= start)
                    && end_height.is_none_or(|end| record.confirmed_block_index < end)
            })
            .map(|record| record.to_coin_record())
            .collect();

        GetCoinRecordsResponse {
            coin_records: Some(records),
            error: None,
            success: true,
            next_cursor: None,
            truncated: None,
        }
    }

    fn random_hash(&mut self) -> Bytes32 {
        let mut bytes = [0; 32];
        self.rng.fill(&mut bytes);
        bytes.into()
    }

    fn secret_key_from_seed(seed: u64) -> SecretKey {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let entropy: [u8; 32] = rng.random();
        let mnemonic = Mnemonic::from_entropy(&entropy).expect("32 bytes is valid BIP39 entropy");
        SecretKey::from_seed(&mnemonic.to_seed(""))
    }

    fn reward_coin(
        header_hash: Bytes32,
        height: u32,
        index: u8,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> Coin {
        Coin::new(
            Self::reward_parent_id(header_hash, height, index),
            puzzle_hash,
            amount,
        )
    }

    fn reward_parent_id(header_hash: Bytes32, height: u32, index: u8) -> Bytes32 {
        let mut hasher = Sha256::new();
        hasher.update(b"chia-sdk-full-node-simulator-reward");
        hasher.update(header_hash.to_bytes());
        hasher.update(height.to_be_bytes());
        hasher.update([index]);
        hasher.finalize().into()
    }

    #[allow(clippy::too_many_arguments)]
    fn make_block_record(
        header_hash: Bytes32,
        prev_hash: Bytes32,
        height: u32,
        timestamp: u64,
        prev_transaction_block_hash: Bytes32,
        fees: u64,
        prev_transaction_block_height: u32,
        farming_puzzle_hash: Bytes32,
        reward_claims_incorporated: Vec<Coin>,
    ) -> BlockRecord {
        BlockRecord::new(
            header_hash,
            prev_hash,
            height,
            height as u128,
            height as u128,
            0,
            ClassgroupElement::default(),
            None,
            header_hash,
            header_hash,
            1,
            farming_puzzle_hash,
            farming_puzzle_hash,
            0,
            15,
            false,
            prev_transaction_block_height,
            Some(timestamp),
            Some(prev_transaction_block_hash),
            Some(fees),
            Some(reward_claims_incorporated),
            None,
            None,
            None,
            None,
        )
    }
}

impl SimCoinRecord {
    fn to_coin_record(self) -> CoinRecord {
        CoinRecord {
            coin: self.coin,
            coinbase: self.coinbase,
            confirmed_block_index: self.confirmed_block_index,
            spent: self.spent_block_index.is_some(),
            spent_block_index: self.spent_block_index.unwrap_or(0),
            timestamp: self.timestamp,
        }
    }
}

impl ValidatedBundle {
    fn to_mempool_item(&self) -> MempoolItem {
        MempoolItem {
            spend_bundle: self.spend_bundle.clone(),
            fee: self.fee,
        }
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::{SecretKey, Signature};
    use chia_protocol::{Coin, CoinSpend, SpendBundle};
    use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};
    use chia_sdk_types::conditions::{CreateCoin, Memos};
    use clvmr::NodePtr;

    use crate::{FullNodeSimulatorEvent, to_program, to_puzzle};

    use super::*;

    fn spend_to_child(
        coin: Coin,
        puzzle_reveal: chia_protocol::Program,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> anyhow::Result<SpendBundle> {
        Ok(SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(puzzle_hash, amount, Memos::None)])?,
            )],
            Signature::default(),
        ))
    }

    #[test]
    fn genesis_contains_prefarm_rewards() {
        let sim = FullNodeSimulator::new();
        let prefarm_puzzle_hash = sim.get_prefarm_puzzle_hash();
        assert_eq!(sim.height(), 1);
        assert_eq!(sim.get_farming_ph(), prefarm_puzzle_hash);

        let prefarm_records = sim
            .get_coin_records_by_puzzle_hash(prefarm_puzzle_hash, None, None, None)
            .coin_records
            .unwrap();
        assert_eq!(prefarm_records.len(), 2);
        assert!(prefarm_records.iter().all(|record| record.coinbase));
        assert!(prefarm_records.iter().all(|record| !record.spent));
        assert_eq!(
            prefarm_records
                .iter()
                .map(|record| u128::from(record.coin.amount))
                .sum::<u128>(),
            21_000_000_000_000_000_000_u128
        );

        let genesis = sim.get_block_record_by_height(1).block_record.unwrap();
        let reward_claims = genesis.reward_claims_incorporated.unwrap();
        assert_eq!(reward_claims.len(), 2);
        assert_eq!(
            reward_claims
                .iter()
                .map(|coin| u128::from(coin.amount))
                .sum::<u128>(),
            21_000_000_000_000_000_000_u128
        );
        assert!(
            reward_claims
                .iter()
                .all(|coin| coin.puzzle_hash == prefarm_puzzle_hash)
        );
    }

    #[test]
    fn explicit_secret_key_derives_prefarm_wallet_index_one() {
        let root_secret_key = SecretKey::from_seed(&[42; 32]);
        let sim = FullNodeSimulator::with_secret_key(root_secret_key.clone());
        let expected_secret_key = master_to_wallet_hardened(&root_secret_key, 1).derive_synthetic();
        let expected_puzzle_hash =
            StandardArgs::curry_tree_hash(expected_secret_key.public_key()).into();

        assert_eq!(sim.get_prefarm_puzzle_hash(), expected_puzzle_hash);
    }

    #[test]
    fn push_tx_autofarms_by_default() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let coin = sim.new_coin(puzzle_hash, 100);
        let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;

        assert!(sim.get_autofarm());
        let response = sim.push_tx(spend_bundle);
        assert!(response.success);
        assert_eq!(
            sim.get_blockchain_state()
                .blockchain_state
                .unwrap()
                .mempool_size,
            0
        );

        let record = sim
            .get_coin_record_by_name(coin.coin_id())
            .coin_record
            .unwrap();
        assert!(record.spent);
        assert_eq!(record.spent_block_index, 2);
        assert_eq!(sim.height(), 2);
        assert_eq!(
            sim.get_block_spends(sim.header_hash())
                .block_spends
                .unwrap()
                .len(),
            1
        );

        Ok(())
    }

    #[test]
    fn farm_block_includes_mempool_and_emits_event() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        sim.set_autofarm(false);
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let coin = sim.new_coin(puzzle_hash, 100);
        let child = Coin::new(coin.coin_id(), puzzle_hash, 99);
        let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;
        assert!(sim.push_tx(spend_bundle).success);

        let records = sim.farm_block(1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].height, 2);
        let reward_claims = records[0].reward_claims_incorporated.clone().unwrap();
        assert_eq!(reward_claims.len(), 1);
        assert_eq!(reward_claims[0].amount, BLOCK_REWARD_AMOUNT);
        assert_eq!(reward_claims[0].puzzle_hash, sim.get_prefarm_puzzle_hash());
        assert_eq!(
            sim.get_blockchain_state()
                .blockchain_state
                .unwrap()
                .mempool_size,
            0
        );

        let spent = sim
            .get_coin_record_by_name(coin.coin_id())
            .coin_record
            .unwrap();
        assert!(spent.spent);
        assert_eq!(spent.spent_block_index, 2);

        let created = sim
            .get_coin_record_by_name(child.coin_id())
            .coin_record
            .unwrap();
        assert!(!created.spent);
        assert_eq!(created.confirmed_block_index, 2);

        let spends = sim
            .get_block_spends(records[0].header_hash)
            .block_spends
            .unwrap();
        assert_eq!(spends.len(), 1);

        let events = sim.drain_events();
        assert!(matches!(
            events.as_slice(),
            [FullNodeSimulatorEvent::Block {
                height: 2,
                additions,
                ..
            }] if additions.iter().any(|record| record.coin.coin_id() == reward_claims[0].coin_id())
        ));

        Ok(())
    }

    #[test]
    fn set_farming_ph_changes_future_reward_destination() {
        let mut sim = FullNodeSimulator::new();
        let (new_farming_ph, _) = to_puzzle(99).unwrap();
        sim.set_farming_ph(new_farming_ph);

        let record = sim.farm_block(1).pop().unwrap();
        let reward_claims = record.reward_claims_incorporated.unwrap();
        assert_eq!(reward_claims.len(), 1);
        assert_eq!(reward_claims[0].amount, BLOCK_REWARD_AMOUNT);
        assert_eq!(reward_claims[0].puzzle_hash, new_farming_ph);
    }

    #[test]
    fn push_tx_accepts_ephemeral_spends_in_same_bundle() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        sim.set_autofarm(false);
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let parent = sim.new_coin(puzzle_hash, 100);
        let child = Coin::new(parent.coin_id(), puzzle_hash, 99);
        let grandchild = Coin::new(child.coin_id(), puzzle_hash, 98);

        let parent_spend = spend_to_child(parent, puzzle_reveal.clone(), puzzle_hash, 99)?;
        let child_spend = CoinSpend::new(
            child,
            puzzle_reveal,
            to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 98, Memos::None)])?,
        );
        let spend_bundle = SpendBundle::new(
            vec![parent_spend.coin_spends[0].clone(), child_spend],
            Signature::default(),
        );
        assert!(sim.push_tx(spend_bundle).success);

        sim.farm_block(1);

        let parent_record = sim
            .get_coin_record_by_name(parent.coin_id())
            .coin_record
            .unwrap();
        assert!(parent_record.spent);

        let child_record = sim
            .get_coin_record_by_name(child.coin_id())
            .coin_record
            .unwrap();
        assert!(child_record.spent);
        assert_eq!(child_record.confirmed_block_index, 2);
        assert_eq!(child_record.spent_block_index, 2);

        let grandchild_record = sim
            .get_coin_record_by_name(grandchild.coin_id())
            .coin_record
            .unwrap();
        assert!(!grandchild_record.spent);
        assert_eq!(grandchild_record.confirmed_block_index, 2);

        Ok(())
    }

    #[test]
    fn autofarm_can_be_turned_off_and_on() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let coin = sim.new_coin(puzzle_hash, 100);
        let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;

        sim.set_autofarm(false);
        assert!(!sim.get_autofarm());
        assert!(sim.push_tx(spend_bundle.clone()).success);
        assert_eq!(
            sim.get_blockchain_state()
                .blockchain_state
                .unwrap()
                .mempool_size,
            1
        );
        assert_eq!(sim.height(), 1);

        sim.set_autofarm(true);
        assert!(sim.get_autofarm());
        assert!(sim.push_tx(spend_bundle).success);
        assert_eq!(sim.height(), 2);
        assert_eq!(
            sim.get_blockchain_state()
                .blockchain_state
                .unwrap()
                .mempool_size,
            0
        );

        Ok(())
    }

    #[test]
    fn revert_removes_farmed_reward() {
        let mut sim = FullNodeSimulator::new();
        let reward = sim
            .farm_block(1)
            .pop()
            .unwrap()
            .reward_claims_incorporated
            .unwrap()
            .pop()
            .unwrap();
        assert!(
            sim.get_coin_record_by_name(reward.coin_id())
                .coin_record
                .is_some()
        );

        sim.revert_blocks(1);
        assert!(
            sim.get_coin_record_by_name(reward.coin_id())
                .coin_record
                .is_none()
        );
    }

    #[test]
    fn reorg_replaces_peak_and_emits_reorg() {
        let mut sim = FullNodeSimulator::new();
        let old_blocks = sim.farm_block(2);
        let old_peak = old_blocks.last().unwrap().header_hash;
        let old_reward = old_blocks
            .last()
            .unwrap()
            .reward_claims_incorporated
            .clone()
            .unwrap()
            .pop()
            .unwrap();

        let new_blocks = sim.reorg_blocks(1, 2);
        assert_eq!(new_blocks.len(), 2);
        assert_ne!(sim.header_hash(), old_peak);
        assert_eq!(sim.height(), 4);
        assert!(
            sim.get_coin_record_by_name(old_reward.coin_id())
                .coin_record
                .is_none()
        );
        let orphan = sim.get_block_record(old_peak).block_record.unwrap();
        assert_eq!(
            orphan.reward_claims_incorporated.unwrap()[0].coin_id(),
            old_reward.coin_id()
        );
        assert!(
            new_blocks
                .iter()
                .all(|block| block.reward_claims_incorporated.as_ref().unwrap().len() == 1)
        );

        let events = sim.drain_events();
        assert!(events.iter().any(|event| {
            matches!(
                event,
                FullNodeSimulatorEvent::Reorg {
                    old_peak_hash,
                    new_peak_hash,
                    ..
                } if *old_peak_hash == old_peak && *new_peak_hash == sim.header_hash()
            )
        }));
    }
}
