use std::collections::VecDeque;

use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{BlockRecord, Bytes32, SpendBundle};
use indexmap::{IndexMap, IndexSet};
use rand::Rng;

use crate::{FullNodeSimulatorEvent, SimulatorError};

use super::{
    BLOCK_REWARD_AMOUNT, FullNodeSimulator, SimBlock, SimCoinRecord, ValidatedSpend,
    state::{BlockDelta, CoinChange, HintChange, SpendChange},
};

impl FullNodeSimulator {
    pub(super) fn create_block_from_mempool(&mut self) -> BlockRecord {
        let previous_header_hash = self.header_hash();
        let height = self.state.height + 1;
        let timestamp = self.state.next_timestamp;
        let header_hash = self.random_hash();

        let mut included_tx_ids = Vec::new();
        let mut included = Vec::new();
        let mut included_spends_by_coin = IndexMap::<Bytes32, ValidatedSpend>::new();
        for (tx_id, item) in self.mempool.clone() {
            let Ok(validated) = self.validate_bundle(item.spend_bundle.clone()) else {
                continue;
            };
            let has_conflict = validated.removals.iter().any(|coin_id| {
                let Some(existing_spend) = included_spends_by_coin.get(coin_id) else {
                    return false;
                };
                let Some(new_spend) = validated.spends.get(coin_id) else {
                    return true;
                };
                !Self::spends_are_dedup_compatible(existing_spend, new_spend)
            });
            if has_conflict {
                continue;
            }
            for coin_id in &validated.removals {
                let Some(spend) = validated.spends.get(coin_id) else {
                    continue;
                };
                included_spends_by_coin
                    .entry(*coin_id)
                    .or_insert_with(|| spend.clone());
            }
            included_tx_ids.push(tx_id);
            included.push(validated);
        }

        let mut additions = Vec::new();
        let mut removals = Vec::new();
        let mut spends = Vec::new();
        let mut transactions = Vec::new();
        let mut fees = 0_u64;
        let mut applied_removals = IndexSet::new();
        let mut applied_additions = IndexSet::new();
        let mut applied_spends = IndexSet::new();
        let mut changed_coins = IndexSet::new();
        let mut changed_spends = IndexSet::new();
        let mut changed_hints = IndexSet::new();
        let mut staged_coins = self.state.coins.clone();
        let mut staged_coin_spends = self.state.coin_spends.clone();
        let mut staged_coin_hints = self.state.coin_hints.clone();
        let reward_coin = Self::reward_coin(
            header_hash,
            height,
            0,
            self.farming_puzzle_hash,
            BLOCK_REWARD_AMOUNT,
        );
        let reward_coin_id = reward_coin.coin_id();
        staged_coins.insert(
            reward_coin_id,
            SimCoinRecord {
                coin: reward_coin,
                coinbase: true,
                confirmed_block_index: height,
                spent_block_index: None,
                timestamp,
            },
        );
        changed_coins.insert(reward_coin_id);
        additions.push(reward_coin_id);

        for item in included {
            fees = fees.saturating_add(item.fee);
            transactions.push(item.spend_bundle);

            for (coin, hint) in item.additions {
                let coin_id = coin.coin_id();
                if !applied_additions.insert(coin_id) {
                    continue;
                }
                staged_coins.insert(
                    coin_id,
                    SimCoinRecord {
                        coin,
                        coinbase: false,
                        confirmed_block_index: height,
                        spent_block_index: None,
                        timestamp,
                    },
                );
                changed_coins.insert(coin_id);
                if let Some(hint) = hint {
                    staged_coin_hints.insert(coin_id, hint);
                    changed_hints.insert(coin_id);
                }
                additions.push(coin_id);
            }

            for coin_id in item.removals {
                if !applied_removals.insert(coin_id) {
                    continue;
                }
                if let Some(record) = staged_coins.get_mut(&coin_id) {
                    record.spent_block_index = Some(height);
                    changed_coins.insert(coin_id);
                    removals.push(coin_id);
                }
            }

            for (coin_id, spend) in item.spends {
                if applied_spends.insert(coin_id) {
                    staged_coin_spends.insert(coin_id, spend.coin_spend.clone());
                    changed_spends.insert(coin_id);
                    spends.push(spend.coin_spend);
                }
            }
        }

        let delta = BlockDelta {
            coins: changed_coins
                .into_iter()
                .map(|coin_id| CoinChange {
                    coin_id,
                    before: self.state.coins.get(&coin_id).copied(),
                    after: staged_coins.get(&coin_id).copied(),
                })
                .collect(),
            spends: changed_spends
                .into_iter()
                .map(|coin_id| SpendChange {
                    coin_id,
                    before: self.state.coin_spends.get(&coin_id).cloned(),
                    after: staged_coin_spends.get(&coin_id).cloned(),
                })
                .collect(),
            hints: changed_hints
                .into_iter()
                .map(|coin_id| HintChange {
                    coin_id,
                    before: self.state.coin_hints.get(&coin_id).copied(),
                    after: staged_coin_hints.get(&coin_id).copied(),
                })
                .collect(),
        };

        let record = Self::make_block_record(
            header_hash,
            previous_header_hash,
            height,
            timestamp,
            self.header_hash_of(height.saturating_sub(1))
                .unwrap_or_default(),
            fees,
            height.saturating_sub(1),
            self.farming_puzzle_hash,
            vec![reward_coin],
        );
        let block = SimBlock {
            record: record.clone(),
            additions: additions.clone(),
            removals: removals.clone(),
            spends,
            transactions,
            delta,
        };

        self.state
            .apply_block(block)
            .expect("locally built block delta must apply atomically");
        for tx_id in included_tx_ids {
            self.mempool.swap_remove(&tx_id);
        }

        self.events.push(FullNodeSimulatorEvent::Block {
            height,
            header_hash,
            previous_header_hash,
            additions: self.records_for_ids(&additions),
            removals: self.records_for_ids(&removals),
        });

        record
    }

