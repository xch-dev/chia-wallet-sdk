use std::{net::SocketAddr, ops::Deref, sync::Arc};

use chia_protocol::{BlockRecord, Handshake, Message, NodeType, ProtocolMessageTypes};
use chia_sdk_client::{ClientError, Peer, PeerOptions};
use peer_map::PeerMap;
use subscriptions::Subscriptions;
use tokio::{
    net::TcpListener,
    sync::{Mutex, mpsc},
    task::JoinHandle,
};
use tokio_tungstenite::connect_async;
use ws_connection::ws_connection;

use crate::{FullNodeSimulator, Simulator};

mod backend;
mod config;
mod error;
mod notify;
mod peer_map;
mod subscriptions;
mod ws_connection;

#[cfg(test)]
mod mempool_tests;

pub use config::*;
pub use error::*;

mod sealed {
    pub trait Sealed {}

    impl Sealed for crate::Simulator {}
    impl Sealed for crate::FullNodeSimulator {}
}

/// A simulator that a [`PeerSimulator`] can serve over the wallet peer protocol.
///
/// This is implemented for [`Simulator`], where every transaction is included in a new block as
/// soon as it's sent, and for [`FullNodeSimulator`], where transactions wait in a mempool until a
/// block is farmed. It is sealed, so it can't be implemented outside of this crate.
pub trait PeerSimulatorBackend: sealed::Sealed + backend::Backend {}

impl PeerSimulatorBackend for Simulator {}
impl PeerSimulatorBackend for FullNodeSimulator {}

/// A local websocket server that exposes a simulator through the Chia wallet peer protocol.
///
/// This lets you test code that uses a `chia-sdk-client` [`Peer`]. The server listens on an
/// ephemeral port on `127.0.0.1` and runs on a Tokio task, so it must be created inside a Tokio
/// runtime. It stops when this value is dropped. Connections use plain websockets without TLS.
///
/// The simulator supports the wallet handshake, sending transactions, coin and puzzle state
/// requests and subscriptions, puzzle solution and children requests, and `RequestTransaction`.
/// Connected peers receive a `NewPeakWallet` message for each new block, and subscribers receive
/// coin state updates.
///
/// There are two modes:
///
/// - [`PeerSimulator::new`] serves a [`Simulator`]. Each accepted transaction is included in a new
///   block right away, and there is no mempool.
/// - [`PeerSimulator::with_mempool`] serves a [`FullNodeSimulator`]. Accepted transactions wait
///   in its mempool until the test farms a block with [`PeerSimulator::farm_block`], and peers
///   that opt in receive the [CHIP-26] mempool updates.
///
/// [`PeerSimulator`] dereferences to the inner [`Mutex`] around the simulator, so you can lock it
/// to create coins or inspect the state.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_test::PeerSimulator;
///
/// # tokio::runtime::Runtime::new()?.block_on(async {
/// let sim = PeerSimulator::new().await?;
/// let peer = sim.connect().await?;
///
/// let coin = sim.lock().await.new_coin(Bytes32::default(), 1_000);
///
/// let response = peer
///     .request_coin_state(vec![coin.coin_id()], None, sim.config().genesis_challenge, false)
///     .await?
///     .unwrap();
/// assert_eq!(response.coin_states[0].coin, coin);
/// # Ok::<(), anyhow::Error>(())
/// # })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// [CHIP-26]: https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0026.md
#[derive(Debug)]
pub struct PeerSimulator<S: PeerSimulatorBackend = Simulator> {
    addr: SocketAddr,
    shared: Arc<Shared<S>>,
    join_handle: JoinHandle<()>,
}

/// The state shared between the [`PeerSimulator`] and its connections.
///
/// Locks are always taken in the order `simulator`, then `subscriptions`, then `peer_map`.
#[derive(Debug)]
pub(crate) struct Shared<S> {
    config: PeerSimulatorConfig,
    simulator: Mutex<S>,
    subscriptions: Mutex<Subscriptions>,
    peer_map: PeerMap,
}

impl<S: PeerSimulatorBackend> Deref for PeerSimulator<S> {
    type Target = Mutex<S>;

