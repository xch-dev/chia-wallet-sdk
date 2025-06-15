use std::collections::HashSet;

use chia_bls::SecretKey;
use chia_consensus::{
    spendbundle_validation::validate_clvm_and_signature, validation_error::ErrorCode,
};
use chia_protocol::{Bytes32, Coin, CoinSpend, CoinState, Program, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use indexmap::{indexset, IndexMap, IndexSet};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::{sign_transaction, BlsPair, BlsPairWithCoin, SimulatorError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Simulator {
    rng: ChaCha8Rng,
    height: u32,
    next_timestamp: u64,
    header_hashes: Vec<Bytes32>,
    coin_states: IndexMap<Bytes32, CoinState>,
    block_timestamps: IndexMap<u32, u64>,
    hinted_coins: IndexMap<Bytes32, IndexSet<Bytes32>>,
    puzzle_and_solutions: IndexMap<Bytes32, (Program, Program)>,
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulator {
    pub fn new() -> Self {
        Self::with_seed(1337)
    }

    pub fn with_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut header_hash = [0; 32];
        rng.fill(&mut header_hash);

        Self {
            rng,
            height: 0,
            next_timestamp: 0,
            header_hashes: vec![header_hash.into()],
            coin_states: IndexMap::new(),
            block_timestamps: IndexMap::new(),
            hinted_coins: IndexMap::new(),
            puzzle_and_solutions: IndexMap::new(),
        }
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn next_timestamp(&self) -> u64 {
        self.next_timestamp
    }

    pub fn header_hash(&self) -> Bytes32 {
        self.header_hashes.last().copied().unwrap()
    }

    pub fn header_hash_of(&self, height: u32) -> Option<Bytes32> {
        self.header_hashes.get(height as usize).copied()
    }

    pub fn insert_coin(&mut self, coin: Coin) {
        let coin_state = CoinState::new(coin, None, Some(self.height));
        self.coin_states.insert(coin.coin_id(), coin_state);
    }

    pub fn new_coin(&mut self, puzzle_hash: Bytes32, amount: u64) -> Coin {
        let mut parent_coin_info = [0; 32];
        self.rng.fill(&mut parent_coin_info);
        let coin = Coin::new(parent_coin_info.into(), puzzle_hash, amount);
        self.insert_coin(coin);
        coin
    }

    pub fn bls(&mut self, amount: u64) -> BlsPairWithCoin {
        let pair = BlsPair::new(self.rng.gen());
        let coin = self.new_coin(pair.puzzle_hash, amount);
        BlsPairWithCoin::new(pair, coin)
    }

    pub fn set_next_timestamp(&mut self, time: u64) -> Result<(), SimulatorError> {
        if self.height > 0 {
            if let Some(last_block_timestamp) = self.block_timestamps.get(&(self.height - 1)) {
                if time < *last_block_timestamp {
                    return Err(SimulatorError::Validation(ErrorCode::TimestampTooFarInPast));
                }
            }
        }
        self.next_timestamp = time;

        Ok(())
    }

    pub fn pass_time(&mut self, time: u64) {
        self.next_timestamp += time;
    }

    pub fn hint_coin(&mut self, coin_id: Bytes32, hint: Bytes32) {
        self.hinted_coins.entry(hint).or_default().insert(coin_id);
    }

    pub fn coin_state(&self, coin_id: Bytes32) -> Option<CoinState> {
        self.coin_states.get(&coin_id).copied()
    }

    pub fn children(&self, coin_id: Bytes32) -> Vec<CoinState> {
        self.coin_states
            .values()
            .filter(move |cs| cs.coin.parent_coin_info == coin_id)
            .copied()
            .collect()
    }

    pub fn hinted_coins(&self, hint: Bytes32) -> Vec<Bytes32> {
        self.hinted_coins
            .get(&hint)
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    pub fn puzzle_reveal(&self, coin_id: Bytes32) -> Option<Program> {
        self.puzzle_and_solutions
            .get(&coin_id)
            .map(|(p, _)| p.clone())
    }

    pub fn solution(&self, coin_id: Bytes32) -> Option<Program> {
        self.puzzle_and_solutions
            .get(&coin_id)
            .map(|(_, s)| s.clone())
    }

    pub fn puzzle_and_solution(&self, coin_id: Bytes32) -> Option<(Program, Program)> {
        self.puzzle_and_solutions.get(&coin_id).cloned()
    }

    pub fn spend_coins(
        &mut self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: &[SecretKey],
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        let signature = sign_transaction(&coin_spends, secret_keys)?;
        self.new_transaction(SpendBundle::new(coin_spends, signature))
    }

    /// Processes a spend bunndle and returns the updated coin states.
    pub fn new_transaction(
        &mut self,
        spend_bundle: SpendBundle,
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        if spend_bundle.coin_spends.is_empty() {
            return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
        }

        // TODO: Fix cost
        let (conds, _pairings, _duration) = validate_clvm_and_signature(
            &spend_bundle,
            7_700_000_000,
            &TESTNET11_CONSTANTS,
            self.height,
        )
        .map_err(SimulatorError::Validation)?;

        let puzzle_hashes: HashSet<Bytes32> =
            conds.spends.iter().map(|spend| spend.puzzle_hash).collect();

        let bundle_puzzle_hashes: HashSet<Bytes32> = spend_bundle
            .coin_spends
            .iter()
            .map(|cs| cs.coin.puzzle_hash)
            .collect();

        if puzzle_hashes != bundle_puzzle_hashes {
            return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
        }

        let mut removed_coins = IndexMap::new();
        let mut added_coins = IndexMap::new();
        let mut added_hints = IndexMap::new();
        let mut puzzle_solutions = IndexMap::new();

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

        if let Some(height) = conds.before_height_absolute {
            if height < self.height {
                return Err(SimulatorError::Validation(
                    ErrorCode::AssertBeforeHeightAbsoluteFailed,
                ));
            }
        }

        if let Some(seconds) = conds.before_seconds_absolute {
            if seconds < self.next_timestamp {
                return Err(SimulatorError::Validation(
                    ErrorCode::AssertBeforeSecondsAbsoluteFailed,
                ));
            }
        }

        for coin_spend in spend_bundle.coin_spends {
            puzzle_solutions.insert(
                coin_spend.coin.coin_id(),
                (coin_spend.puzzle_reveal, coin_spend.solution),
            );
        }

        // Calculate additions and removals.
        for spend in &conds.spends {
            for new_coin in &spend.create_coin {
                let coin = Coin::new(spend.coin_id, new_coin.0, new_coin.1);

                added_coins.insert(
                    coin.coin_id(),
                    CoinState::new(coin, None, Some(self.height)),
                );

                let Some(hint) = new_coin.2.clone() else {
                    continue;
                };

                if hint.len() != 32 {
                    continue;
                }

                added_hints
                    .entry(Bytes32::try_from(hint).unwrap())
                    .or_insert_with(IndexSet::new)
                    .insert(coin.coin_id());
            }

            let coin = Coin::new(spend.parent_id, spend.puzzle_hash, spend.coin_amount);

            let coin_state = self
                .coin_states
                .get(&spend.coin_id)
                .copied()
                .unwrap_or(CoinState::new(coin, None, Some(self.height)));

            if let Some(relative_height) = spend.height_relative {
                let Some(created_height) = coin_state.created_height else {
                    return Err(SimulatorError::Validation(
                        ErrorCode::EphemeralRelativeCondition,
                    ));
                };

                if self.height < created_height + relative_height {
                    return Err(SimulatorError::Validation(
                        ErrorCode::AssertHeightRelativeFailed,
                    ));
                }
            }

            if let Some(relative_seconds) = spend.seconds_relative {
                let Some(created_height) = coin_state.created_height else {
                    return Err(SimulatorError::Validation(
                        ErrorCode::EphemeralRelativeCondition,
                    ));
                };
                let Some(created_timestamp) = self.block_timestamps.get(&created_height) else {
                    return Err(SimulatorError::Validation(
                        ErrorCode::EphemeralRelativeCondition,
                    ));
                };

                if self.next_timestamp < created_timestamp + relative_seconds {
                    return Err(SimulatorError::Validation(
                        ErrorCode::AssertSecondsRelativeFailed,
                    ));
                }
            }

            if let Some(relative_height) = spend.before_height_relative {
                let Some(created_height) = coin_state.created_height else {
                    return Err(SimulatorError::Validation(
                        ErrorCode::EphemeralRelativeCondition,
                    ));
                };

                if created_height + relative_height < self.height {
                    return Err(SimulatorError::Validation(
                        ErrorCode::AssertBeforeHeightRelativeFailed,
                    ));
                }
            }

            if let Some(relative_seconds) = spend.before_seconds_relative {
                let Some(created_height) = coin_state.created_height else {
                    return Err(SimulatorError::Validation(
                        ErrorCode::EphemeralRelativeCondition,
                    ));
                };
                let Some(created_timestamp) = self.block_timestamps.get(&created_height) else {
                    return Err(SimulatorError::Validation(
                        ErrorCode::EphemeralRelativeCondition,
                    ));
                };

                if created_timestamp + relative_seconds < self.next_timestamp {
                    return Err(SimulatorError::Validation(
                        ErrorCode::AssertBeforeSecondsRelativeFailed,
                    ));
                }
            }

            removed_coins.insert(spend.coin_id, coin_state);
        }

        // Validate removals.
        for (coin_id, coin_state) in &mut removed_coins {
            let height = self.height;

            if !self.coin_states.contains_key(coin_id) && !added_coins.contains_key(coin_id) {
                return Err(SimulatorError::Validation(ErrorCode::UnknownUnspent));
            }

            if coin_state.spent_height.is_some() {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }

            coin_state.spent_height = Some(height);
        }

        // Update the coin data.
        let mut updates = added_coins.clone();
        updates.extend(removed_coins);
        self.create_block();
        self.coin_states.extend(updates.clone());
        for (hint, coins) in added_hints {
            self.hinted_coins.entry(hint).or_default().extend(coins);
        }
        self.puzzle_and_solutions.extend(puzzle_solutions);

        Ok(updates)
    }

    pub fn lookup_coin_ids(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
        coin_ids
            .iter()
            .filter_map(|coin_id| self.coin_states.get(coin_id).copied())
            .collect()
    }

    pub fn lookup_puzzle_hashes(
        &self,
        puzzle_hashes: IndexSet<Bytes32>,
        include_hints: bool,
    ) -> Vec<CoinState> {
        let mut coin_states = IndexMap::new();

        for (coin_id, coin_state) in &self.coin_states {
            if puzzle_hashes.contains(&coin_state.coin.puzzle_hash) {
                coin_states.insert(*coin_id, self.coin_states[coin_id]);
            }
        }

        if include_hints {
            for puzzle_hash in puzzle_hashes {
                if let Some(hinted_coins) = self.hinted_coins.get(&puzzle_hash) {
                    for coin_id in hinted_coins {
                        coin_states.insert(*coin_id, self.coin_states[coin_id]);
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
        self.rng.fill(&mut header_hash);
        self.header_hashes.push(header_hash.into());
        self.block_timestamps
            .insert(self.height, self.next_timestamp);

        self.height += 1;
        self.next_timestamp += 1;
    }
}
