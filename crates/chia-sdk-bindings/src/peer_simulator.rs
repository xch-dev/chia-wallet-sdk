use std::sync::Arc;

use bindy::Result;
use chia_protocol::{Bytes32, Coin, CoinState};

use crate::{BlsPairWithCoin, Peer};

/// A local wallet protocol server backed by an in-memory simulator, for testing [`Peer`].
#[derive(Clone)]
pub struct PeerSimulator(Arc<chia_sdk_test::PeerSimulator>);

impl PeerSimulator {
    /// Starts a peer simulator listening on an ephemeral port on `127.0.0.1`.
    pub async fn start() -> Result<Self> {
        let simulator = chia_sdk_test::PeerSimulator::new().await?;
        Ok(Self(Arc::new(simulator)))
    }

    /// Connects a new [`Peer`] to the simulator, after the initial `NewPeakWallet` message.
    pub async fn connect(&self) -> Result<Peer> {
        let (peer, receiver) = self.0.connect_split().await?;
        Ok(Peer::from_parts(peer, receiver))
    }

    /// The address that the simulator is listening on, such as `127.0.0.1:12345`.
    pub fn socket_addr(&self) -> Result<String> {
        Ok(self.0.socket_addr().to_string())
    }

    /// The header hash to use for coin and puzzle state requests without a previous height.
    pub fn genesis_challenge(&self) -> Result<Bytes32> {
        Ok(self.0.config().genesis_challenge)
    }

    /// The height of the latest block.
    pub async fn height(&self) -> Result<u32> {
        Ok(self.0.lock().await.height())
    }

    /// The header hash of the latest block.
    pub async fn header_hash(&self) -> Result<Bytes32> {
        Ok(self.0.lock().await.header_hash())
    }

    /// Creates an unspent coin with the given puzzle hash and amount.
    pub async fn new_coin(&self, puzzle_hash: Bytes32, amount: u64) -> Result<Coin> {
        Ok(self.0.lock().await.new_coin(puzzle_hash, amount))
    }

    /// Creates a key pair and an unspent standard coin of the given amount owned by it.
    pub async fn bls(&self, amount: u64) -> Result<BlsPairWithCoin> {
        Ok(self.0.lock().await.bls(amount).into())
    }

    /// The state of a coin, if it exists.
    pub async fn coin_state(&self, coin_id: Bytes32) -> Result<Option<CoinState>> {
        Ok(self.0.lock().await.coin_state(coin_id))
    }
}