    fn deref(&self) -> &Self::Target {
        &self.shared.simulator
    }
}

impl PeerSimulator<Simulator> {
    /// Starts a peer simulator with the default [`PeerSimulatorConfig`] and a default
    /// [`Simulator`], which includes each transaction in a new block right away.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSimulatorError::Io`] if the listener can't be bound.
    pub async fn new() -> Result<Self, PeerSimulatorError> {
        Self::with_config(PeerSimulatorConfig::default()).await
    }

    /// Starts a peer simulator with the given [`PeerSimulatorConfig`] and a default
    /// [`Simulator`].
    ///
    /// # Errors
    ///
    /// Returns [`PeerSimulatorError::Io`] if the listener can't be bound.
    pub async fn with_config(config: PeerSimulatorConfig) -> Result<Self, PeerSimulatorError> {
        Self::start(config).await
    }
}

impl PeerSimulator<FullNodeSimulator> {
    /// Starts a peer simulator with the default [`PeerSimulatorConfig`] and a mempool, backed by
    /// a [`FullNodeSimulator`].
    ///
    /// See [`PeerSimulator::with_mempool_config`].
    ///
    /// # Errors
    ///
    /// Returns [`PeerSimulatorError::Io`] if the listener can't be bound.
    pub async fn with_mempool() -> Result<Self, PeerSimulatorError> {
        Self::with_mempool_config(PeerSimulatorConfig::default()).await
    }

    /// Starts a peer simulator with the given [`PeerSimulatorConfig`] and a mempool, backed by a
    /// new [`FullNodeSimulator`].
    ///
    /// In this mode:
    ///
    /// - `SendTransaction` validates the transaction and adds it to the mempool, without creating
    ///   a block. Replace-by-fee and conflicts follow [`FullNodeSimulator::push_tx_detailed`].
    /// - [`PeerSimulator::farm_block`] creates blocks from the mempool and sends the
    ///   `NewPeakWallet` and coin state updates.
    /// - `RequestTransaction` is answered from the mempool. Like a full node, there's no response
    ///   if the transaction isn't in the mempool.
    /// - Peers that advertise the CHIP-26 `MEMPOOL_UPDATES` capability (5) in their handshake
    ///   receive `MempoolItemsAdded` and `MempoolItemsRemoved` for transactions that spend or
    ///   create a coin they're subscribed to, by coin id, puzzle hash or hint. A new subscription
    ///   is followed by `MempoolItemsAdded` for the transactions already in the mempool that match
    ///   it. Transactions are removed with the reason `BlockInclusion` when farmed, and
    ///   `Conflict` when replaced by fee or when they're no longer valid after a block (along
    ///   with the transactions that spend their outputs). Since every valid transaction is
    ///   included in the next block, transactions don't expire, and `PoolFull` isn't used. Use
    ///   [`PeerSimulator::connect_split_with_capabilities`] to connect such a peer.
    ///
    /// The simulator's genesis challenge, which is also the `AGG_SIG_ME` additional data, is set
    /// to [`PeerSimulatorConfig::genesis_challenge`]. With the default config, that means spends
    /// signed with [`sign_transaction`](crate::sign_transaction) are valid. The chain starts at
    /// height 1, as described in [`FullNodeSimulator`].
    ///
    /// Changes made by locking the simulator directly, such as
    /// [`FullNodeSimulator::push_tx`], are sent to peers by the next [`PeerSimulator::farm_block`],
    /// transaction, or [`PeerSimulator::notify_peers`]. Reverts and reorgs made this way aren't
    /// sent as coin state updates, only as the mempool changes they cause.
    ///
    /// # Example
    ///
    /// ```
    /// use chia_bls::Signature;
    /// use chia_protocol::{CoinSpend, MempoolItemsAdded, ProtocolMessageTypes, SpendBundle};
    /// use chia_sdk_test::{PeerSimulator, to_program, to_puzzle};
    /// use chia_traits::Streamable;
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async {
    /// let sim = PeerSimulator::with_mempool().await?;
    /// let (peer, mut receiver) = sim
    ///     .connect_split_with_capabilities(vec![(5, "1".to_string())])
    ///     .await?;
    ///
    /// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    /// let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    /// peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;
    ///
    /// let spend_bundle = SpendBundle::new(
    ///     vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
    ///     Signature::default(),
    /// );
    /// let transaction_id = spend_bundle.name();
    /// assert_eq!(peer.send_transaction(spend_bundle).await?.status, 1);
    ///
    /// // The transaction is pending, so the coin isn't spent yet.
    /// let message = receiver.recv().await.unwrap();
    /// assert_eq!(message.msg_type, ProtocolMessageTypes::MempoolItemsAdded);
    /// let added = MempoolItemsAdded::from_bytes(&message.data)?;
    /// assert_eq!(added.transaction_ids, vec![transaction_id]);
    ///
    /// sim.farm_block(1).await;
    /// let record = sim.lock().await.get_coin_record_by_name(coin.coin_id());
    /// assert!(record.coin_record.unwrap().spent);
    /// # Ok::<(), anyhow::Error>(())
    /// # })?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`PeerSimulatorError::Io`] if the listener can't be bound.
    pub async fn with_mempool_config(
        config: PeerSimulatorConfig,
    ) -> Result<Self, PeerSimulatorError> {
        Self::start(config).await
    }

    /// Farms `blocks` blocks (at least one) with [`FullNodeSimulator::farm_block`], and sends
    /// the resulting messages to connected peers.
    ///
    /// For each block, every peer receives a `NewPeakWallet` message, subscribers receive a
    /// `CoinStateUpdate`, and peers that opted into mempool updates receive
    /// `MempoolItemsRemoved` for the matching transactions that left the mempool.
    pub async fn farm_block(&self, blocks: u32) -> Vec<BlockRecord> {
        let mut simulator = self.shared.simulator.lock().await;
        let records = simulator.farm_block(blocks);
        self.flush(&mut simulator).await;
        records
    }
}

impl<S: PeerSimulatorBackend> PeerSimulator<S> {
    async fn start(config: PeerSimulatorConfig) -> Result<Self, PeerSimulatorError> {
        tracing::info!("starting simulator");

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            simulator: Mutex::new(S::new_for_peers(&config)),
            config,
            subscriptions: Mutex::new(Subscriptions::default()),
            peer_map: PeerMap::default(),
        });

