use std::time::{SystemTime, UNIX_EPOCH};

use bip39::Mnemonic;
use chia_bls::{SecretKey, master_to_wallet_hardened};
use chia_protocol::{BlockRecord, Bytes32, ClassgroupElement, Coin};
use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};
use chia_sha2::Sha256;
use hex_literal::hex;
use indexmap::IndexMap;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

mod chain;
mod fast_forward;
mod peer_events;
mod push_tx;
mod queries;
mod state;
#[cfg(feature = "serde")]
mod state_dump;
mod types;
mod validation;

#[cfg(test)]
mod tests;

use peer_events::PeerEventLog;
#[cfg(feature = "peer-simulator")]
pub(crate) use peer_events::{MempoolItemIds, PeerEvent};
use state::ChainState;
use types::{SimBlock, SimCoinRecord, ValidatedBundle, ValidatedSpend};

pub use types::{FullNodeSimulatorEvent, FullNodeSimulatorPushTxResponse};

const BLOCK_REWARD_AMOUNT: u64 = 2_000_000_000_000;
const PREFARM_WALLET_INDEX: u32 = 1;
const SIMULATOR_GENESIS_CHALLENGE: Bytes32 = Bytes32::new(hex!(
    "eb8c4d20b322be8d9fddbf9412016bdffe9a2901d7edb0e364e94266d0e095f7"
));

/// An in-memory full node with a mempool, block farming, reverts, and reorgs.
///
/// Unlike the [`Simulator`](crate::Simulator), transactions submitted with
/// [`FullNodeSimulator::push_tx`] wait in a mempool until [`FullNodeSimulator::farm_block`] is
/// called. The query methods return the same response types as the full node RPC in
/// `chia-sdk-coinset`, so this is a good fit for testing code that talks to a full node. With
/// the `full-node-simulator-http` feature, it can also be served over HTTP.
///
/// The chain starts with a genesis block at height 1 whose timestamp is the current system time.
/// It contains two prefarm coins, worth 21 million XCH in total, locked to
/// [`FullNodeSimulator::get_prefarm_puzzle_hash`]. Every farmed block adds a 2 XCH reward coin
/// for [`FullNodeSimulator::get_farming_ph`] and advances the timestamp by one second.
///
/// Spends are validated with the simulator's genesis challenge as the `AGG_SIG_ME` additional
/// data (see [`FullNodeSimulator::get_aggsig_additional_data`]). This differs from the testnet11
/// value used by [`sign_transaction`](crate::sign_transaction), so BLS signatures must be made
/// against this value instead.
///
/// # Example
///
/// ```
/// use chia_bls::Signature;
/// use chia_protocol::{CoinSpend, SpendBundle};
/// use chia_sdk_test::{FullNodeSimulator, to_program, to_puzzle};
/// use chia_sdk_types::conditions::{CreateCoin, Memos};
/// use clvmr::NodePtr;
///
/// let mut sim = FullNodeSimulator::new();
/// assert_eq!(sim.height(), 1);
///
/// // The puzzle `1` returns its solution as its conditions, so no signature is needed.
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
/// let coin = sim.new_coin(puzzle_hash, 1_000);
///
/// let solution = to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 900, Memos::None)])?;
/// let spend_bundle = SpendBundle::new(
///     vec![CoinSpend::new(coin, puzzle_reveal, solution)],
///     Signature::default(),
/// );
///
/// // The transaction waits in the mempool until a block is farmed.
/// assert!(sim.push_tx(spend_bundle).success);
/// let record = sim.get_coin_record_by_name(coin.coin_id()).coin_record.unwrap();
/// assert!(!record.spent);
///
/// sim.farm_block(1);
/// let record = sim.get_coin_record_by_name(coin.coin_id()).coin_record.unwrap();
/// assert!(record.spent);
/// assert_eq!(record.spent_block_index, sim.height());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct FullNodeSimulator {
    rng: ChaCha8Rng,
    state: ChainState,
    orphaned_blocks: IndexMap<Bytes32, SimBlock>,
    mempool: IndexMap<Bytes32, ValidatedBundle>,
    farming_puzzle_hash: Bytes32,
    master_secret_key: SecretKey,
    prefarm_puzzle_hash: Bytes32,
    node_id: Bytes32,
    genesis_challenge: Bytes32,
    events: Vec<FullNodeSimulatorEvent>,
    peer_events: PeerEventLog,
}

