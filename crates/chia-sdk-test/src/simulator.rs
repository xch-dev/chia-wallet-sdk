use std::collections::HashSet;

use chia_bls::{DerivableKey, PublicKey, SecretKey};
use chia_consensus::{
    consensus_constants::ConsensusConstants, gen::validation_error::ErrorCode,
    spendbundle_validation::validate_clvm_and_signature,
};
use chia_protocol::{Bytes32, Coin, CoinSpend, CoinState, Program, SpendBundle};
use chia_puzzles::standard::StandardArgs;
use chia_sdk_types::TESTNET11_CONSTANTS;
use fastrand::Rng;
use indexmap::{IndexMap, IndexSet};

use crate::{sign_transaction, test_secret_key, SimulatorError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Simulator {
    rng: Rng,
    height: u32,
    header_hashes: Vec<Bytes32>,
    coin_states: IndexMap<Bytes32, CoinState>,
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
        let mut rng = Rng::with_seed(seed);
        let mut header_hash = [0; 32];
        rng.fill(&mut header_hash);

        Self {
            rng,
            height: 0,
            header_hashes: vec![header_hash.into()],
            coin_states: IndexMap::new(),
            hinted_coins: IndexMap::new(),
            puzzle_and_solutions: IndexMap::new(),
        }
    }

    pub fn height(&self) -> u32 {
        self.height
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

    pub fn new_p2(
        &mut self,
        amount: u64,
    ) -> Result<(SecretKey, PublicKey, Bytes32, Coin), bip39::Error> {
        let sk = test_secret_key()?;
        let pk = sk.public_key();
        let p2 = StandardArgs::curry_tree_hash(pk).into();
        let coin = self.new_coin(p2, amount);
        Ok((sk, pk, p2, coin))
    }

    pub fn child_p2(
        &mut self,
        amount: u64,
        child: u32,
    ) -> Result<(SecretKey, PublicKey, Bytes32, Coin), bip39::Error> {
        let sk = test_secret_key()?.derive_unhardened(child);
        let pk = sk.public_key();
        let p2 = StandardArgs::curry_tree_hash(pk).into();
        let coin = self.new_coin(p2, amount);
        Ok((sk, pk, p2, coin))
    }

    pub(crate) fn hint_coin(&mut self, coin_id: Bytes32, hint: Bytes32) {
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

    pub fn spend_coins(
        &mut self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: &[SecretKey],
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        let signature =
            sign_transaction(&coin_spends, secret_keys, &(&*TESTNET11_CONSTANTS).into())?;
        self.new_transaction(
            SpendBundle::new(coin_spends, signature),
            &TESTNET11_CONSTANTS,
        )
    }

    /// Processes a spend bunndle and returns the updated coin states.
    pub fn new_transaction(
        &mut self,
        spend_bundle: SpendBundle,
        constants: &ConsensusConstants,
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        if spend_bundle.coin_spends.is_empty() {
            return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
        }

        // TODO: Fix cost
        let (conds, _pairings, _duration) =
            validate_clvm_and_signature(&spend_bundle, 7_700_000_000, constants, self.height)
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
        self.hinted_coins.extend(added_hints.clone());
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

    fn create_block(&mut self) {
        let mut header_hash = [0; 32];
        self.rng.fill(&mut header_hash);
        self.header_hashes.push(header_hash.into());
        self.height += 1;
    }
}
