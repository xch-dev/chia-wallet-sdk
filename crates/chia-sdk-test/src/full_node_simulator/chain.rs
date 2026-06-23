use super::*;

impl FullNodeSimulator {
    pub(super) fn create_block_from_mempool(&mut self) -> BlockRecord {
        let previous_header_hash = self.header_hash();
        let height = self.height + 1;
        let timestamp = self.next_timestamp;
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
        let mut applied_removals = IndexSet::new();
        let mut applied_additions = IndexSet::new();
        let mut applied_spends = IndexSet::new();
        let reward_coin = Self::reward_coin(
            header_hash,
            height,
            0,
            self.farming_puzzle_hash,
            BLOCK_REWARD_AMOUNT,
        );
        let reward_coin_id = reward_coin.coin_id();
        self.insert_coin_record(reward_coin, true, height, timestamp);
        additions.push(reward_coin_id);

        for item in included {
            fees = fees.saturating_add(item.fee);
            transactions.push(item.spend_bundle);
            let ephemeral_removals = item
                .additions
                .iter()
                .map(|(coin, _)| coin.coin_id())
                .collect::<IndexSet<_>>();
            let mut pending_ephemeral_removals = Vec::new();

            for coin_id in item.removals {
                if let Some(record) = self.coins.get_mut(&coin_id) {
                    if !applied_removals.insert(coin_id) {
                        continue;
                    }
                    previous_coin_records.push((coin_id, *record));
                    record.spent_block_index = Some(height);
                    removals.push(coin_id);
                } else if ephemeral_removals.contains(&coin_id) {
                    pending_ephemeral_removals.push(coin_id);
                }
            }

            for (coin, hint) in item.additions {
                let coin_id = coin.coin_id();
                if coin.amount == 100_000 {
                    eprintln!(
                        "[DEBUG-SIM-RESTORE] create_block_from_mempool addition amount={} puzzle_hash={:?} coin_id={:?} height={} duplicate={}",
                        coin.amount,
                        coin.puzzle_hash,
                        coin_id,
                        height,
                        applied_additions.contains(&coin_id),
                    );
                }
                if !applied_additions.insert(coin_id) {
                    continue;
                }
                self.insert_coin_record(coin, false, height, timestamp);
                if let Some(hint) = hint {
                    self.coin_hints.insert(coin_id, hint);
                    added_hints.push(coin_id);
                }
                additions.push(coin_id);
            }

            for coin_id in pending_ephemeral_removals {
                if !applied_removals.insert(coin_id) {
                    continue;
                }
                if let Some(record) = self.coins.get_mut(&coin_id) {
                    record.spent_block_index = Some(height);
                    removals.push(coin_id);
                }
            }

            for (coin_id, spend) in item.spends {
                if applied_spends.insert(coin_id) {
                    spends.push(spend.coin_spend);
                }
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

    pub(super) fn revert_canonical_blocks(&mut self, blocks: u32) -> Vec<SimBlock> {
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

    pub(super) fn requeue_transactions(
        &mut self,
        transactions: impl IntoIterator<Item = SpendBundle>,
    ) {
        for spend_bundle in transactions {
            let tx_id = spend_bundle.name();
            if self.mempool.contains_key(&tx_id) {
                continue;
            }
            if let Ok(validated) = self.validate_bundle(spend_bundle) {
                let _ = self.insert_mempool_item(tx_id, validated);
            }
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
}