impl Default for FullNodeSimulator {
    fn default() -> Self {
        Self::with_seed(1337)
    }
}

impl FullNodeSimulator {
    /// Creates a simulator with the default seed of 1337.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a simulator whose master secret key and random number generator are derived from
    /// `seed`.
    ///
    /// The genesis timestamp still comes from the system clock.
    pub fn with_seed(seed: u64) -> Self {
        Self::with_secret_key_and_rng(
            Self::secret_key_from_seed(seed),
            ChaCha8Rng::seed_from_u64(seed),
        )
    }

    /// Creates a simulator with the given master secret key.
    ///
    /// The prefarm coins are locked to the standard puzzle of the synthetic key for the hardened
    /// wallet key at index 1. The random number generator is seeded with the secret key bytes.
    pub fn with_secret_key(root_secret_key: SecretKey) -> Self {
        let mut seed = [0; 32];
        seed.copy_from_slice(&root_secret_key.to_bytes());
        Self::with_secret_key_and_rng(root_secret_key, ChaCha8Rng::from_seed(seed))
    }

    fn with_secret_key_and_rng(root_secret_key: SecretKey, mut rng: ChaCha8Rng) -> Self {
        let prefarm_secret_key =
            master_to_wallet_hardened(&root_secret_key, PREFARM_WALLET_INDEX).derive_synthetic();
        let prefarm_puzzle_hash =
            StandardArgs::curry_tree_hash(prefarm_secret_key.public_key()).into();
        let mut node_id = [0; 32];
        rng.fill(&mut node_id);

        let genesis_height = 1;
        let genesis_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let genesis_hash = Bytes32::default();
        let prefarm_coins = vec![
            Self::reward_coin(
                genesis_hash,
                genesis_height,
                0,
                prefarm_puzzle_hash,
                18_375_000_000_000_000_000,
            ),
            Self::reward_coin(
                genesis_hash,
                genesis_height,
                1,
                prefarm_puzzle_hash,
                2_625_000_000_000_000_000,
            ),
        ];
        let genesis_record = Self::make_block_record(
            genesis_hash,
            Bytes32::default(),
            genesis_height,
            genesis_timestamp,
            Bytes32::default(),
            0,
            0,
            prefarm_puzzle_hash,
            prefarm_coins.clone(),
        );
        let additions = prefarm_coins.iter().map(Coin::coin_id).collect::<Vec<_>>();
        let mut coins = IndexMap::new();
        for coin in prefarm_coins {
            coins.insert(
                coin.coin_id(),
                SimCoinRecord {
                    coin,
                    coinbase: true,
                    confirmed_block_index: genesis_height,
                    spent_block_index: None,
                    timestamp: genesis_timestamp,
                },
            );
        }
        let mut blocks = IndexMap::new();
        blocks.insert(
            genesis_hash,
            SimBlock {
                record: genesis_record,
                additions: additions.clone(),
                removals: Vec::new(),
                spends: Vec::new(),
                transactions: Vec::new(),
                delta: state::BlockDelta {
                    coins: additions
                        .iter()
                        .map(|coin_id| state::CoinChange {
                            coin_id: *coin_id,
                            before: None,
                            after: coins.get(coin_id).copied(),
                        })
                        .collect(),
                    ..state::BlockDelta::default()
                },
            },
        );

        Self {
            rng,
            state: ChainState::new(
                genesis_height,
                genesis_timestamp.saturating_add(1),
                vec![genesis_hash],
                blocks,
                coins,
                IndexMap::new(),
                IndexMap::new(),
            ),
            orphaned_blocks: IndexMap::new(),
            mempool: IndexMap::new(),
            farming_puzzle_hash: prefarm_puzzle_hash,
            master_secret_key: root_secret_key,
            prefarm_puzzle_hash,
            node_id: node_id.into(),
            genesis_challenge: SIMULATOR_GENESIS_CHALLENGE,
            events: Vec::new(),
            peer_events: PeerEventLog::default(),
        }
    }

