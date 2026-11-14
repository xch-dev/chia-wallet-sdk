use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use bindy::Result;
use chia_protocol::{BlockRecord, Bytes32, Coin, CoinState};
use chia_sdk_client::Capability;
use chia_sdk_test::{BlsPair, FullNodeSimulator, Simulator};

use crate::{BlsPairWithCoin, Peer};

type SdkPeerSimulator<S> = chia_sdk_test::PeerSimulator<S>;

/// A local wallet protocol server backed by an in-memory simulator, for testing [`Peer`].
#[derive(Clone)]
pub struct PeerSimulator(Arc<Mode>);

enum Mode {
    /// Each transaction is included in a new block as soon as it's sent.
    Instant(SdkPeerSimulator<Simulator>),
    /// Transactions wait in a mempool until a block is farmed. `next_seed` seeds the key pairs
    /// made by [`PeerSimulator::bls`], since [`FullNodeSimulator`] has no random key helper.
    Mempool {
        simulator: SdkPeerSimulator<FullNodeSimulator>,
        next_seed: AtomicU64,
    },
}

/// Runs the same expression against whichever simulator the mode holds.
macro_rules! with_simulator {
    ($mode:expr, $sim:ident => $body:expr) => {
        match $mode {
            Mode::Instant($sim) => $body,
            Mode::Mempool {
                simulator: $sim, ..
            } => $body,
        }
    };
}

impl PeerSimulator {
    /// Starts a peer simulator listening on an ephemeral port on `127.0.0.1`. Each transaction
    /// is included in a new block as soon as it's sent, and there is no mempool.
    pub async fn start() -> Result<Self> {
        let simulator = SdkPeerSimulator::new().await?;
        Ok(Self(Arc::new(Mode::Instant(simulator))))
    }

    /// Starts a peer simulator with a mempool. Transactions wait in the mempool until
    /// [`PeerSimulator::farm_block`] is called, `RequestTransaction` is answered from the
    /// mempool, and peers connected with [`PeerSimulator::connect_with_mempool_updates`]
    /// receive the CHIP-26 mempool updates. The chain starts at height 1.
    pub async fn start_with_mempool() -> Result<Self> {
        let simulator = SdkPeerSimulator::with_mempool().await?;
        Ok(Self(Arc::new(Mode::Mempool {
            simulator,
            next_seed: AtomicU64::new(0),
        })))
    }

    /// Connects a new [`Peer`] to the simulator, after the initial `NewPeakWallet` message.
    /// It doesn't receive mempool updates.
    pub async fn connect(&self) -> Result<Peer> {
        let (peer, receiver) = with_simulator!(&*self.0, sim => sim.connect_split().await?);
        Ok(Peer::from_parts(peer, receiver))
    }

    /// Connects a new [`Peer`] that advertises the CHIP-26 mempool updates capability, after
    /// the initial `NewPeakWallet` message. In mempool mode, its events include
    /// `mempool_items_added` and `mempool_items_removed` for transactions that match its
    /// subscriptions.
    pub async fn connect_with_mempool_updates(&self) -> Result<Peer> {
        let capabilities = vec![(u16::from(Capability::MempoolUpdates), "1".to_string())];
        let (peer, receiver) = with_simulator!(
            &*self.0,
            sim => sim.connect_split_with_capabilities(capabilities).await?
        );
        Ok(Peer::from_parts(peer, receiver))
    }

    /// The address that the simulator is listening on, such as `127.0.0.1:12345`.
    pub fn socket_addr(&self) -> Result<String> {
        Ok(with_simulator!(&*self.0, sim => sim.socket_addr()).to_string())
    }

    /// The header hash to use for coin and puzzle state requests without a previous height.
    pub fn genesis_challenge(&self) -> Result<Bytes32> {
        Ok(with_simulator!(&*self.0, sim => sim.config().genesis_challenge))
    }

    /// Farms `blocks` blocks (at least one) from the mempool, and sends the resulting messages
    /// to connected peers. Only available in mempool mode.
    pub async fn farm_block(&self, blocks: u32) -> Result<Vec<BlockRecord>> {
        match &*self.0 {
            Mode::Instant(_) => Err(bindy::Error::Custom(
                "blocks can only be farmed in mempool mode".to_string(),
            )),
            Mode::Mempool { simulator, .. } => Ok(simulator.farm_block(blocks).await),
        }
    }

    /// The height of the latest block.
    pub async fn height(&self) -> Result<u32> {
        Ok(with_simulator!(&*self.0, sim => sim.lock().await.height()))
    }

    /// The header hash of the latest block.
    pub async fn header_hash(&self) -> Result<Bytes32> {
        Ok(with_simulator!(&*self.0, sim => sim.lock().await.header_hash()))
    }

    /// Creates an unspent coin with the given puzzle hash and amount.
    ///
    /// In mempool mode, peers are told about it with the next block, transaction or
    /// subscription.
    pub async fn new_coin(&self, puzzle_hash: Bytes32, amount: u64) -> Result<Coin> {
        Ok(with_simulator!(&*self.0, sim => sim.lock().await.new_coin(puzzle_hash, amount)))
    }

    /// Creates a key pair and an unspent standard coin of the given amount owned by it.
    pub async fn bls(&self, amount: u64) -> Result<BlsPairWithCoin> {
        let pair = match &*self.0 {
            Mode::Instant(simulator) => simulator.lock().await.bls(amount),
            Mode::Mempool {
                simulator,
                next_seed,
            } => {
                let pair = BlsPair::new(next_seed.fetch_add(1, Ordering::Relaxed));
                let coin = simulator.lock().await.new_coin(pair.puzzle_hash, amount);
                chia_sdk_test::BlsPairWithCoin::new(pair, coin)
            }
        };
        Ok(pair.into())
    }

    /// The state of a coin, if it exists.
    pub async fn coin_state(&self, coin_id: Bytes32) -> Result<Option<CoinState>> {
        Ok(match &*self.0 {
            Mode::Instant(simulator) => simulator.lock().await.coin_state(coin_id),
            Mode::Mempool { simulator, .. } => simulator
                .lock()
                .await
                .get_coin_record_by_name(coin_id)
                .coin_record
                .map(|record| {
                    CoinState::new(
                        record.coin,
                        record.spent.then_some(record.spent_block_index),
                        Some(record.confirmed_block_index),
                    )
                }),
        })
    }
}