    pub(super) fn revert_canonical_blocks(&mut self, blocks: u32) -> Vec<SimBlock> {
        let mut reverted = Vec::new();
        for _ in 0..blocks {
            let Some(block) = self
                .state
                .revert_tip()
                .expect("canonical tip delta must revert exactly")
            else {
                break;
            };
            reverted.push(block);
        }
        reverted.reverse();
        reverted
    }

    pub(super) fn requeue_transactions(
        &mut self,
        transactions: impl IntoIterator<Item = SpendBundle>,
    ) {
        let mut pending = transactions.into_iter().collect::<VecDeque<_>>();
        while !pending.is_empty() {
            let mut deferred = VecDeque::new();
            let mut made_progress = false;
            while let Some(spend_bundle) = pending.pop_front() {
                match self.normalize_and_insert(spend_bundle.clone()) {
                    Ok(()) => made_progress = true,
                    Err(SimulatorError::Validation(ErrorCode::UnknownUnspent)) => {
                        deferred.push_back(spend_bundle);
                    }
                    Err(_) => {}
                }
            }
            if !made_progress {
                break;
            }
            pending = deferred;
        }
    }

    pub(super) fn prune_mempool(&mut self) {
        let spend_bundles = self
            .mempool
            .values()
            .map(|item| item.spend_bundle.clone())
            .collect::<Vec<_>>();
        self.mempool.clear();
        self.requeue_transactions(spend_bundles);
    }

    fn random_hash(&mut self) -> Bytes32 {
        let mut bytes = [0; 32];
        self.rng.fill(&mut bytes);
        bytes.into()
    }

    pub fn drain_events(&mut self) -> Vec<FullNodeSimulatorEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn set_farming_ph(&mut self, puzzle_hash: Bytes32) {
        self.farming_puzzle_hash = puzzle_hash;
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
        let fork_height = self.state.height;
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
}