    /// Sets the genesis challenge, which is used as the `AGG_SIG_ME` additional data when
    /// validating spends and is reported by [`FullNodeSimulator::get_network_info`].
    ///
    /// This lets the simulator accept spends signed for another network. For example, setting it
    /// to the testnet11 genesis challenge makes it accept spends signed with
    /// [`sign_transaction`](crate::sign_transaction). Transactions already in the mempool are
    /// revalidated when the next block is farmed, so any whose signatures no longer match are
    /// dropped then.
    pub fn set_genesis_challenge(&mut self, genesis_challenge: Bytes32) {
        self.genesis_challenge = genesis_challenge;
    }

    /// Adds an unspent coin to the coin set, confirmed at the current height and timestamp.
    ///
    /// The coin isn't part of any block, so it isn't reverted by
    /// [`FullNodeSimulator::revert_blocks`]. With the `serde` feature,
    /// `FullNodeSimulator::dump_state` leaves it out and fails if a block spends it.
    pub fn insert_coin(&mut self, coin: Coin) {
        self.insert_coin_record(coin, false, self.state.height, self.state.next_timestamp);
    }

    /// Creates an unspent coin with a random parent coin id and adds it to the coin set.
    ///
    /// See [`FullNodeSimulator::insert_coin`].
    pub fn new_coin(&mut self, puzzle_hash: Bytes32, amount: u64) -> Coin {
        let mut parent_coin_info = [0; 32];
        self.rng.fill(&mut parent_coin_info);
        let coin = Coin::new(parent_coin_info.into(), puzzle_hash, amount);
        self.insert_coin(coin);
        coin
    }

    fn insert_coin_record(&mut self, coin: Coin, coinbase: bool, height: u32, timestamp: u64) {
        self.state.insert_manual_coin(
            coin.coin_id(),
            SimCoinRecord {
                coin,
                coinbase,
                confirmed_block_index: height,
                spent_block_index: None,
                timestamp,
            },
        );
    }

    fn secret_key_from_seed(seed: u64) -> SecretKey {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let entropy: [u8; 32] = rng.random();
        let mnemonic = Mnemonic::from_entropy(&entropy).expect("32 bytes is valid BIP39 entropy");
        SecretKey::from_seed(&mnemonic.to_seed(""))
    }

    fn reward_coin(
        header_hash: Bytes32,
        height: u32,
        index: u8,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> Coin {
        Coin::new(
            Self::reward_parent_id(header_hash, height, index),
            puzzle_hash,
            amount,
        )
    }

    fn reward_parent_id(header_hash: Bytes32, height: u32, index: u8) -> Bytes32 {
        let mut hasher = Sha256::new();
        hasher.update(b"chia-sdk-full-node-simulator-reward");
        hasher.update(header_hash.to_bytes());
        hasher.update(height.to_be_bytes());
        hasher.update([index]);
        hasher.finalize().into()
    }

    #[allow(clippy::too_many_arguments)]
    fn make_block_record(
        header_hash: Bytes32,
        prev_hash: Bytes32,
        height: u32,
        timestamp: u64,
        prev_transaction_block_hash: Bytes32,
        fees: u64,
        prev_transaction_block_height: u32,
        farming_puzzle_hash: Bytes32,
        reward_claims_incorporated: Vec<Coin>,
    ) -> BlockRecord {
        BlockRecord::new(
            header_hash,
            prev_hash,
            height,
            u128::from(height),
            u128::from(height),
            0,
            ClassgroupElement::default(),
            None,
            header_hash,
            header_hash,
            1,
            farming_puzzle_hash,
            farming_puzzle_hash,
            0,
            15,
            false,
            prev_transaction_block_height,
            Some(timestamp),
            Some(prev_transaction_block_hash),
            Some(fees),
            Some(reward_claims_incorporated),
            None,
            None,
            None,
            None,
        )
    }
}
