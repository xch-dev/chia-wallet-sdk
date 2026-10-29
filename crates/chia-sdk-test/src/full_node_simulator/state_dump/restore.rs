use chia_protocol::{Bytes32, CoinSpend};
use indexmap::{IndexMap, IndexSet};

use crate::{SimulatorError, StateDumpError};

use super::super::{
    FullNodeSimulator, SimBlock, SimCoinRecord, ValidatedBundle,
    state::{BlockDelta, ChainState, CoinChange, HintChange, SpendChange},
    validation::ValidationOverlay,
};
use super::{DumpBlock, FORMAT, SimulatorStateDump, VERSION};

impl DumpBlock {
    fn into_block(
        self,
        simulator: &FullNodeSimulator,
        final_coins: &IndexMap<Bytes32, SimCoinRecord>,
        final_spends: &IndexMap<Bytes32, CoinSpend>,
        final_hints: &IndexMap<Bytes32, Bytes32>,
    ) -> Result<(Bytes32, SimBlock), SimulatorError> {
        let state = &simulator.state;
        if self.header_hash != self.record.header_hash {
            return Err(StateDumpError::BlockHeaderMismatch {
                block_key: self.header_hash,
                record_header_hash: self.record.header_hash,
            }
            .into());
        }

        let height = self.record.height;
        let timestamp = self
            .record
            .timestamp
            .ok_or(StateDumpError::MissingBlockTimestamp)?;
        validate_unique_ids("block additions", &self.additions)?;
        validate_unique_ids("block removals", &self.removals)?;
        validate_unique_ids("block added hints", &self.added_hints)?;
        let (derived_hints, reward_coin_ids) = self.validate_contents(simulator)?;

        let mut changed_coin_ids = IndexSet::new();
        for coin_id in self.additions.iter().chain(&self.removals) {
            changed_coin_ids.insert(*coin_id);
        }
        let previous_coin_records =
            collect_unique_pairs("block previous coin records", self.previous_coin_records)?;
        for coin_id in &changed_coin_ids {
            if state.coins.get(coin_id) != previous_coin_records.get(coin_id) {
                return Err(StateDumpError::PreviousCoinRecordMismatch(*coin_id).into());
            }
        }
        if previous_coin_records
            .keys()
            .any(|coin_id| !changed_coin_ids.contains(coin_id))
        {
            return Err(StateDumpError::PreviousCoinRecordForUnchangedCoin.into());
        }

        let coins = changed_coin_ids
            .into_iter()
            .map(|coin_id| {
                let before = state.coins.get(&coin_id).copied();
                let mut after = if self.additions.contains(&coin_id) {
                    let mut record = final_coins
                        .get(&coin_id)
                        .copied()
                        .ok_or(StateDumpError::MissingFinalCoinRecord(coin_id))?;
                    if record.coin.coin_id() != coin_id
                        || record.coinbase != reward_coin_ids.contains(&coin_id)
                        || record.confirmed_block_index != height
                        || record.timestamp != timestamp
                    {
                        return Err(StateDumpError::InconsistentAddedCoinRecord(height).into());
                    }
                    record.spent_block_index = None;
                    Some(record)
                } else {
                    before
                };
                if self.removals.contains(&coin_id) {
                    let record = after
                        .as_mut()
                        .ok_or(StateDumpError::UnknownRemovedCoin(coin_id))?;
                    record.spent_block_index = Some(height);
                }
                Ok(CoinChange {
                    coin_id,
                    before,
                    after,
                })
            })
            .collect::<Result<Vec<_>, SimulatorError>>()?;

        let mut spend_ids = IndexSet::new();
        let spends = self
            .spends
            .iter()
            .cloned()
            .map(|spend| {
                let coin_id = spend.coin.coin_id();
                if !spend_ids.insert(coin_id) {
                    return Err(StateDumpError::DuplicateBlockSpend.into());
                }
                if !self.removals.contains(&coin_id) {
                    return Err(StateDumpError::SpendNotRemoval.into());
                }
                if final_spends.get(&coin_id) != Some(&spend) {
                    return Err(StateDumpError::CoinSpendIndexMismatch(coin_id).into());
                }
                Ok(SpendChange {
                    coin_id,
                    before: state.coin_spends.get(&coin_id).cloned(),
                    after: Some(spend),
                })
            })
            .collect::<Result<Vec<_>, SimulatorError>>()?;

        let hints = self
            .added_hints
            .iter()
            .map(|coin_id| {
                if !self.additions.contains(coin_id) {
                    return Err(StateDumpError::HintNotAddition.into());
                }
                let hint = final_hints
                    .get(coin_id)
                    .copied()
                    .ok_or(StateDumpError::MissingFinalHint(*coin_id))?;
                if derived_hints.get(coin_id) != Some(&hint) {
                    return Err(StateDumpError::CoinHintMismatch(*coin_id).into());
                }
                Ok(HintChange {
                    coin_id: *coin_id,
                    before: state.coin_hints.get(coin_id).copied(),
                    after: Some(hint),
                })
            })
            .collect::<Result<Vec<_>, SimulatorError>>()?;

        Ok((
            self.header_hash,
            SimBlock {
                record: self.record,
                additions: self.additions,
                removals: self.removals,
                spends: self.spends,
                transactions: self.transactions,
                delta: BlockDelta {
                    coins,
                    spends,
                    hints,
                },
            },
        ))
    }

