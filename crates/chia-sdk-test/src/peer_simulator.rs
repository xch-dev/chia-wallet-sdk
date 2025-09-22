use std::{net::SocketAddr, ops::Deref, sync::Arc};

use chia_protocol::Message;
use chia_sdk_client::{Peer, PeerOptions};
use peer_map::PeerMap;
use subscriptions::Subscriptions;
use tokio::{
    net::TcpListener,
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use tokio_tungstenite::connect_async;
use ws_connection::ws_connection;

use crate::Simulator;

mod error;
mod peer_map;
mod simulator_config;
mod subscriptions;
mod ws_connection;

pub use error::*;
pub use simulator_config::*;

#[derive(Debug)]
pub struct PeerSimulator {
    config: Arc<SimulatorConfig>,
    addr: SocketAddr,
    simulator: Arc<Mutex<Simulator>>,
    subscriptions: Arc<Mutex<Subscriptions>>,
    join_handle: JoinHandle<()>,
}

impl Deref for PeerSimulator {
    type Target = Mutex<Simulator>;

    fn deref(&self) -> &Self::Target {
        &self.simulator
    }
}

impl PeerSimulator {
    pub async fn new() -> Result<Self, PeerSimulatorError> {
        Self::with_config(SimulatorConfig::default()).await
    }

    pub async fn with_config(config: SimulatorConfig) -> Result<Self, PeerSimulatorError> {
        tracing::info!("starting simulator");

        let addr = "127.0.0.1:0";
        let peer_map = PeerMap::default();
        let listener = TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        let simulator = Arc::new(Mutex::new(Simulator::default()));
        let subscriptions = Arc::new(Mutex::new(Subscriptions::default()));
        let config = Arc::new(config);

        let simulator_clone = simulator.clone();
        let subscriptions_clone = subscriptions.clone();
        let config_clone = config.clone();

        let join_handle = tokio::spawn(async move {
            let simulator = simulator_clone;
            let subscriptions = subscriptions_clone;
            let config = config_clone;

            while let Ok((stream, addr)) = listener.accept().await {
                let stream = match tokio_tungstenite::accept_async(stream).await {
                    Ok(stream) => stream,
                    Err(error) => {
                        tracing::error!("error accepting websocket connection: {}", error);
                        continue;
                    }
                };
                tokio::spawn(ws_connection(
                    peer_map.clone(),
                    stream,
                    addr,
                    config.clone(),
                    simulator.clone(),
                    subscriptions.clone(),
                ));
            }
        });

        Ok(Self {
            config,
            addr,
            simulator,
            subscriptions,
            join_handle,
        })
    }

    pub fn config(&self) -> &SimulatorConfig {
        &self.config
    }

    pub async fn connect_raw(&self) -> Result<(Peer, mpsc::Receiver<Message>), PeerSimulatorError> {
        tracing::info!("connecting new peer to simulator");
        let (ws, _) = connect_async(format!("ws://{}", self.addr)).await?;
        Ok(Peer::from_websocket(
            ws,
            PeerOptions {
                rate_limit_factor: 0.6,
            },
        )?)
    }

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

    pub async fn connect(&self) -> Result<Peer, PeerSimulatorError> {
        let (peer, mut receiver) = self.connect_split().await?;

        tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                tracing::debug!("received message: {message:?}");
            }
        });

        Ok(peer)
    }

    pub async fn reset(&self) -> Result<(), PeerSimulatorError> {
        *self.simulator.lock().await = Simulator::default();
        *self.subscriptions.lock().await = Subscriptions::default();
        Ok(())
    }
}

impl Drop for PeerSimulator {
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

    use crate::{sign_transaction, to_program, to_puzzle, BlsPair};

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
                sim.config().constants.genesis_challenge,
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
                sim.config().constants.genesis_challenge,
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
                sim.config().constants.genesis_challenge,
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
                sim.config().constants.genesis_challenge,
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
