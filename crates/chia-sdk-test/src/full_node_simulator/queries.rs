use std::collections::HashSet;

use chia_bls::SecretKey;
use chia_protocol::Bytes32;
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainState, BlockchainStateResponse, CoinRecord,
    GetBlockRecordResponse, GetBlockRecordsResponse, GetBlockSpendsResponse, GetCoinRecordResponse,
    GetCoinRecordsResponse, GetMempoolItemResponse, GetMempoolItemsResponse,
    GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, MempoolMinFees, SyncState,
};

use super::{FullNodeSimulator, SIMULATOR_GENESIS_CHALLENGE, SimCoinRecord, ValidatedBundle};

impl FullNodeSimulator {
    pub fn height(&self) -> u32 {
        self.state.height
    }

    pub fn header_hash(&self) -> Bytes32 {
        self.state.header_hashes.last().copied().unwrap_or_default()
    }

    pub fn header_hash_of(&self, height: u32) -> Option<Bytes32> {
        self.state
            .header_hashes
            .get((height as usize).saturating_sub(1))
            .copied()
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

    pub fn get_blockchain_state(&self) -> BlockchainStateResponse {
        let peak = self.state.blocks.get(&self.header_hash()).map_or_else(
            || {
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
            },
            |block| block.record.clone(),
        );

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
                mempool_size: self.mempool.len().try_into().unwrap(),
                node_id: self.node_id,
                peak,
                space: 0,
                sub_slot_iters: 1,
                sync: SyncState {
                    sync_mode: false,
                    sync_progress_height: self.state.height,
                    sync_tip_height: self.state.height,
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
                .state
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
            .and_then(|header_hash| self.state.blocks.get(&header_hash))
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
            .state
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
                .state
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
            coin_record: self
                .state
                .coins
                .get(&name)
                .map(|record| record.to_coin_record()),
            error: None,
            success: true,
        }
    }

    pub fn get_coin_records_by_names(
        &self,
        names: &[Bytes32],
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        Self::records_response(
            self.state
                .coins
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
        Self::records_response(
            self.state
                .coins
                .iter()
                .filter(|(coin_id, _)| {
                    self.state
                        .coin_hints
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
        Self::records_response(
            self.state
                .coins
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
        Self::records_response(
            self.state
                .coins
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
        let coin_solution = self.state.coin_spends.get(&coin_id).and_then(|spend| {
            let record = self.state.coins.get(&coin_id)?;
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

    pub(super) fn records_for_ids(&self, coin_ids: &[Bytes32]) -> Vec<CoinRecord> {
        coin_ids
            .iter()
            .filter_map(|coin_id| self.state.coins.get(coin_id))
            .map(|record| record.to_coin_record())
            .collect()
    }

    fn records_response(
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
            .map(SimCoinRecord::to_coin_record)
            .collect();

        GetCoinRecordsResponse {
            coin_records: Some(records),
            error: None,
            success: true,
            next_cursor: None,
            truncated: None,
        }
    }
}