        let shared_clone = shared.clone();

        let join_handle = tokio::spawn(async move {
            let shared = shared_clone;

            while let Ok((stream, addr)) = listener.accept().await {
                let stream = match tokio_tungstenite::accept_async(stream).await {
                    Ok(stream) => stream,
                    Err(error) => {
                        tracing::error!("error accepting websocket connection: {}", error);
                        continue;
                    }
                };
                tokio::spawn(ws_connection(shared.clone(), stream, addr));
            }
        });

        Ok(Self {
            addr,
            shared,
            join_handle,
        })
    }

    /// Returns the configuration the simulator was started with.
    pub fn config(&self) -> &PeerSimulatorConfig {
        &self.shared.config
    }

    /// Connects a new [`Peer`] and returns it with the receiver for incoming messages.
    ///
    /// No handshake is sent. The first message is the initial `NewPeakWallet`, which is left in
    /// the receiver.
    ///
    /// # Errors
    ///
    /// Returns an error if the websocket connection fails.
    pub async fn connect_raw(&self) -> Result<(Peer, mpsc::Receiver<Message>), PeerSimulatorError> {
        tracing::info!("connecting new peer to simulator");
        let (ws, _) = connect_async(format!("ws://{}", self.addr)).await?;
        Ok(Peer::from_websocket(ws, PeerOptions::default())?)
    }

    /// Connects a new [`Peer`] and returns it with the receiver for incoming messages, after
    /// waiting for the initial `NewPeakWallet` message and discarding it.
    ///
    /// No handshake is sent, so the peer doesn't receive mempool updates.
    ///
    /// # Errors
    ///
    /// Returns an error if the websocket connection fails.
    ///
    /// # Panics
    ///
    /// Panics if the connection closes before the initial `NewPeakWallet` message arrives.
    pub async fn connect_split(
        &self,
    ) -> Result<(Peer, mpsc::Receiver<Message>), PeerSimulatorError> {
        let (peer, mut receiver) = self.connect_raw().await?;
        receiver
            .recv()
            .await
            .expect("expected NewPeakWallet message");
        Ok((peer, receiver))
    }

    /// Connects a new [`Peer`] and performs the wallet handshake, advertising the given
    /// capabilities. Returns the peer with the receiver for incoming messages, after discarding
    /// the initial `NewPeakWallet` and the simulator's `Handshake`.
    ///
    /// Each capability is a pair of its id and value, such as `(5, "1".to_string())` for the
    /// CHIP-26 `MEMPOOL_UPDATES` capability. A peer connected without the handshake, such as
    /// with [`PeerSimulator::connect_split`], has no capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error if the websocket connection fails, or
    /// [`ClientError::MissingHandshake`] if it closes before the simulator's `Handshake`
    /// arrives.
    pub async fn connect_split_with_capabilities(
        &self,
        capabilities: Vec<(u16, String)>,
    ) -> Result<(Peer, mpsc::Receiver<Message>), PeerSimulatorError> {
        let (peer, mut receiver) = self.connect_raw().await?;

        peer.send(Handshake {
            network_id: "simulator0".to_string(),
            protocol_version: "0.0.37".to_string(),
            software_version: "0.0.0".to_string(),
            server_port: 0,
            node_type: NodeType::Wallet,
            capabilities,
        })
        .await?;

        loop {
            let Some(message) = receiver.recv().await else {
                return Err(ClientError::MissingHandshake.into());
            };
            if message.msg_type == ProtocolMessageTypes::Handshake {
                break;
            }
        }

        Ok((peer, receiver))
    }

    /// Connects a new [`Peer`], discarding all incoming messages on a background task.
    ///
    /// Use [`PeerSimulator::connect_split`] if you need to read coin state updates.
    ///
    /// # Errors
    ///
    /// Returns an error if the websocket connection fails.
    ///
    /// # Panics
    ///
    /// Panics if the connection closes before the initial `NewPeakWallet` message arrives.
    pub async fn connect(&self) -> Result<Peer, PeerSimulatorError> {
        let (peer, mut receiver) = self.connect_split().await?;

        tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                tracing::debug!("received message: {message:?}");
            }
        });

        Ok(peer)
    }

    /// Sends connected peers the messages for changes that were made by locking the simulator
    /// directly, such as transactions pushed to a [`FullNodeSimulator`]'s mempool or blocks
    /// farmed with [`FullNodeSimulator::farm_block`].
    ///
    /// Changes made through the peer protocol or [`PeerSimulator::farm_block`] are sent right
    /// away, so this is only needed after direct changes. With a [`Simulator`], there's nothing
    /// to send.
    pub async fn notify_peers(&self) {
        let mut simulator = self.shared.simulator.lock().await;
        self.flush(&mut simulator).await;
    }

    async fn flush(&self, simulator: &mut S) {
        let events = simulator.drain_events();
        let subscriptions = self.shared.subscriptions.lock().await;
        if let Err(error) =
            notify::notify_peers(&self.shared.peer_map, &subscriptions, &events).await
        {
            tracing::error!("error notifying peers: {}", error);
        }
    }

    /// Replaces the simulator with a new one and clears all subscriptions.
    ///
    /// Existing connections stay open. Peers aren't told about the reset, including the removal
    /// of any transactions that were in the mempool.
    ///
    /// # Errors
    ///
    /// This currently never fails.
    pub async fn reset(&self) -> Result<(), PeerSimulatorError> {
        *self.shared.simulator.lock().await = S::new_for_peers(&self.shared.config);
        *self.shared.subscriptions.lock().await = Subscriptions::default();
        Ok(())
    }
}

