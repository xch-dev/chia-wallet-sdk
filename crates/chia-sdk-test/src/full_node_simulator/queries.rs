use std::collections::HashSet;

use chia_bls::SecretKey;
use chia_protocol::Bytes32;
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainState, BlockchainStateResponse, CoinRecord,
    GetBlockRecordResponse, GetBlockRecordsResponse, GetBlockSpendsResponse, GetCoinRecordResponse,
    GetCoinRecordsResponse, GetMempoolItemResponse, GetMempoolItemsResponse,
    GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, MempoolMinFees, SyncState,
};

use super::{FullNodeSimulator, SimCoinRecord, ValidatedBundle};

impl FullNodeSimulator {
    /// Returns the height of the peak block. The genesis block is at height 1.
    pub fn height(&self) -> u32 {
        self.state.height
    }

    /// Returns the header hash of the peak block.
    ///
    /// The genesis header hash is all zeros. Later header hashes are random.
    pub fn header_hash(&self) -> Bytes32 {
        self.state.header_hashes.last().copied().unwrap_or_default()
    }

    /// Returns the header hash of the canonical block at `height`, or [`None`] if there is no
    /// block at that height.
    ///
    /// Height 0 returns the genesis header hash, the same as height 1.
    pub fn header_hash_of(&self, height: u32) -> Option<Bytes32> {
        self.state
            .header_hashes
            .get((height as usize).saturating_sub(1))
            .copied()
    }

    /// Returns the puzzle hash that receives block rewards. This defaults to the
    /// [`FullNodeSimulator::get_prefarm_puzzle_hash`].
    pub fn get_farming_ph(&self) -> Bytes32 {
        self.farming_puzzle_hash
    }

    /// Returns the master secret key that the prefarm puzzle hash is derived from.
    pub fn get_master_secret_key(&self) -> SecretKey {
        self.master_secret_key.clone()
    }

    /// Returns the puzzle hash of the genesis prefarm coins.
    ///
    /// This is the standard puzzle hash of the synthetic key for the hardened wallet key at index
    /// 1 of [`FullNodeSimulator::get_master_secret_key`].
    pub fn get_prefarm_puzzle_hash(&self) -> Bytes32 {
        self.prefarm_puzzle_hash
    }

    /// Returns the blockchain state, as the `get_blockchain_state` RPC would.
    ///
    /// The peak is the current tip, the mempool totals reflect the current mempool, and the node
    /// always reports itself as synced. Values such as difficulty and space are fixed.
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

    /// Returns the network info. The network name is `simulator0`, the address prefix is `txch`,
    /// and the genesis challenge is [`FullNodeSimulator::get_aggsig_additional_data`].
    pub fn get_network_info(&self) -> GetNetworkInfoResponse {
        GetNetworkInfoResponse {
            network_name: Some("simulator0".to_string()),
            network_prefix: Some("txch".to_string()),
            genesis_challenge: Some(self.genesis_challenge),
            error: None,
            success: true,
        }
    }

    /// Returns the `AGG_SIG_ME` additional data that signatures must be made with.
    ///
    /// This is the simulator's genesis challenge. Unless it was changed with
    /// [`FullNodeSimulator::set_genesis_challenge`], it isn't the testnet11 value used by
    /// [`sign_transaction`](crate::sign_transaction).
    pub fn get_aggsig_additional_data(&self) -> Bytes32 {
        self.genesis_challenge
    }

    /// Returns the record of the block with the given header hash, including blocks orphaned by
    /// [`FullNodeSimulator::reorg_blocks`].
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

    /// Returns the record of the canonical block at `height`.
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

    /// Returns the records of the canonical blocks from `start` (inclusive) to `end` (exclusive).
    /// Heights without a block are skipped.
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

    /// Returns the coins created and spent by the block with the given header hash, including
    /// orphaned blocks.
    ///
    /// The coin records reflect the current coin set, not the state at that block. The response
    /// is unsuccessful if the block doesn't exist.
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

    /// Returns the coin spends in the block with the given header hash, including orphaned blocks.
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

    /// Returns the record of the coin with the given id, whether or not it's spent.
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

    /// Returns the records of the coins with the given ids.
    ///
    /// Only unspent coins are returned unless `include_spent_coins` is `Some(true)`. If given,
    /// `start_height` (inclusive) and `end_height` (exclusive) filter by confirmed height.
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

    /// Returns the records of the coins hinted with `hint`.
    ///
    /// Only unspent coins are returned unless `include_spent_coins` is `Some(true)`. If given,
    /// `start_height` (inclusive) and `end_height` (exclusive) filter by confirmed height.
    pub fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> GetCoinRecordsResponse {
        self.get_coin_records_by_hints(vec![hint], start_height, end_height, include_spent_coins)
    }

    /// Returns the records of the coins hinted with any of the given hints.
    ///
    /// Only unspent coins are returned unless `include_spent_coins` is `Some(true)`. If given,
    /// `start_height` (inclusive) and `end_height` (exclusive) filter by confirmed height.
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

    /// Returns the records of the coins whose parent coin id is one of `parent_ids`.
    ///
    /// Only unspent coins are returned unless `include_spent_coins` is `Some(true)`. If given,
    /// `start_height` (inclusive) and `end_height` (exclusive) filter by confirmed height.
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

    /// Returns the records of the coins with the given puzzle hash.
    ///
    /// Only unspent coins are returned unless `include_spent_coins` is `Some(true)`. If given,
    /// `start_height` (inclusive) and `end_height` (exclusive) filter by confirmed height.
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

    /// Returns the records of the coins with any of the given puzzle hashes.
    ///
    /// Only unspent coins are returned unless `include_spent_coins` is `Some(true)`. If given,
    /// `start_height` (inclusive) and `end_height` (exclusive) filter by confirmed height.
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

    /// Returns the coin spend of a spent coin.
    ///
    /// If `height` is given, the spend is only returned if the coin was spent at that height.
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

    /// Returns the mempool item with the given spend bundle id.
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

    /// Returns the mempool items that spend the coin with the given id.
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
