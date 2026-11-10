use chia_bls::SecretKey;
use chia_consensus::flags::ConsensusFlags;
use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{Bytes32, Coin, CoinSpend, CoinState, Program, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use indexmap::{IndexMap, IndexSet, indexset};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::{
    BlsPair, BlsPairWithCoin, SimulatorError, sign_transaction,
    spend_bundle_validation::{
        CoinRecord, ValidationClock, ValidationSettings, validate_conditions,
        validate_relative_conditions,
    },
};

mod config;
mod data;

pub use config::*;

use data::SimulatorData;

/// An in-memory blockchain for testing spends quickly and deterministically.
///
/// The simulator tracks coin states, hints, and (optionally) coin spends. Coins can be created
/// directly with [`Simulator::new_coin`] or [`Simulator::bls`], without a parent spend. Each call
/// to [`Simulator::new_transaction`] (or [`Simulator::spend_coins`]) validates a spend bundle
/// with testnet11 consensus constants, including the CLVM cost, aggregate signature, and timelock
/// conditions. If it is valid, the result is added to a new block immediately. There is no
/// mempool.
///
/// The chain starts at height 0 with a timestamp of 0. Each new block increments the height by
/// one and the next timestamp by one second. You can move time forward with
/// [`Simulator::pass_time`] and [`Simulator::set_next_timestamp`], and create an empty block with
/// [`Simulator::create_block`].
///
/// For a simulator with a mempool, farming, and reorgs, see
/// [`FullNodeSimulator`](crate::FullNodeSimulator).
///
/// # Example
///
/// ```
/// use chia_protocol::CoinSpend;
/// use chia_sdk_test::{Simulator, to_program, to_puzzle};
/// use chia_sdk_types::conditions::{AssertSecondsRelative, CreateCoin, Memos};
/// use clvmr::NodePtr;
///
/// let mut sim = Simulator::new();
///
/// // The puzzle `1` returns its solution as its conditions.
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
/// let coin = sim.new_coin(puzzle_hash, 1_000);
///
/// // Confirm the coin in a block so that it has a creation timestamp.
/// sim.create_block();
///
/// // This spend can't be confirmed until 60 seconds after the coin was created.
/// let solution = to_program((
///     AssertSecondsRelative::new(60),
///     (CreateCoin::<NodePtr>::new(puzzle_hash, 1_000, Memos::None), ()),
/// ))?;
/// let coin_spend = CoinSpend::new(coin, puzzle_reveal, solution);
///
/// assert!(sim.spend_coins(vec![coin_spend.clone()], &[]).is_err());
///
/// sim.pass_time(60);
/// let updates = sim.spend_coins(vec![coin_spend], &[])?;
///
/// // The updates include the spent coin and the newly created coin.
/// assert_eq!(updates.len(), 2);
/// assert_eq!(sim.coin_state(coin.coin_id()).unwrap().spent_height, Some(1));
/// assert_eq!(sim.children(coin.coin_id()).len(), 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
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
    /// Creates a simulator with the default [`SimulatorConfig`].
    pub fn new() -> Self {
        Self::with_config(SimulatorConfig::default())
    }

    /// Creates a simulator with the given [`SimulatorConfig`].
    ///
    /// The [`SimulatorConfig::seed`] seeds the random number generator used for new coin parent
    /// ids, key pairs from [`Simulator::bls`], and header hashes.
    pub fn with_config(config: SimulatorConfig) -> Self {
        Self {
            config,
            data: SimulatorData::new(ChaCha8Rng::seed_from_u64(config.seed)),
        }
    }

    /// Serializes the simulator's chain state with `bincode`.
    ///
    /// The [`SimulatorConfig`] is not included. Pass it to
    /// [`Simulator::deserialize_with_config`] when restoring if it isn't the default.
    ///
    /// # Errors
    ///
    /// Returns an error if `bincode` fails to encode the state.
    ///
    /// # Example
    ///
    /// ```
    /// use chia_protocol::Bytes32;
    /// use chia_sdk_test::Simulator;
    ///
    /// let mut sim = Simulator::new();
    /// let coin = sim.new_coin(Bytes32::default(), 1);
    ///
    /// let bytes = sim.serialize()?;
    /// let restored = Simulator::deserialize(&bytes)?;
    /// assert_eq!(restored, sim);
    /// assert!(restored.coin_state(coin.coin_id()).is_some());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[cfg(feature = "serde")]
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(&self.data, bincode::config::standard())
    }

    /// Restores a simulator from bytes created by [`Simulator::serialize`], using the given
    /// [`SimulatorConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes aren't a valid `bincode` encoding of the simulator state.
    #[cfg(feature = "serde")]
    pub fn deserialize_with_config(
        data: &[u8],
        config: SimulatorConfig,
    ) -> Result<Self, bincode::error::DecodeError> {
        let data: SimulatorData =
            bincode::serde::decode_from_slice(data, bincode::config::standard())?.0;
        Ok(Self { config, data })
    }

    /// Restores a simulator from bytes created by [`Simulator::serialize`], using the default
    /// [`SimulatorConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes aren't a valid `bincode` encoding of the simulator state.
    #[cfg(feature = "serde")]
    pub fn deserialize(data: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        Self::deserialize_with_config(data, SimulatorConfig::default())
    }

    /// Returns the current height, which is the number of blocks created so far.
    ///
    /// Coins created or spent now are recorded at this height.
    pub fn height(&self) -> u32 {
        self.data.height
    }

    /// Returns the timestamp that the next block will have, in seconds.
    ///
    /// Absolute and relative time conditions are checked against this value.
    pub fn next_timestamp(&self) -> u64 {
        self.data.next_timestamp
    }

    /// Returns the header hash of the most recent block.
    ///
    /// Before any block is created, this is the all-zero genesis hash. Header hashes of later
    /// blocks are random.
    pub fn header_hash(&self) -> Bytes32 {
        self.data.header_hashes.last().copied().unwrap()
    }

    /// Returns the header hash at the given height, or [`None`] if the height is above the
    /// current [`Simulator::height`].
    ///
    /// Height 0 is the all-zero genesis hash, and height `n` is the hash of the `n`th block.
    pub fn header_hash_of(&self, height: u32) -> Option<Bytes32> {
        self.data.header_hashes.get(height as usize).copied()
    }

    /// Adds an unspent coin to the coin set, created at the current height.
    ///
    /// The coin doesn't need a parent spend. If a coin with the same id already exists, it is
    /// replaced with an unspent coin state.
    pub fn insert_coin(&mut self, coin: Coin) {
        let coin_state = CoinState::new(coin, None, Some(self.data.height));
        self.data.coin_states.insert(coin.coin_id(), coin_state);
    }

    /// Creates an unspent coin with a random parent coin id and adds it to the coin set.
    ///
    /// See [`Simulator::insert_coin`].
    pub fn new_coin(&mut self, puzzle_hash: Bytes32, amount: u64) -> Coin {
        let mut parent_coin_info = [0; 32];
        self.data.rng.fill(&mut parent_coin_info);
        let coin = Coin::new(parent_coin_info.into(), puzzle_hash, amount);
        self.insert_coin(coin);
        coin
    }

    /// Creates a random [`BlsPair`] and a coin of `amount` mojos locked to its standard puzzle
    /// hash.
    ///
    /// # Example
    ///
    /// ```
    /// use chia_sdk_test::Simulator;
    ///
    /// let mut sim = Simulator::new();
    /// let alice = sim.bls(1_000);
    ///
    /// assert_eq!(alice.coin.puzzle_hash, alice.puzzle_hash);
    /// assert_eq!(sim.unspent_coins(alice.puzzle_hash, false), vec![alice.coin]);
    /// ```
    pub fn bls(&mut self, amount: u64) -> BlsPairWithCoin {
        let pair = BlsPair::new(self.data.rng.random());
        let coin = self.new_coin(pair.puzzle_hash, amount);
        BlsPairWithCoin::new(pair, coin)
    }

    /// Sets the timestamp of the next block, in seconds.
    ///
    /// # Errors
    ///
    /// Returns [`SimulatorError::Validation`] with [`ErrorCode::TimestampTooFarInPast`] if `time`
    /// is earlier than the timestamp of the most recent block.
    pub fn set_next_timestamp(&mut self, time: u64) -> Result<(), SimulatorError> {
        if self.data.height > 0
            && let Some(last_block_timestamp) =
                self.data.block_timestamps.get(&(self.data.height - 1))
            && time < *last_block_timestamp
        {
            return Err(SimulatorError::Validation(ErrorCode::TimestampTooFarInPast));
        }
        self.data.next_timestamp = time;

        Ok(())
    }

    /// Moves the timestamp of the next block forward by `time` seconds.
    pub fn pass_time(&mut self, time: u64) {
        self.data.next_timestamp += time;
    }

    /// Hints the coin with id `coin_id` with `hint`, as if it had been created with that memo.
    ///
    /// This is done even if [`SimulatorConfig::save_hints`] is `false`. The coin should already be
    /// in the coin set, since [`Simulator::lookup_puzzle_hashes`] expects every hinted coin to
    /// exist.
    pub fn hint_coin(&mut self, coin_id: Bytes32, hint: Bytes32) {
        self.data
            .hinted_coins
            .entry(hint)
            .or_default()
            .insert(coin_id);
    }

    /// Returns the current state of a coin, or [`None`] if the coin doesn't exist.
    pub fn coin_state(&self, coin_id: Bytes32) -> Option<CoinState> {
        self.data.coin_states.get(&coin_id).copied()
    }

    /// Returns the states of all coins whose parent coin id is `coin_id`.
    pub fn children(&self, coin_id: Bytes32) -> Vec<CoinState> {
        self.data
            .coin_states
            .values()
            .filter(move |cs| cs.coin.parent_coin_info == coin_id)
            .copied()
            .collect()
    }

    /// Returns the ids of all coins that have been hinted with `hint`.
    pub fn hinted_coins(&self, hint: Bytes32) -> Vec<Bytes32> {
        self.data
            .hinted_coins
            .get(&hint)
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    /// Returns the puzzle reveal used to spend a coin, or [`None`] if it wasn't saved.
    ///
    /// Spends are only saved if [`SimulatorConfig::save_spends`] is `true`.
    pub fn puzzle_reveal(&self, coin_id: Bytes32) -> Option<Program> {
        self.data
            .coin_spends
            .get(&coin_id)
            .map(|spend| spend.puzzle_reveal.clone())
    }

    /// Returns the solution used to spend a coin, or [`None`] if it wasn't saved.
    ///
    /// Spends are only saved if [`SimulatorConfig::save_spends`] is `true`.
    pub fn solution(&self, coin_id: Bytes32) -> Option<Program> {
        self.data
            .coin_spends
            .get(&coin_id)
            .map(|spend| spend.solution.clone())
    }

    /// Returns the puzzle reveal and solution used to spend a coin, or [`None`] if they weren't
    /// saved.
    ///
    /// Spends are only saved if [`SimulatorConfig::save_spends`] is `true`.
    pub fn puzzle_and_solution(&self, coin_id: Bytes32) -> Option<(Program, Program)> {
        self.data
            .coin_spends
            .get(&coin_id)
            .map(|spend| (spend.puzzle_reveal.clone(), spend.solution.clone()))
    }

    /// Returns the [`CoinSpend`] used to spend a coin, or [`None`] if it wasn't saved.
    ///
    /// Spends are only saved if [`SimulatorConfig::save_spends`] is `true`.
    pub fn coin_spend(&self, coin_id: Bytes32) -> Option<CoinSpend> {
        self.data.coin_spends.get(&coin_id).cloned()
    }

    /// Return every coin spend that resolved at `height`.
    ///
    /// Iterates `data.coin_states` filtered by `spent_height == Some(height)`,
    /// joins each `coin_id` against `data.coin_spends`, and returns the joined
    /// `CoinSpend`s in iteration order.
    ///
    /// Returns an empty vector when no spends resolved at `height`.
    #[must_use]
    pub fn block_spends(&self, height: u32) -> Vec<CoinSpend> {
        self.data
            .coin_states
            .values()
            .filter(|cs| cs.spent_height == Some(height))
            .filter_map(|cs| self.data.coin_spends.get(&cs.coin.coin_id()).cloned())
            .collect()
    }

    /// Return every coin created at `height`.
    ///
    /// Iterates `data.coin_states` filtered by `created_height == Some(height)`
    /// and returns the underlying `Coin` for each (drops the `CoinState`
    /// envelope — callers that need spent-height information should use
    /// `coin_state(coin_id)` directly).
    ///
    /// Returns an empty vector when no coins were created at `height`.
    #[must_use]
    pub fn block_outputs(&self, height: u32) -> Vec<Coin> {
        self.data
            .coin_states
            .values()
            .filter(|cs| cs.created_height == Some(height))
            .map(|cs| cs.coin)
            .collect()
    }

    /// Signs the coin spends with the given secret keys, then processes them as a spend bundle
    /// with [`Simulator::new_transaction`].
    ///
    /// Signatures are created with [`sign_transaction`], so only BLS signatures are supported.
    /// Pass an empty slice if no signatures are required.
    ///
    /// # Errors
    ///
    /// Returns [`SimulatorError::MissingKey`] if a required BLS signature uses a public key that
    /// doesn't match any of `secret_keys`, [`SimulatorError::Signer`] if the required signatures
    /// can't be calculated, or any error from [`Simulator::new_transaction`].
    pub fn spend_coins(
        &mut self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: &[SecretKey],
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        let signature = sign_transaction(&coin_spends, secret_keys)?;
        self.new_transaction(SpendBundle::new(coin_spends, signature))
    }

    /// Validates a spend bundle and, if it's valid, adds its result to a new block.
    ///
    /// The bundle is validated with testnet11 consensus constants against the current height and
    /// [`Simulator::next_timestamp`]. This checks the CLVM, cost, aggregate signature, puzzle
    /// hashes, and timelock conditions. Every spent coin must exist and be unspent, unless it is
    /// created in the same bundle. `RESERVE_FEE` conditions are not checked.
    ///
    /// Returns the new state of every coin created or spent by the bundle, keyed by coin id.
    /// Created coins are hinted if [`SimulatorConfig::save_hints`] is `true`, and the coin spends
    /// are saved if [`SimulatorConfig::save_spends`] is `true`.
    ///
    /// # Errors
    ///
    /// Returns [`SimulatorError::Validation`] with the consensus [`ErrorCode`] if the bundle is
    /// invalid. In that case the simulator is not changed.
    pub fn new_transaction(
        &mut self,
        spend_bundle: SpendBundle,
    ) -> Result<IndexMap<Bytes32, CoinState>, SimulatorError> {
        let clock = ValidationClock {
            height: self.data.height,
            timestamp: self.data.next_timestamp,
        };
        let validation = validate_conditions(
            &spend_bundle,
            ValidationSettings {
                constants: &TESTNET11_CONSTANTS,
                max_cost: 11_000_000_000 / 2,
                flags: ConsensusFlags::ENABLE_KECCAK_OPS_OUTSIDE_GUARD,
                clock,
            },
        )
        .map_err(SimulatorError::Validation)?;
        let conds = validation.conditions;

        let mut removed_coins = IndexMap::new();
        let mut added_coins = IndexMap::new();
        let mut added_hints = IndexMap::new();
        let mut coin_spends = IndexMap::new();

        for coin_spend in spend_bundle.coin_spends {
            coin_spends.insert(coin_spend.coin.coin_id(), coin_spend);
        }

        // Calculate additions and removals.
        for (spend, parsed) in conds.spends.iter().zip(validation.additions) {
            debug_assert_eq!(parsed.coin_id, spend.coin_id);
            for addition in parsed.additions {
                let coin = addition.coin;
                added_coins.insert(
                    coin.coin_id(),
                    CoinState::new(coin, None, Some(self.data.height)),
                );

                let Some(hint) = addition.hint else {
                    continue;
                };

                added_hints
                    .entry(hint)
                    .or_insert_with(IndexSet::new)
                    .insert(coin.coin_id());
            }

            let coin = Coin::new(spend.parent_id, spend.puzzle_hash, spend.coin_amount);

            let coin_state = self
                .data
                .coin_states
                .get(&spend.coin_id)
                .copied()
                .unwrap_or(CoinState::new(coin, None, Some(self.data.height)));

            let created_timestamp = coin_state
                .created_height
                .and_then(|height| self.data.block_timestamps.get(&height).copied());
            validate_relative_conditions(
                spend,
                CoinRecord {
                    created_height: coin_state.created_height,
                    created_timestamp,
                },
                clock,
            )
            .map_err(SimulatorError::Validation)?;

            removed_coins.insert(spend.coin_id, coin_state);
        }

        // Validate removals.
        for (coin_id, coin_state) in &mut removed_coins {
            let height = self.data.height;

            if !self.data.coin_states.contains_key(coin_id) && !added_coins.contains_key(coin_id) {
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

    /// Returns the states of the given coins. Coins that don't exist are skipped.
    pub fn lookup_coin_ids(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
        coin_ids
            .iter()
            .filter_map(|coin_id| self.data.coin_states.get(coin_id).copied())
            .collect()
    }

    /// Returns the states of all coins (spent and unspent) with one of the given puzzle hashes.
    ///
    /// If `include_hints` is `true`, coins hinted with one of the puzzle hashes are included too.
    ///
    /// # Panics
    ///
    /// Panics if `include_hints` is `true` and a coin that was hinted with
    /// [`Simulator::hint_coin`] doesn't exist.
    pub fn lookup_puzzle_hashes(
        &self,
        puzzle_hashes: IndexSet<Bytes32>,
        include_hints: bool,
    ) -> Vec<CoinState> {
        let mut coin_states = IndexMap::new();

        for (coin_id, coin_state) in &self.data.coin_states {
            if puzzle_hashes.contains(&coin_state.coin.puzzle_hash) {
                coin_states.insert(*coin_id, self.data.coin_states[coin_id]);
            }
        }

        if include_hints {
            for puzzle_hash in puzzle_hashes {
                if let Some(hinted_coins) = self.data.hinted_coins.get(&puzzle_hash) {
                    for coin_id in hinted_coins {
                        coin_states.insert(*coin_id, self.data.coin_states[coin_id]);
                    }
                }
            }
        }

        coin_states.into_values().collect()
    }

    /// Returns all unspent coins with the given puzzle hash.
    ///
    /// If `include_hints` is `true`, unspent coins hinted with the puzzle hash are included too.
    pub fn unspent_coins(&self, puzzle_hash: Bytes32, include_hints: bool) -> Vec<Coin> {
        self.lookup_puzzle_hashes(indexset![puzzle_hash], include_hints)
            .iter()
            .filter(|cs| cs.spent_height.is_none())
            .map(|cs| cs.coin)
            .collect()
    }

    /// Creates an empty block at the current height and [`Simulator::next_timestamp`].
    ///
    /// This increments the height by one and the next timestamp by one second.
    pub fn create_block(&mut self) {
        let mut header_hash = [0; 32];
        self.data.rng.fill(&mut header_hash);
        self.data.header_hashes.push(header_hash.into());
        self.data
            .block_timestamps
            .insert(self.data.height, self.data.next_timestamp);

        self.data.height += 1;
        self.data.next_timestamp += 1;
    }
}

#[cfg(test)]
mod block_accessor_tests {
    use super::*;

    #[test]
    fn block_spends_empty_for_unused_height() {
        let sim = Simulator::new();
        assert!(sim.block_spends(0).is_empty());
        assert!(sim.block_spends(1).is_empty());
    }

    #[test]
    fn block_outputs_empty_for_unused_height() {
        let sim = Simulator::new();
        assert!(sim.block_outputs(0).is_empty());
        assert!(sim.block_outputs(1).is_empty());
    }
}
