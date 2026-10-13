use std::collections::HashMap;

use chia_bls::SecretKey;
use chia_consensus::{flags::MEMPOOL_MODE, validation_error::ErrorCode};
use chia_protocol::{Bytes32, Coin, CoinSpend, CoinState, Program, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use clvmr::ENABLE_KECCAK_OPS_OUTSIDE_GUARD;
use indexmap::{IndexMap, IndexSet, indexset};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::{
    BlsPair, BlsPairWithCoin, SimulatorError, sign_transaction,
    spend_bundle_validation::{
        ValidationClock, ValidationSettings, coin_record, ephemeral_coin_record,
        validate_conditions, validate_reserve_fee, validate_time_locks,
    },
};

mod config;
mod data;

pub use config::*;

use data::SimulatorData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Simulator {
    config: SimulatorConfig,
    data: SimulatorData,
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulator {
    pub fn new() -> Self {
        Self::with_config(SimulatorConfig::default())
    }

    pub fn with_config(config: SimulatorConfig) -> Self {
        let genesis_timestamp = config.timestamp_mode.genesis_timestamp();
        Self {
            config,
            data: SimulatorData::new(
                ChaCha8Rng::seed_from_u64(config.seed),
                genesis_timestamp,
                config.timestamp_mode.next_timestamp(genesis_timestamp),
            ),
        }
    }

    pub fn config(&self) -> SimulatorConfig {
        self.config
    }

    #[cfg(feature = "serde")]
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(&self.data, bincode::config::standard())
    }

    #[cfg(feature = "serde")]
    pub fn deserialize_with_config(
        data: &[u8],
        config: SimulatorConfig,
    ) -> Result<Self, bincode::error::DecodeError> {
        let data: SimulatorData =
            bincode::serde::decode_from_slice(data, bincode::config::standard())?.0;
        Ok(Self { config, data })
    }

    #[cfg(feature = "serde")]
    pub fn deserialize(data: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        Self::deserialize_with_config(data, SimulatorConfig::default())
    }

    /// The height of the peak block.
    ///
    /// Like mainnet, transactions are validated against the peak and included in the next block,
    /// so a height lock of `height() + 1` only passes after another block has been created.
    pub fn height(&self) -> u32 {
        self.data.height
    }

    /// The timestamp of the peak block, which transactions are validated against.
    pub fn timestamp(&self) -> u64 {
        self.block_timestamp(self.data.height)
    }

    /// The timestamp the next block will have.
    ///
    /// Transactions are validated against [`Simulator::timestamp`], so time locks that depend on
    /// this timestamp only pass after the next block has been created.
    pub fn next_timestamp(&self) -> u64 {
        self.data.next_timestamp
    }

    fn block_timestamp(&self, height: u32) -> u64 {
        self.data
            .block_timestamps
            .get(&height)
            .copied()
            .unwrap_or_default()
    }

    pub fn header_hash(&self) -> Bytes32 {
        self.data.header_hashes.last().copied().unwrap()
    }

    pub fn header_hash_of(&self, height: u32) -> Option<Bytes32> {
        self.data.header_hashes.get(height as usize).copied()
    }

    pub fn insert_coin(&mut self, coin: Coin) {
        let coin_state = CoinState::new(coin, None, Some(self.data.height));
        self.data.coin_states.insert(coin.coin_id(), coin_state);
    }

    pub fn new_coin(&mut self, puzzle_hash: Bytes32, amount: u64) -> Coin {
        let mut parent_coin_info = [0; 32];
        self.data.rng.fill(&mut parent_coin_info);
        let coin = Coin::new(parent_coin_info.into(), puzzle_hash, amount);
        self.insert_coin(coin);
        coin
    }

    pub fn bls(&mut self, amount: u64) -> BlsPairWithCoin {
        let pair = BlsPair::new(self.data.rng.random());
        let coin = self.new_coin(pair.puzzle_hash, amount);
        BlsPairWithCoin::new(pair, coin)
    }

    /// Sets the timestamp of the next block. Call [`Simulator::create_block`] afterward for
    /// transactions to be validated against it.
    pub fn set_next_timestamp(&mut self, time: u64) -> Result<(), SimulatorError> {
        if time < self.timestamp() {
            return Err(SimulatorError::Validation(ErrorCode::TimestampTooFarInPast));
        }
        self.data.next_timestamp = time;

        Ok(())
    }

    /// Advances the timestamp of the next block. Call [`Simulator::create_block`] afterward for
    /// transactions to be validated against it.
    pub fn pass_time(&mut self, time: u64) {
        self.data.next_timestamp += time;
    }

    pub fn hint_coin(&mut self, coin_id: Bytes32, hint: Bytes32) {
        self.data
            .hinted_coins
            .entry(hint)
            .or_default()
            .insert(coin_id);
    }

    pub fn coin_state(&self, coin_id: Bytes32) -> Option<CoinState> {
        self.data.coin_states.get(&coin_id).copied()
    }

    pub fn children(&self, coin_id: Bytes32) -> Vec<CoinState> {
        self.data
            .coin_states
            .values()
            .filter(move |cs| cs.coin.parent_coin_info == coin_id)
            .copied()
            .collect()
    }

    pub fn hinted_coins(&self, hint: Bytes32) -> Vec<Bytes32> {
        self.data
            .hinted_coins
            .get(&hint)
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    pub fn puzzle_reveal(&self, coin_id: Bytes32) -> Option<Program> {
        self.data
            .coin_spends
            .get(&coin_id)
            .map(|spend| spend.puzzle_reveal.clone())
    }

    pub fn solution(&self, coin_id: Bytes32) -> Option<Program> {
        self.data
            .coin_spends
            .get(&coin_id)
            .map(|spend| spend.solution.clone())
    }

    pub fn puzzle_and_solution(&self, coin_id: Bytes32) -> Option<(Program, Program)> {
        self.data
            .coin_spends
            .get(&coin_id)
            .map(|spend| (spend.puzzle_reveal.clone(), spend.solution.clone()))
    }

    pub fn coin_spend(&self, coin_id: Bytes32) -> Option<CoinSpend> {
        self.data.coin_spends.get(&coin_id).cloned()
    }

    pub fn spend_coins(
        &mut self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: &[SecretKey],
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        let signature = sign_transaction(&coin_spends, secret_keys)?;
        self.new_transaction(SpendBundle::new(coin_spends, signature))
    }

    /// Validates a spend bundle against the peak, includes it in a new block, and returns the
    /// updated coin states.
    pub fn new_transaction(
        &mut self,
        spend_bundle: SpendBundle,
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        let clock = ValidationClock {
            height: self.data.height,
            timestamp: self.timestamp(),
        };
        let mut flags = ENABLE_KECCAK_OPS_OUTSIDE_GUARD;
        if self.config.mempool_mode {
            flags |= MEMPOOL_MODE;
        }
        let validation = validate_conditions(
            &spend_bundle,
            ValidationSettings {
                constants: &TESTNET11_CONSTANTS,
                max_cost: 11_000_000_000 / 2,
                flags,
            },
        )
        .map_err(SimulatorError::Validation)?;
        let conds = validation.conditions;
        validate_reserve_fee(&conds, validation.fee).map_err(SimulatorError::Validation)?;

        let block_height = self.data.height + 1;

        let mut removed_coins = IndexMap::new();
        let mut added_coins = IndexMap::new();
        let mut added_hints = IndexMap::new();
        let mut coin_spends = IndexMap::new();

        for coin_spend in spend_bundle.coin_spends {
            coin_spends.insert(coin_spend.coin.coin_id(), coin_spend);
        }

        // Calculate additions.
        for parsed in validation.additions {
            for addition in parsed.additions {
                let coin = addition.coin;
                added_coins.insert(
                    coin.coin_id(),
                    CoinState::new(coin, None, Some(block_height)),
                );

                let Some(hint) = addition.hint else {
                    continue;
                };

                added_hints
                    .entry(hint)
                    .or_insert_with(IndexSet::new)
                    .insert(coin.coin_id());
            }
        }

        // Validate removals.
        let mut removal_records = HashMap::new();

        for spend in &conds.spends {
            let (coin_state, record) =
                if let Some(coin_state) = self.data.coin_states.get(&spend.coin_id) {
                    if coin_state.spent_height.is_some() {
                        return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
                    }
                    let created_height = coin_state.created_height.unwrap_or_default();
                    let record = coin_record(
                        coin_state.coin,
                        created_height,
                        false,
                        self.block_timestamp(created_height),
                    );
                    (*coin_state, record)
                } else if let Some(coin_state) = added_coins.get(&spend.coin_id) {
                    (*coin_state, ephemeral_coin_record(coin_state.coin, clock))
                } else {
                    return Err(SimulatorError::Validation(ErrorCode::UnknownUnspent));
                };

            removal_records.insert(spend.coin_id, record);
            removed_coins.insert(
                spend.coin_id,
                CoinState {
                    spent_height: Some(block_height),
                    ..coin_state
                },
            );
        }

        validate_time_locks(&conds, &removal_records, clock).map_err(SimulatorError::Validation)?;

        // Update the coin data.
        let mut updates = added_coins.clone();
        updates.extend(removed_coins);

        self.create_block();

        self.data.coin_states.extend(updates.clone());

        if self.config.save_hints {
            for (hint, coins) in added_hints {
                self.data
                    .hinted_coins
                    .entry(hint)
                    .or_default()
                    .extend(coins);
            }
        }

        if self.config.save_spends {
            self.data.coin_spends.extend(coin_spends);
        }

        Ok(updates)
    }

    pub fn lookup_coin_ids(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
        coin_ids
            .iter()
            .filter_map(|coin_id| self.data.coin_states.get(coin_id).copied())
            .collect()
    }

    pub fn lookup_puzzle_hashes(
        &self,
        puzzle_hashes: IndexSet<Bytes32>,
        include_hints: bool,
    ) -> Vec<CoinState> {
        let mut coin_states = IndexMap::new();

        for (coin_id, coin_state) in &self.data.coin_states {
            if puzzle_hashes.contains(&coin_state.coin.puzzle_hash) {
                coin_states.insert(*coin_id, *coin_state);
            }
        }

        if include_hints {
            for puzzle_hash in puzzle_hashes {
                if let Some(hinted_coins) = self.data.hinted_coins.get(&puzzle_hash) {
                    // Coins can be hinted with `hint_coin` before (or without) being created.
                    for coin_id in hinted_coins {
                        if let Some(coin_state) = self.data.coin_states.get(coin_id) {
                            coin_states.insert(*coin_id, *coin_state);
                        }
                    }
                }
            }
        }

        coin_states.into_values().collect()
    }

    pub fn unspent_coins(&self, puzzle_hash: Bytes32, include_hints: bool) -> Vec<Coin> {
        self.lookup_puzzle_hashes(indexset![puzzle_hash], include_hints)
            .iter()
            .filter(|cs| cs.spent_height.is_none())
            .map(|cs| cs.coin)
            .collect()
    }

    pub fn create_block(&mut self) {
        let mut header_hash = [0; 32];
        self.data.rng.fill(&mut header_hash);
        self.data.header_hashes.push(header_hash.into());

        let timestamp = self.data.next_timestamp;
        self.data.height += 1;
        self.data
            .block_timestamps
            .insert(self.data.height, timestamp);
        self.data.next_timestamp = self.config.timestamp_mode.next_timestamp(timestamp);
    }
}