impl<S: PeerSimulatorBackend> Drop for PeerSimulator<S> {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::{PublicKey, SecretKey, Signature};
    use chia_protocol::{
        Bytes, Bytes32, Coin, CoinSpend, CoinState, CoinStateFilters, CoinStateUpdate,
        ProtocolMessageTypes, RespondCoinState, RespondPuzzleState, SpendBundle, TransactionAck,
    };
    use chia_sdk_types::conditions::{AggSigMe, CreateCoin, Memos, Remark};
    use chia_traits::Streamable;
    use clvmr::NodePtr;

    use crate::{BlsPair, sign_transaction, to_program, to_puzzle};

    use super::*;

    fn coin_state_updates(receiver: &mut mpsc::Receiver<Message>) -> Vec<CoinStateUpdate> {
        let mut items = Vec::new();
        while let Ok(message) = receiver.try_recv() {
            if message.msg_type != ProtocolMessageTypes::CoinStateUpdate {
                continue;
            }
            items.push(CoinStateUpdate::from_bytes(&message.data).unwrap());
        }
        items
    }

    async fn test_transaction_raw(
        peer: &Peer,
        coin_spends: Vec<CoinSpend>,
        secret_keys: &[SecretKey],
    ) -> anyhow::Result<TransactionAck> {
        let aggregated_signature = sign_transaction(&coin_spends, secret_keys)?;

        Ok(peer
            .send_transaction(SpendBundle::new(coin_spends, aggregated_signature))
            .await?)
    }