    fn validate_contents(
        &self,
        simulator: &FullNodeSimulator,
    ) -> Result<(IndexMap<Bytes32, Bytes32>, IndexSet<Bytes32>), SimulatorError> {
        let mut additions = IndexSet::new();
        let mut reward_coin_ids = IndexSet::new();
        if let Some(rewards) = &self.record.reward_claims_incorporated {
            for coin in rewards {
                let coin_id = coin.coin_id();
                additions.insert(coin_id);
                reward_coin_ids.insert(coin_id);
            }
        }
        let mut removals = IndexSet::new();
        let mut spends = IndexMap::new();
        let mut hints = IndexMap::new();
        let mut overlay = ValidationOverlay::default();

        for transaction in &self.transactions {
            let validated = simulator.validate_bundle_in_block(transaction.clone(), &overlay)?;
            overlay.apply(&validated);
            for (coin, hint) in &validated.additions {
                let coin_id = coin.coin_id();
                if additions.insert(coin_id)
                    && let Some(hint) = *hint
                {
                    hints.insert(coin_id, hint);
                }
            }
            for coin_id in &validated.removals {
                removals.insert(*coin_id);
            }
            for (coin_id, spend) in &validated.spends {
                spends
                    .entry(*coin_id)
                    .or_insert_with(|| spend.coin_spend.clone());
            }
        }

        if additions.iter().copied().collect::<Vec<_>>() != self.additions {
            return Err(StateDumpError::BlockAdditionsMismatch.into());
        }
        if removals.iter().copied().collect::<Vec<_>>() != self.removals {
            return Err(StateDumpError::BlockRemovalsMismatch.into());
        }
        if spends.into_values().collect::<Vec<_>>() != self.spends {
            return Err(StateDumpError::BlockSpendsMismatch.into());
        }
        if hints.keys().copied().collect::<Vec<_>>() != self.added_hints {
            return Err(StateDumpError::BlockHintsMismatch.into());
        }
        Ok((hints, reward_coin_ids))
    }
}

fn validate_unique_ids(label: &'static str, ids: &[Bytes32]) -> Result<(), SimulatorError> {
    let mut unique = IndexSet::new();
    if ids.iter().any(|id| !unique.insert(*id)) {
        return Err(StateDumpError::DuplicateKey(label).into());
    }
    Ok(())
}

fn collect_unique_pairs<T>(
    label: &'static str,
    pairs: Vec<(Bytes32, T)>,
) -> Result<IndexMap<Bytes32, T>, SimulatorError> {
    let mut values = IndexMap::new();
    for (key, value) in pairs {
        if values.insert(key, value).is_some() {
            return Err(StateDumpError::DuplicateKey(label).into());
        }
    }
    Ok(values)
}

