use std::collections::HashSet;

use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{BlockRecord, Bytes32, ClassgroupElement, Coin, CoinSpend, SpendBundle};
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainState, BlockchainStateResponse, CoinRecord,
    GetBlockRecordResponse, GetBlockRecordsResponse, GetBlockSpendsResponse,
    GetCoinRecordResponse, GetCoinRecordsResponse, GetMempoolItemResponse,
    GetMempoolItemsResponse, GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, MempoolItem,
    MempoolMinFees, PushTxResponse, SyncState,
};
use chia_sdk_types::TESTNET11_CONSTANTS;
use clvmr::ENABLE_KECCAK_OPS_OUTSIDE_GUARD;
use indexmap::{IndexMap, IndexSet};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::{SimulatorError, validate_clvm_and_signature};

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
    farming_puzzle_hash: Bytes32,
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
    coin_spends: Vec<CoinSpend>,
    cost: u64,
    fee: u64,
}

#[derive(Debug, Clone)]
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
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut node_id = [0; 32];
        rng.fill(&mut node_id);

        let genesis_hash = Bytes32::default();
        let genesis_record = Self::make_block_record(
            genesis_hash,
            Bytes32::default(),
            0,
            0,
            Bytes32::default(),
            0,
            0,
            Bytes32::default(),
        );
        let mut blocks = IndexMap::new();
        blocks.insert(
            genesis_hash,
            SimBlock {
                record: genesis_record,
                additions: Vec::new(),
                removals: Vec::new(),
                spends: Vec::new(),
                transactions: Vec::new(),
                previous_coin_records: Vec::new(),
                added_hints: Vec::new(),
            },
        );

        Self {
            rng,
            height: 0,
            next_timestamp: 1,
            header_hashes: vec![genesis_hash],
            blocks,
            orphaned_blocks: IndexMap::new(),
            coins: IndexMap::new(),
            coin_spends: IndexMap::new(),
            coin_hints: IndexMap::new(),
            mempool: IndexMap::new(),
            farming_puzzle_hash: Bytes32::default(),
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
        self.header_hashes.get(height as usize).copied()
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

    pub fn set_farming_ph(&mut self, puzzle_hash: Bytes32) {
        self.farming_puzzle_hash = puzzle_hash;
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
            network_name: Some("testnet11".to_string()),
            network_prefix: Some("txch".to_string()),
            genesis_challenge: Some(TESTNET11_CONSTANTS.genesis_challenge),
            error: None,
            success: true,
        }
    }

    pub fn get_aggsig_additional_data(&self) -> Bytes32 {
        TESTNET11_CONSTANTS.agg_sig_me_additional_data
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

    pub fn push_tx(&mut self, spend_bundle: SpendBundle) -> PushTxResponse {
        let tx_id = spend_bundle.name();
        if self.mempool.contains_key(&tx_id) {
            return PushTxResponse {
                status: "SUCCESS".to_string(),
                error: None,
                success: true,
            };
        }

        let validated = match self.validate_bundle(spend_bundle, true) {
            Ok(validated) => validated,
            Err(error) => {
                return PushTxResponse {
                    status: "FAILED".to_string(),
                    error: Some(error.to_string()),
                    success: false,
                };
            }
        };

        self.mempool.insert(tx_id, validated);

        PushTxResponse {
            status: "SUCCESS".to_string(),
            error: None,
            success: true,
        }
    }

    pub fn get_mempool_item_by_tx_id(&self, tx_id: Bytes32) -> GetMempoolItemResponse {
        GetMempoolItemResponse {
            mempool_item: self.mempool.get(&tx_id).map(ValidatedBundle::to_mempool_item),
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
        reverted.iter().map(|block| block.record.header_hash).collect()
    }

    pub fn reorg_blocks(&mut self, num_of_blocks_to_rev: u32, num_of_new_blocks: u32) -> Vec<BlockRecord> {
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

    fn validate_bundle(
        &self,
        spend_bundle: SpendBundle,
        check_mempool: bool,
    ) -> Result<ValidatedBundle, SimulatorError> {
        if spend_bundle.coin_spends.is_empty() {
            return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
        }

        let conds = validate_clvm_and_signature(
            &spend_bundle,
            11_000_000_000 / 2,
            &TESTNET11_CONSTANTS,
            ENABLE_KECCAK_OPS_OUTSIDE_GUARD,
        )
        .map_err(SimulatorError::Validation)?;

        if self.height < conds.height_absolute {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertHeightAbsoluteFailed,
            ));
        }
        if self.next_timestamp < conds.seconds_absolute {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertSecondsAbsoluteFailed,
            ));
        }
        if let Some(height) = conds.before_height_absolute
            && height < self.height
        {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertBeforeHeightAbsoluteFailed,
            ));
        }
        if let Some(seconds) = conds.before_seconds_absolute
            && seconds < self.next_timestamp
        {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertBeforeSecondsAbsoluteFailed,
            ));
        }

        let bundle_puzzle_hashes = spend_bundle
            .coin_spends
            .iter()
            .map(|spend| spend.coin.puzzle_hash)
            .collect::<HashSet<_>>();
        let condition_puzzle_hashes = conds
            .spends
            .iter()
            .map(|spend| spend.puzzle_hash)
            .collect::<HashSet<_>>();
        if bundle_puzzle_hashes != condition_puzzle_hashes {
            return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
        }

        let mut removals = IndexSet::new();
        let mut additions = IndexMap::new();

        for spend in &conds.spends {
            let coin = Coin::new(spend.parent_id, spend.puzzle_hash, spend.coin_amount);
            let coin_id = coin.coin_id();
            if !removals.insert(coin_id) {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }

            let record = self
                .coins
                .get(&coin_id)
                .ok_or(SimulatorError::Validation(ErrorCode::UnknownUnspent))?;

            if record.spent_block_index.is_some() {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }

            self.validate_relative_conditions(spend, record)?;

            if check_mempool
                && self
                    .mempool
                    .values()
                    .any(|item| item.removals.contains(&coin_id))
            {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }

            for (puzzle_hash, amount, hint) in &spend.create_coin {
                let coin = Coin::new(coin_id, *puzzle_hash, *amount);
                let parsed_hint = hint
                    .as_ref()
                    .filter(|bytes| bytes.len() == 32)
                    .and_then(|bytes| Bytes32::try_from(bytes.as_ref()).ok());
                additions.insert(coin.coin_id(), (coin, parsed_hint));
            }
        }

        let fee = conds
            .removal_amount
            .checked_sub(conds.addition_amount)
            .unwrap_or_default()
            .try_into()
            .unwrap_or(u64::MAX);
        if fee < conds.reserve_fee {
            return Err(SimulatorError::Validation(ErrorCode::ReserveFeeConditionFailed));
        }

        Ok(ValidatedBundle {
            coin_spends: spend_bundle.coin_spends.clone(),
            spend_bundle,
            removals: removals.into_iter().collect(),
            additions: additions.into_values().collect(),
            cost: conds.cost,
            fee,
        })
    }

    fn validate_relative_conditions(
        &self,
        spend: &chia_consensus::owned_conditions::OwnedSpendConditions,
        record: &SimCoinRecord,
    ) -> Result<(), SimulatorError> {
        if let Some(relative_height) = spend.height_relative
            && self.height < record.confirmed_block_index + relative_height
        {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertHeightRelativeFailed,
            ));
        }
        if let Some(relative_seconds) = spend.seconds_relative
            && self.next_timestamp < record.timestamp + relative_seconds
        {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertSecondsRelativeFailed,
            ));
        }
        if let Some(relative_height) = spend.before_height_relative
            && record.confirmed_block_index + relative_height < self.height
        {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertBeforeHeightRelativeFailed,
            ));
        }
        if let Some(relative_seconds) = spend.before_seconds_relative
            && record.timestamp + relative_seconds < self.next_timestamp
        {
            return Err(SimulatorError::Validation(
                ErrorCode::AssertBeforeSecondsRelativeFailed,
            ));
        }
        Ok(())
    }

    fn create_block_from_mempool(&mut self) -> BlockRecord {
        let previous_header_hash = self.header_hash();
        let height = self.height + 1;
        let timestamp = self.next_timestamp;
        let header_hash = self.random_hash();

        let mut included_tx_ids = Vec::new();
        let mut included = Vec::new();
        let mut spent_in_block = IndexSet::new();
        for (tx_id, item) in self.mempool.clone() {
            let Ok(validated) = self.validate_bundle(item.spend_bundle.clone(), false) else {
                continue;
            };
            if validated
                .removals
                .iter()
                .any(|coin_id| spent_in_block.contains(coin_id))
            {
                continue;
            }
            spent_in_block.extend(validated.removals.iter().copied());
            included_tx_ids.push(tx_id);
            included.push(validated);
        }

        for tx_id in included_tx_ids {
            self.mempool.swap_remove(&tx_id);
        }

        let mut additions = Vec::new();
        let mut removals = Vec::new();
        let mut spends = Vec::new();
        let mut transactions = Vec::new();
        let mut previous_coin_records = Vec::new();
        let mut added_hints = Vec::new();
        let mut fees = 0_u64;

        for item in included {
            fees = fees.saturating_add(item.fee);
            transactions.push(item.spend_bundle);
            spends.extend(item.coin_spends);

            for coin_id in item.removals {
                if let Some(record) = self.coins.get_mut(&coin_id) {
                    previous_coin_records.push((coin_id, *record));
                    record.spent_block_index = Some(height);
                    removals.push(coin_id);
                }
            }

            for (coin, hint) in item.additions {
                let coin_id = coin.coin_id();
                self.insert_coin_record(coin, false, height, timestamp);
                if let Some(hint) = hint {
                    self.coin_hints.insert(coin_id, hint);
                    added_hints.push(coin_id);
                }
                additions.push(coin_id);
            }
        }

        for spend in &spends {
            self.coin_spends.insert(spend.coin.coin_id(), spend.clone());
        }

        let record = Self::make_block_record(
            header_hash,
            previous_header_hash,
            height,
            timestamp,
            self.header_hash_of(height.saturating_sub(1)).unwrap_or_default(),
            fees,
            height.saturating_sub(1),
            self.farming_puzzle_hash,
        );
        let block = SimBlock {
            record: record.clone(),
            additions: additions.clone(),
            removals: removals.clone(),
            spends,
            transactions,
            previous_coin_records,
            added_hints,
        };

        self.blocks.insert(header_hash, block);
        self.header_hashes.push(header_hash);
        self.height = height;
        self.next_timestamp = self.next_timestamp.saturating_add(1);

        self.events.push(FullNodeSimulatorEvent::Block {
            height,
            header_hash,
            previous_header_hash,
            additions: self.records_for_ids(&additions),
            removals: self.records_for_ids(&removals),
        });

        record
    }

    fn revert_canonical_blocks(&mut self, blocks: u32) -> Vec<SimBlock> {
        let mut reverted = Vec::new();
        for _ in 0..blocks {
            if self.height == 0 {
                break;
            }
            let Some(header_hash) = self.header_hashes.pop() else {
                break;
            };
            let Some(block) = self.blocks.swap_remove(&header_hash) else {
                break;
            };

            for coin_id in &block.additions {
                self.coins.swap_remove(coin_id);
                self.coin_hints.swap_remove(coin_id);
            }
            for coin_id in &block.added_hints {
                self.coin_hints.swap_remove(coin_id);
            }
            for (coin_id, previous_record) in &block.previous_coin_records {
                self.coins.insert(*coin_id, *previous_record);
                self.coin_spends.swap_remove(coin_id);
            }

            self.height = self.height.saturating_sub(1);
            self.next_timestamp = block.record.timestamp.unwrap_or(self.next_timestamp);
            reverted.push(block);
        }
        reverted
    }

    fn requeue_transactions(&mut self, transactions: impl IntoIterator<Item = SpendBundle>) {
        for spend_bundle in transactions {
            let tx_id = spend_bundle.name();
            if self.mempool.contains_key(&tx_id) {
                continue;
            }
            if let Ok(validated) = self.validate_bundle(spend_bundle, true) {
                self.mempool.insert(tx_id, validated);
            }
        }
    }

    fn prune_mempool(&mut self) {
        let spend_bundles = self
            .mempool
            .values()
            .map(|item| item.spend_bundle.clone())
            .collect::<Vec<_>>();
        self.mempool.clear();
        self.requeue_transactions(spend_bundles);
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
                    && end_height.is_none_or(|end| record.confirmed_block_index <= end)
            })
            .map(|record| record.to_coin_record())
            .collect();

        GetCoinRecordsResponse {
            coin_records: Some(records),
            error: None,
            success: true,
        }
    }

    fn random_hash(&mut self) -> Bytes32 {
        let mut bytes = [0; 32];
        self.rng.fill(&mut bytes);
        bytes.into()
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
            Some(Vec::new()),
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
    use chia_bls::Signature;
    use chia_protocol::{Coin, CoinSpend, SpendBundle};
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
                to_program([CreateCoin::<NodePtr>::new(
                    puzzle_hash,
                    amount,
                    Memos::None,
                )])?,
            )],
            Signature::default(),
        ))
    }

    #[test]
    fn push_tx_does_not_mutate_until_farmed() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let coin = sim.new_coin(puzzle_hash, 100);
        let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;

        let response = sim.push_tx(spend_bundle);
        assert!(response.success);
        assert_eq!(sim.get_blockchain_state().blockchain_state.unwrap().mempool_size, 1);

        let record = sim.get_coin_record_by_name(coin.coin_id()).coin_record.unwrap();
        assert!(!record.spent);
        assert!(sim.get_block_spends(sim.header_hash()).block_spends.unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn farm_block_includes_mempool_and_emits_event() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let coin = sim.new_coin(puzzle_hash, 100);
        let child = Coin::new(coin.coin_id(), puzzle_hash, 99);
        let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;
        assert!(sim.push_tx(spend_bundle).success);

        let records = sim.farm_block(1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].height, 1);
        assert_eq!(sim.get_blockchain_state().blockchain_state.unwrap().mempool_size, 0);

        let spent = sim.get_coin_record_by_name(coin.coin_id()).coin_record.unwrap();
        assert!(spent.spent);
        assert_eq!(spent.spent_block_index, 1);

        let created = sim.get_coin_record_by_name(child.coin_id()).coin_record.unwrap();
        assert!(!created.spent);
        assert_eq!(created.confirmed_block_index, 1);

        let spends = sim.get_block_spends(records[0].header_hash).block_spends.unwrap();
        assert_eq!(spends.len(), 1);

        let events = sim.drain_events();
        assert!(matches!(events.as_slice(), [FullNodeSimulatorEvent::Block { height: 1, .. }]));

        Ok(())
    }

    #[test]
    fn reorg_replaces_peak_and_emits_reorg() {
        let mut sim = FullNodeSimulator::new();
        let old_peak = sim.farm_block(2).last().unwrap().header_hash;

        let new_blocks = sim.reorg_blocks(1, 2);
        assert_eq!(new_blocks.len(), 2);
        assert_ne!(sim.header_hash(), old_peak);
        assert_eq!(sim.height(), 3);

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