    async fn test_transaction(peer: &Peer, coin_spends: Vec<CoinSpend>, secret_keys: &[SecretKey]) {
        let ack = test_transaction_raw(peer, coin_spends, secret_keys)
            .await
            .expect("could not submit transaction");

        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);
    }

    #[tokio::test]
    async fn test_coin_state() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;

        let coin = sim.lock().await.new_coin(Bytes32::default(), 1000);
        let coin_state = sim
            .lock()
            .await
            .coin_state(coin.coin_id())
            .expect("missing coin state");

        assert_eq!(coin_state.coin, coin);
        assert_eq!(coin_state.created_height, Some(0));
        assert_eq!(coin_state.spent_height, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_empty_transaction() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let empty_bundle = SpendBundle::new(Vec::new(), Signature::default());
        let transaction_id = empty_bundle.name();

        let ack = peer.send_transaction(empty_bundle).await?;
        assert_eq!(ack.status, 3);
        assert_eq!(ack.txid, transaction_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_simple_transaction() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_unknown_coin() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = Coin::new(Bytes32::default(), puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_bad_signature() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;
        let public_key = BlsPair::new(0).pk;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([AggSigMe::new(public_key, Bytes::default())])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_infinity_signature() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([AggSigMe::new(PublicKey::default(), Bytes::default())])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_valid_signature() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;
        let pair = BlsPair::new(0);

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        test_transaction(
            &peer,
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([AggSigMe::new(pair.pk, b"Hello, world!".to_vec().into())])?,
            )],
            &[pair.sk],
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_aggregated_signature() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let alice = BlsPair::new(0);
        let bob = BlsPair::new(1);

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        test_transaction(
            &peer,
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([
                    AggSigMe::new(alice.pk, b"Hello, world!".to_vec().into()),
                    AggSigMe::new(bob.pk, b"Goodbye, world!".to_vec().into()),
                ])?,
            )],
            &[alice.sk, bob.sk],
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_excessive_output() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 1, Memos::None)])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_lineage() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let mut coin = sim.lock().await.new_coin(puzzle_hash, 1000);

        for _ in 0..1000 {
            let spend_bundle = SpendBundle::new(
                vec![CoinSpend::new(
                    coin,
                    puzzle_reveal.clone(),
                    to_program([CreateCoin::<NodePtr>::new(
                        puzzle_hash,
                        coin.amount - 1,
                        Memos::None,
                    )])?,
                )],
                Signature::default(),
            );

            let ack = peer.send_transaction(spend_bundle).await?;
            assert_eq!(ack.status, 1);

            coin = Coin::new(coin.coin_id(), puzzle_hash, coin.amount - 1);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_request_children_unknown() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let children = peer.request_children(Bytes32::default()).await?;
        assert!(children.coin_states.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_request_empty_children() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let children = peer.request_children(coin.coin_id()).await?;
        assert!(children.coin_states.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_request_children() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 3);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([
                    CreateCoin::<NodePtr>::new(puzzle_hash, 1, Memos::None),
                    CreateCoin::<NodePtr>::new(puzzle_hash, 2, Memos::None),
                ])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let children = peer.request_children(coin.coin_id()).await?;
        assert_eq!(children.coin_states.len(), 2);

        let found_1 = children
            .coin_states
            .iter()
            .find(|cs| cs.coin.amount == 1)
            .copied();
        let found_2 = children
            .coin_states
            .iter()
            .find(|cs| cs.coin.amount == 2)
            .copied();

        let expected_1 = CoinState::new(Coin::new(coin.coin_id(), puzzle_hash, 1), None, Some(0));
        let expected_2 = CoinState::new(Coin::new(coin.coin_id(), puzzle_hash, 2), None, Some(0));

        assert_eq!(found_1, Some(expected_1));
        assert_eq!(found_2, Some(expected_2));

        Ok(())
    }

    #[tokio::test]
    async fn test_puzzle_solution() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let solution = to_program([Remark::new(())])?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal.clone(),
                solution.clone(),
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let response = peer
            .request_puzzle_and_solution(coin.coin_id(), 0)
            .await?
            .unwrap();
        assert_eq!(response.coin_name, coin.coin_id());
        assert_eq!(response.puzzle, puzzle_reveal);
        assert_eq!(response.solution, solution);
        assert_eq!(response.height, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_spent_coin_subscription() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let (peer, mut receiver) = sim.connect_split().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);
        let mut coin_state = sim
            .lock()
            .await
            .coin_state(coin.coin_id())
            .expect("missing coin state");

        let coin_states = peer
            .register_for_coin_updates(vec![coin.coin_id()], 0)
            .await?
            .coin_states;
        assert_eq!(coin_states.len(), 1);
        assert_eq!(coin_states[0], coin_state);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        coin_state.spent_height = Some(0);

        let updates = coin_state_updates(&mut receiver);
        assert_eq!(updates.len(), 1);

        assert_eq!(
            updates[0],
            CoinStateUpdate::new(1, 1, sim.lock().await.header_hash(), vec![coin_state])
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_created_coin_subscription() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let (peer, mut receiver) = sim.connect_split().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 1);
        let child_coin = Coin::new(coin.coin_id(), puzzle_hash, 1);

        let coin_states = peer
            .register_for_coin_updates(vec![child_coin.coin_id()], 0)
            .await?
            .coin_states;
        assert_eq!(coin_states.len(), 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 1, Memos::None)])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let updates = coin_state_updates(&mut receiver);
        assert_eq!(updates.len(), 1);

        let coin_state = CoinState::new(child_coin, None, Some(0));

        assert_eq!(
            updates[0],
            CoinStateUpdate::new(1, 1, sim.lock().await.header_hash(), vec![coin_state])
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_spent_puzzle_subscription() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let (peer, mut receiver) = sim.connect_split().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);
        let mut coin_state = sim
            .lock()
            .await
            .coin_state(coin.coin_id())
            .expect("missing coin state");

        let coin_states = peer
            .register_for_ph_updates(vec![coin.puzzle_hash], 0)
            .await?
            .coin_states;
        assert_eq!(coin_states.len(), 1);
        assert_eq!(coin_states[0], coin_state);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        coin_state.spent_height = Some(0);

        let updates = coin_state_updates(&mut receiver);
        assert_eq!(updates.len(), 1);

        assert_eq!(
            updates[0],
            CoinStateUpdate::new(1, 1, sim.lock().await.header_hash(), vec![coin_state])
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_created_puzzle_subscription() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let (peer, mut receiver) = sim.connect_split().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 1);
        let child_coin = Coin::new(coin.coin_id(), Bytes32::default(), 1);

        let coin_states = peer
            .register_for_ph_updates(vec![child_coin.puzzle_hash], 0)
            .await?
            .coin_states;
        assert_eq!(coin_states.len(), 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([CreateCoin::<NodePtr>::new(
                    child_coin.puzzle_hash,
                    1,
                    Memos::None,
                )])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let updates = coin_state_updates(&mut receiver);
        assert_eq!(updates.len(), 1);

        let coin_state = CoinState::new(child_coin, None, Some(0));

        assert_eq!(
            updates[0],
            CoinStateUpdate::new(1, 1, sim.lock().await.header_hash(), vec![coin_state])
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_spent_hint_subscription() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let (peer, mut receiver) = sim.connect_split().await?;

        let hint = Bytes32::new([42; 32]);
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);
        sim.lock().await.hint_coin(coin.coin_id(), hint);

        let mut coin_state = sim
            .lock()
            .await
            .coin_state(coin.coin_id())
            .expect("missing coin state");

        let coin_states = peer
            .register_for_ph_updates(vec![hint], 0)
            .await?
            .coin_states;
        assert_eq!(coin_states.len(), 1);
        assert_eq!(coin_states[0], coin_state);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        coin_state.spent_height = Some(0);

        let updates = coin_state_updates(&mut receiver);
        assert_eq!(updates.len(), 1);

        assert_eq!(
            updates[0],
            CoinStateUpdate::new(1, 1, sim.lock().await.header_hash(), vec![coin_state])
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_created_hint_subscription() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let (peer, mut receiver) = sim.connect_split().await?;

        let hint = Bytes32::new([42; 32]);
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);

        let coin_states = peer
            .register_for_ph_updates(vec![hint], 0)
            .await?
            .coin_states;
        assert_eq!(coin_states.len(), 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([CreateCoin::new(puzzle_hash, 0, Memos::Some([hint]))])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let updates = coin_state_updates(&mut receiver);
        assert_eq!(updates.len(), 1);

        assert_eq!(
            updates[0],
            CoinStateUpdate::new(
                1,
                1,
                sim.lock().await.header_hash(),
                vec![CoinState::new(
                    Coin::new(coin.coin_id(), puzzle_hash, 0),
                    None,
                    Some(0)
                )]
            )
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_request_coin_state() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);
        let mut coin_state = sim
            .lock()
            .await
            .coin_state(coin.coin_id())
            .expect("missing coin state");

        let response = peer
            .request_coin_state(
                vec![coin.coin_id()],
                None,
                sim.config().genesis_challenge,
                false,
            )
            .await?
            .unwrap();
        assert_eq!(
            response,
            RespondCoinState::new(vec![coin.coin_id()], vec![coin_state])
        );

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        coin_state.spent_height = Some(0);

        let response = peer
            .request_coin_state(
                vec![coin.coin_id()],
                None,
                sim.config().genesis_challenge,
                false,
            )
            .await?
            .unwrap();
        assert_eq!(
            response,
            RespondCoinState::new(vec![coin.coin_id()], vec![coin_state])
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_request_puzzle_state() -> anyhow::Result<()> {
        let sim = PeerSimulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.lock().await.new_coin(puzzle_hash, 0);
        let mut coin_state = sim
            .lock()
            .await
            .coin_state(coin.coin_id())
            .expect("missing coin state");

        let response = peer
            .request_puzzle_state(
                vec![puzzle_hash],
                None,
                sim.config().genesis_challenge,
                CoinStateFilters::new(true, true, true, 0),
                false,
            )
            .await?
            .unwrap();
        assert_eq!(
            response,
            RespondPuzzleState::new(
                vec![puzzle_hash],
                0,
                sim.lock()
                    .await
                    .header_hash_of(0)
                    .expect("missing header hash"),
                true,
                vec![coin_state]
            )
        );

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        coin_state.spent_height = Some(0);

        let response = peer
            .request_puzzle_state(
                vec![puzzle_hash],
                None,
                sim.config().genesis_challenge,
                CoinStateFilters::new(true, true, true, 0),
                false,
            )
            .await?
            .unwrap();
        assert_eq!(
            response,
            RespondPuzzleState::new(
                vec![puzzle_hash],
                1,
                sim.lock()
                    .await
                    .header_hash_of(1)
                    .expect("missing header hash"),
                true,
                vec![coin_state]
            )
        );

        Ok(())
    }
}