fn validate_index<T: PartialEq>(
    expected: &IndexMap<Bytes32, T>,
    actual: &IndexMap<Bytes32, T>,
    label: &'static str,
) -> Result<(), SimulatorError> {
    if expected.len() != actual.len()
        || expected
            .iter()
            .any(|(coin_id, value)| actual.get(coin_id) != Some(value))
    {
        return Err(StateDumpError::SerializedIndexMismatch(label).into());
    }
    Ok(())
}

impl TryFrom<SimulatorStateDump> for FullNodeSimulator {
    type Error = SimulatorError;

    fn try_from(dump: SimulatorStateDump) -> Result<Self, Self::Error> {
        if dump.format != FORMAT {
            return Err(StateDumpError::UnsupportedFormat(dump.format).into());
        }
        if dump.version != VERSION {
            return Err(StateDumpError::UnsupportedVersion(dump.version).into());
        }
        if dump.height as usize != dump.header_hashes.len() {
            return Err(StateDumpError::HeightHeaderCountMismatch {
                height: dump.height,
                header_count: dump.header_hashes.len(),
            }
            .into());
        }
        if dump.blocks.len() != dump.header_hashes.len() {
            return Err(StateDumpError::HeaderBlockCountMismatch {
                header_count: dump.header_hashes.len(),
                block_count: dump.blocks.len(),
            }
            .into());
        }
        validate_unique_ids("canonical header hashes", &dump.header_hashes)?;

        let coins = collect_unique_pairs("serialized coin records", dump.coins)?;
        for (coin_id, record) in &coins {
            if record.coin.coin_id() != *coin_id {
                return Err(StateDumpError::CoinRecordKeyMismatch(*coin_id).into());
            }
        }
        let coin_spends = collect_unique_pairs("serialized coin spends", dump.coin_spends)?;
        for (coin_id, spend) in &coin_spends {
            if spend.coin.coin_id() != *coin_id {
                return Err(StateDumpError::CoinSpendKeyMismatch(*coin_id).into());
            }
        }
        let coin_hints = collect_unique_pairs("serialized coin hints", dump.coin_hints)?;
        let master_secret_key = chia_bls::SecretKey::from_bytes(&dump.master_secret_key)
            .map_err(|error| StateDumpError::InvalidMasterSecretKey(error.to_string()))?;

        let first_timestamp = dump
            .blocks
            .first()
            .and_then(|block| block.record.timestamp)
            .ok_or(StateDumpError::MissingGenesisTimestamp)?;
        let mut simulator = Self {
            rng: dump.rng,
            state: ChainState::new(
                0,
                first_timestamp,
                Vec::new(),
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
            ),
            orphaned_blocks: IndexMap::new(),
            mempool: IndexMap::<Bytes32, ValidatedBundle>::new(),
            farming_puzzle_hash: dump.farming_puzzle_hash,
            master_secret_key,
            prefarm_puzzle_hash: dump.prefarm_puzzle_hash,
            node_id: dump.node_id,
            events: Vec::new(),
        };
        for (expected_header, block) in dump.header_hashes.iter().zip(dump.blocks) {
            if block.header_hash != *expected_header {
                return Err(StateDumpError::CanonicalBlockOrderMismatch.into());
            }
            let (_, block) = block.into_block(&simulator, &coins, &coin_spends, &coin_hints)?;
            simulator.state.apply_block(block)?;
        }
        if simulator.state.height != dump.height {
            return Err(StateDumpError::ReplayedHeightMismatch.into());
        }
        if simulator.state.next_timestamp != dump.next_timestamp {
            return Err(StateDumpError::NextTimestampMismatch.into());
        }
        validate_index(&coins, &simulator.state.coins, "coin records")?;
        validate_index(&coin_spends, &simulator.state.coin_spends, "coin spends")?;
        validate_index(&coin_hints, &simulator.state.coin_hints, "coin hints")?;

        Ok(simulator)
    }
}
