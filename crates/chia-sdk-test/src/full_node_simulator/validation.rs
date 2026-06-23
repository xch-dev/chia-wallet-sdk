use super::*;

impl FullNodeSimulator {
    pub(super) fn validate_bundle(
        &self,
        spend_bundle: SpendBundle,
    ) -> Result<ValidatedBundle, SimulatorError> {
        if spend_bundle.coin_spends.is_empty() {
            return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
        }

        let constants = default_constants(SIMULATOR_GENESIS_CHALLENGE, SIMULATOR_GENESIS_CHALLENGE);
        let conds = validate_clvm_and_signature(
            &spend_bundle,
            11_000_000_000 / 2,
            &constants,
            ENABLE_KECCAK_OPS_OUTSIDE_GUARD | COMPUTE_FINGERPRINT,
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

        let bundle_coin_spends = spend_bundle
            .coin_spends
            .iter()
            .map(|spend| (spend.coin.coin_id(), spend.clone()))
            .collect::<IndexMap<_, _>>();

        let mut removals = IndexSet::new();
        let mut additions = IndexMap::new();
        let mut spends = IndexMap::new();

        for spend in &conds.spends {
            let coin_id = spend.coin_id;
            let mut spend_additions = Vec::new();

            for (puzzle_hash, amount, hint) in &spend.create_coin {
                let coin = Coin::new(coin_id, *puzzle_hash, *amount);
                if *amount == 100_000 {
                    eprintln!(
                        "[DEBUG-SIM-RESTORE] validate_bundle create_coin parent_coin_id={:?} amount={} puzzle_hash={:?} child_coin_id={:?} hint={:?}",
                        coin_id,
                        amount,
                        puzzle_hash,
                        coin.coin_id(),
                        hint,
                    );
                }
                let parsed_hint = hint
                    .as_ref()
                    .filter(|bytes| bytes.len() == 32)
                    .and_then(|bytes| Bytes32::try_from(bytes.as_ref()).ok());
                spend_additions.push((coin, parsed_hint));
                additions.insert(coin.coin_id(), (coin, parsed_hint));
            }

            let Some(coin_spend) = bundle_coin_spends.get(&coin_id).cloned() else {
                return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
            };

            let fingerprint = if (spend.flags & ELIGIBLE_FOR_DEDUP) != 0 {
                Bytes32::try_from(spend.fingerprint.as_ref()).ok()
            } else {
                None
            };

            spends.insert(
                coin_id,
                ValidatedSpend {
                    coin_spend,
                    flags: spend.flags,
                    fingerprint,
                    additions: spend_additions,
                },
            );
        }

        for spend in &conds.spends {
            let coin_id = spend.coin_id;
            if !removals.insert(coin_id) {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }

            if let Some(record) = self.coins.get(&coin_id) {
                if record.spent_block_index.is_some() {
                    return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
                }

                self.validate_relative_conditions(spend, record)?;
            } else if additions.contains_key(&coin_id) {
                let coin = additions
                    .get(&coin_id)
                    .map(|(coin, _)| *coin)
                    .unwrap_or_else(|| {
                        Coin::new(spend.parent_id, spend.puzzle_hash, spend.coin_amount)
                    });
                let ephemeral_coin_record = SimCoinRecord {
                    coin,
                    coinbase: false,
                    confirmed_block_index: self.height,
                    spent_block_index: None,
                    timestamp: self.next_timestamp,
                };
                self.validate_relative_conditions(spend, &ephemeral_coin_record)?;
            } else if let Some(coin) = self.mempool_addition_coin(coin_id) {
                let ephemeral_coin_record = SimCoinRecord {
                    coin,
                    coinbase: false,
                    confirmed_block_index: self.height,
                    spent_block_index: None,
                    timestamp: self.next_timestamp,
                };
                self.validate_relative_conditions(spend, &ephemeral_coin_record)?;
            } else {
                return Err(SimulatorError::Validation(ErrorCode::UnknownUnspent));
            }
        }

        let fee = conds
            .removal_amount
            .checked_sub(conds.addition_amount)
            .unwrap_or_default()
            .try_into()
            .unwrap_or(u64::MAX);
        if fee < conds.reserve_fee {
            return Err(SimulatorError::Validation(
                ErrorCode::ReserveFeeConditionFailed,
            ));
        }

        Ok(ValidatedBundle {
            spend_bundle,
            removals: removals.into_iter().collect(),
            additions: additions.into_values().collect(),
            spends,
            cost: conds.cost,
            fee,
        })
    }

    pub(super) fn mempool_addition_coin(&self, coin_id: Bytes32) -> Option<Coin> {
        self.mempool
            .values()
            .flat_map(|item| item.additions.iter().map(|(coin, _)| *coin))
            .find(|coin| coin.coin_id() == coin_id)
    }

    pub(super) fn validate_relative_conditions(
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
}
