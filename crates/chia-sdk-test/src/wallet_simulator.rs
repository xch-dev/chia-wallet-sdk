use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
};

use chia_bls::aggregate_verify;
use chia_client::Peer;
use chia_consensus::gen::{
    conditions::EmptyVisitor,
    owned_conditions::OwnedSpendBundleConditions,
    run_block_generator::run_block_generator,
    solution_generator::solution_generator,
    validation_error::{ErrorCode, ValidationErr},
};
use chia_protocol::{
    Bytes, Bytes32, Coin, CoinState, CoinStateUpdate, Message, NewPeakWallet, Program,
    ProtocolMessageTypes, PuzzleSolutionResponse, RegisterForCoinUpdates, RegisterForPhUpdates,
    RejectPuzzleSolution, RequestChildren, RequestPuzzleSolution, RespondChildren,
    RespondPuzzleSolution, RespondToCoinUpdates, RespondToPhUpdates, SendTransaction, SpendBundle,
    TransactionAck,
};
use chia_sdk_signer::RequiredSignature;
use chia_traits::Streamable;
use clvmr::{
    sha2::{Digest, Sha256},
    Allocator, NodePtr, MEMPOOL_MODE,
};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, SinkExt, StreamExt, TryStreamExt};
use indexmap::{IndexMap, IndexSet};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::JoinHandle,
};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

type PeerMapInner = HashMap<SocketAddr, UnboundedSender<WsMessage>>;
type PeerMap = Arc<Mutex<PeerMapInner>>;

/// A very limited full node simulator that can be used to test specifically wallet functionality.
/// It's not guaranteed to be fully accurate, and is only to be able to test wallet code efficiently.
pub struct WalletSimulator {
    rng: Mutex<ChaCha8Rng>,
    addr: SocketAddr,
    join_handle: JoinHandle<()>,
    data: Arc<Mutex<Data>>,
}

#[derive(Default)]
struct Data {
    block_height: u32,
    coin_states: IndexMap<Bytes32, CoinState>,
    hinted_coins: IndexMap<Bytes, IndexSet<Bytes32>>,
    puzzle_subscriptions: IndexMap<SocketAddr, IndexSet<Bytes32>>,
    coin_subscriptions: IndexMap<SocketAddr, IndexSet<Bytes32>>,
    puzzle_and_solutions: IndexMap<Bytes32, (Program, Program)>,
}

impl WalletSimulator {
    /// The `AGG_SIG_ME_ADDIITONAL_DATA` constant for the wallet simulator.
    pub const AGG_SIG_ME: [u8; 32] = [42; 32];

    /// Create a new wallet simulator and start listening for connections in the background.
    pub async fn new() -> Self {
        let addr = "127.0.0.1:0";
        let peer_map = PeerMap::new(Mutex::new(HashMap::new()));
        let try_socket = TcpListener::bind(addr).await;
        let listener = try_socket.unwrap_or_else(|_| panic!("failed to bind to `{addr}`"));
        let addr = listener.local_addr().unwrap();

        let data = Arc::new(Mutex::new(Data::default()));
        let join_handle = tokio::spawn(listen_for_connections(peer_map, listener, data.clone()));

        Self {
            rng: Mutex::new(ChaCha8Rng::seed_from_u64(0)),
            addr,
            join_handle,
            data,
        }
    }

    /// Resets all of the simulator data to default.
    pub async fn reset(&self) {
        let mut data = self.data.lock().await;
        *data = Data::default();
    }

    /// Generate a new coin with the given puzzle hash and amount.
    pub async fn generate_coin(&self, puzzle_hash: Bytes32, amount: u64) -> CoinState {
        let mut data = self.data.lock().await;

        let bytes = self.rng.lock().await.gen();
        let parent_coin_info = Bytes32::new(bytes);

        let coin = Coin {
            parent_coin_info,
            puzzle_hash,
            amount,
        };
        let coin_id = coin.coin_id();
        let coin_state = CoinState::new(coin, None, Some(data.block_height));

        data.coin_states.insert(coin_id, coin_state);
        coin_state
    }

    /// Connects a WebSocket peer to the wallet simulator.
    pub async fn peer(&self) -> Peer {
        let (ws, _) = connect_async(format!("ws://{}", self.addr))
            .await
            .expect("failed to connect to websocket server");
        Peer::new(ws)
    }
}

impl Drop for WalletSimulator {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

async fn listen_for_connections(peer_map: PeerMap, listener: TcpListener, data: Arc<Mutex<Data>>) {
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(
            peer_map.clone(),
            stream,
            addr,
            data.clone(),
        ));
    }
}

async fn handle_connection(
    peer_map: PeerMap,
    raw_stream: TcpStream,
    addr: SocketAddr,
    data: Arc<Mutex<Data>>,
) {
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("failed to accept websocket connection");

    let (tx, rx) = unbounded();
    peer_map.lock().await.insert(addr, tx);

    let (sink, stream) = ws_stream.split();

    let broadcast_incoming = stream.try_for_each(|message| {
        let peer_map = peer_map.clone();
        let data = data.clone();

        async move {
            let message = Message::from_bytes(&message.into_data()).unwrap();

            match message.msg_type {
                ProtocolMessageTypes::SendTransaction => {
                    let tx = SendTransaction::from_bytes(message.data.as_ref()).unwrap();
                    let spend_bundle = tx.transaction;

                    let transaction_id = spend_bundle.name();

                    let error = process_spend_bundle(peer_map.clone(), data, spend_bundle)
                        .await
                        .err();

                    let body = match error {
                        Some(error) => {
                            TransactionAck::new(transaction_id, 3, Some(format!("{:?}", error.1)))
                        }
                        None => TransactionAck::new(transaction_id, 1, None),
                    }
                    .to_bytes()
                    .unwrap();

                    let response = Message {
                        msg_type: ProtocolMessageTypes::TransactionAck,
                        data: body.into(),
                        id: message.id,
                    }
                    .to_bytes()
                    .unwrap();

                    peer_map
                        .lock()
                        .await
                        .get_mut(&addr)
                        .unwrap()
                        .send(response.into())
                        .await
                        .unwrap();
                }
                ProtocolMessageTypes::RegisterForCoinUpdates => {
                    let request =
                        RegisterForCoinUpdates::from_bytes(message.data.as_ref()).unwrap();

                    let mut coin_states = Vec::new();
                    let mut data = data.lock().await;

                    for coin_id in request.coin_ids.iter() {
                        if let Some(coin_state) = data.coin_states.get(coin_id).copied() {
                            coin_states.push(coin_state);
                        }

                        data.coin_subscriptions
                            .entry(addr)
                            .or_default()
                            .insert(*coin_id);
                    }

                    let response = Message {
                        msg_type: ProtocolMessageTypes::RespondToCoinUpdates,
                        data: RespondToCoinUpdates {
                            coin_ids: request.coin_ids,
                            min_height: request.min_height,
                            coin_states,
                        }
                        .to_bytes()
                        .unwrap()
                        .into(),
                        id: message.id,
                    }
                    .to_bytes()
                    .unwrap();

                    peer_map
                        .lock()
                        .await
                        .get_mut(&addr)
                        .unwrap()
                        .send(response.into())
                        .await
                        .unwrap();
                }
                ProtocolMessageTypes::RegisterForPhUpdates => {
                    let request = RegisterForPhUpdates::from_bytes(message.data.as_ref()).unwrap();

                    let mut coin_states = IndexMap::new();
                    let mut data = data.lock().await;

                    for (coin_id, coin_state) in data.coin_states.iter() {
                        if request.puzzle_hashes.contains(&coin_state.coin.puzzle_hash) {
                            coin_states.insert(*coin_id, data.coin_states[coin_id]);
                        }
                    }

                    for puzzle_hash in request.puzzle_hashes.iter() {
                        if let Some(coin_state) = data.coin_states.get(puzzle_hash).copied() {
                            coin_states.insert(coin_state.coin.coin_id(), coin_state);
                        }

                        if let Some(hinted_coins) =
                            data.hinted_coins.get(&Bytes::new(puzzle_hash.to_vec()))
                        {
                            for coin_id in hinted_coins.iter() {
                                coin_states.insert(*coin_id, data.coin_states[coin_id]);
                            }
                        }

                        data.puzzle_subscriptions
                            .entry(addr)
                            .or_default()
                            .insert(*puzzle_hash);
                    }

                    let response = Message {
                        msg_type: ProtocolMessageTypes::RespondToPhUpdates,
                        data: RespondToPhUpdates {
                            puzzle_hashes: request.puzzle_hashes,
                            min_height: request.min_height,
                            coin_states: coin_states.into_values().collect(),
                        }
                        .to_bytes()
                        .unwrap()
                        .into(),
                        id: message.id,
                    }
                    .to_bytes()
                    .unwrap();

                    peer_map
                        .lock()
                        .await
                        .get_mut(&addr)
                        .unwrap()
                        .send(response.into())
                        .await
                        .unwrap();
                }
                ProtocolMessageTypes::RequestPuzzleSolution => {
                    let request = RequestPuzzleSolution::from_bytes(message.data.as_ref()).unwrap();
                    let data = data.lock().await;

                    let matches_height = data
                        .coin_states
                        .get(&request.coin_name)
                        .map_or(false, |cs| cs.spent_height == Some(request.height));

                    let response = match data.puzzle_and_solutions.get(&request.coin_name).cloned()
                    {
                        Some((puzzle, solution)) if matches_height => Message {
                            msg_type: ProtocolMessageTypes::RespondPuzzleSolution,
                            data: RespondPuzzleSolution::new(PuzzleSolutionResponse::new(
                                request.coin_name,
                                request.height,
                                puzzle,
                                solution,
                            ))
                            .to_bytes()
                            .unwrap()
                            .into(),
                            id: message.id,
                        }
                        .to_bytes()
                        .unwrap(),
                        _ => Message {
                            msg_type: ProtocolMessageTypes::RejectPuzzleSolution,
                            data: RejectPuzzleSolution::new(request.coin_name, request.height)
                                .to_bytes()
                                .unwrap()
                                .into(),
                            id: message.id,
                        }
                        .to_bytes()
                        .unwrap(),
                    };

                    peer_map
                        .lock()
                        .await
                        .get_mut(&addr)
                        .unwrap()
                        .send(response.into())
                        .await
                        .unwrap();
                }
                ProtocolMessageTypes::RequestChildren => {
                    let request = RequestChildren::from_bytes(message.data.as_ref()).unwrap();
                    let data = data.lock().await;

                    let coin_states: Vec<CoinState> = data
                        .coin_states
                        .iter()
                        .filter(|(_, cs)| cs.coin.parent_coin_info == request.coin_name)
                        .map(|(_, cs)| *cs)
                        .collect();

                    let response = Message {
                        msg_type: ProtocolMessageTypes::RespondChildren,
                        data: RespondChildren::new(coin_states).to_bytes().unwrap().into(),
                        id: message.id,
                    }
                    .to_bytes()
                    .unwrap();

                    peer_map
                        .lock()
                        .await
                        .get_mut(&addr)
                        .unwrap()
                        .send(response.into())
                        .await
                        .unwrap();
                }
                _ => unimplemented!(
                    "unsupported message type for wallet simulator: {:?}",
                    message.msg_type
                ),
            }

            Ok(())
        }
    });

    let receive_from_others = rx.map(Ok).forward(sink);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    peer_map.lock().await.remove(&addr);
}

async fn process_spend_bundle(
    peer_map: PeerMap,
    data: Arc<Mutex<Data>>,
    spend_bundle: SpendBundle,
) -> Result<(), ValidationErr> {
    let mut allocator = Allocator::new();

    let gen = solution_generator(
        spend_bundle
            .coin_spends
            .iter()
            .cloned()
            .map(|spend| (spend.coin, spend.puzzle_reveal, spend.solution)),
    )
    .unwrap();

    let conds = run_block_generator::<&[u8], EmptyVisitor>(
        &mut allocator,
        &gen,
        &[],
        6_600_000_000,
        MEMPOOL_MODE,
    )?;

    let conds = OwnedSpendBundleConditions::from(&allocator, conds).unwrap();

    let cond_puzzle_hashes = conds
        .spends
        .iter()
        .map(|s| s.puzzle_hash)
        .collect::<HashSet<_>>();

    let bundle_puzzle_hashes = spend_bundle
        .coin_spends
        .iter()
        .map(|s| s.coin.puzzle_hash)
        .collect::<HashSet<_>>();

    if cond_puzzle_hashes != bundle_puzzle_hashes {
        return Err(ValidationErr(NodePtr::NIL, ErrorCode::InvalidSpendBundle));
    }

    let required_signatures = RequiredSignature::from_coin_spends(
        &mut allocator,
        &spend_bundle.coin_spends,
        WalletSimulator::AGG_SIG_ME.into(),
    )
    .unwrap();

    if !aggregate_verify(
        &spend_bundle.aggregated_signature,
        required_signatures
            .into_iter()
            .map(|r| (r.public_key(), r.final_message()))
            .collect::<Vec<_>>(),
    ) {
        return Err(ValidationErr(
            NodePtr::NIL,
            ErrorCode::BadAggregateSignature,
        ));
    }

    let data = &mut data.lock().await;

    let mut removed_coins = IndexMap::new();
    let mut added_coins = IndexMap::new();
    let mut added_hints: IndexMap<Bytes, IndexSet<Bytes32>> = IndexMap::new();
    let mut puzzles_and_solutions = IndexMap::new();

    for coin_spend in spend_bundle.coin_spends.into_iter() {
        puzzles_and_solutions.insert(
            coin_spend.coin.coin_id(),
            (coin_spend.puzzle_reveal, coin_spend.solution),
        );
    }

    // Calculate additions and removals.
    for spend in conds.spends.iter() {
        for new_coin in spend.create_coin.iter() {
            let coin = Coin {
                parent_coin_info: spend.coin_id,
                puzzle_hash: new_coin.0,
                amount: new_coin.1,
            };

            let coin_id = coin.coin_id();

            let coin_state = CoinState {
                coin,
                spent_height: None,
                created_height: Some(data.block_height),
            };

            added_coins.insert(coin_id, coin_state);

            if let Some(hint) = new_coin.2.clone() {
                added_hints.entry(hint).or_default().insert(coin_id);
            }
        }

        let coin_state = data
            .coin_states
            .get(&spend.coin_id)
            .cloned()
            .unwrap_or_else(|| CoinState {
                coin: Coin {
                    parent_coin_info: spend.parent_id,
                    puzzle_hash: spend.puzzle_hash,
                    amount: spend.coin_amount,
                },
                created_height: Some(data.block_height),
                spent_height: None,
            });

        removed_coins.insert(spend.coin_id, coin_state);
    }

    // Validate removals.
    for (coin_id, coin_state) in removed_coins.iter_mut() {
        let height = data.block_height;

        if !data.coin_states.contains_key(coin_id) && !added_coins.contains_key(coin_id) {
            return Err(ValidationErr(NodePtr::NIL, ErrorCode::UnknownUnspent));
        }

        if coin_state.spent_height.is_some() {
            return Err(ValidationErr(NodePtr::NIL, ErrorCode::DoubleSpend));
        }

        coin_state.spent_height = Some(height);
    }

    // Update the coin data.
    let mut updates = added_coins.clone();
    updates.extend(removed_coins);
    data.block_height += 1;
    data.coin_states.extend(updates.clone());
    data.hinted_coins.extend(added_hints.clone());
    data.puzzle_and_solutions.extend(puzzles_and_solutions);

    // Calculate a deterministic but fake header hash.
    let mut hasher = Sha256::new();
    hasher.update(data.block_height.to_be_bytes());
    let header_hash = Bytes32::new(hasher.finalize().into());

    let mut peers = peer_map.lock().await;

    // Send updates to peers.
    for (&addr, peer) in peers.iter_mut() {
        let mut peer_updates = IndexSet::new();

        let coin_subscriptions = data
            .coin_subscriptions
            .get(&addr)
            .cloned()
            .unwrap_or_default();

        let puzzle_subscriptions = data
            .puzzle_subscriptions
            .get(&addr)
            .cloned()
            .unwrap_or_default();

        for (hint, coins) in added_hints.iter() {
            let Ok(hint) = hint.to_vec().try_into() else {
                continue;
            };
            let hint = Bytes32::new(hint);

            if puzzle_subscriptions.contains(&hint) {
                peer_updates.extend(coins.iter().map(|coin_id| data.coin_states[coin_id]));
            }
        }

        for coin_id in updates.keys() {
            if coin_subscriptions.contains(coin_id) {
                peer_updates.insert(data.coin_states[coin_id]);
            }

            if puzzle_subscriptions.contains(&data.coin_states[coin_id].coin.puzzle_hash) {
                peer_updates.insert(data.coin_states[coin_id]);
            }
        }

        let new_peak = Message {
            msg_type: ProtocolMessageTypes::NewPeakWallet,
            id: None,
            data: NewPeakWallet::new(header_hash, data.block_height, 0, data.block_height)
                .to_bytes()
                .unwrap()
                .into(),
        }
        .to_bytes()
        .unwrap();

        peer.send(new_peak.into()).await.unwrap();

        if !peer_updates.is_empty() {
            let update = Message {
                msg_type: ProtocolMessageTypes::CoinStateUpdate,
                id: None,
                data: CoinStateUpdate::new(
                    data.block_height,
                    data.block_height,
                    header_hash,
                    peer_updates.into_iter().collect(),
                )
                .to_bytes()
                .unwrap()
                .into(),
            }
            .to_bytes()
            .unwrap();

            peer.send(update.into()).await.unwrap();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use chia_bls::Signature;
    use chia_client::PeerEvent;
    use chia_protocol::{CoinSpend, SpendBundle};
    use chia_sdk_types::conditions::CreateCoin;
    use clvm_traits::{FromNodePtr, ToClvm};
    use clvm_utils::tree_hash;

    #[tokio::test]
    async fn test_coin_lineage_many_blocks() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let mut coin = sim.generate_coin(puzzle_hash, 1000).await.coin;

        for _ in 0..1000 {
            let solution = [CreateCoin::new(puzzle_hash, coin.amount - 1)].to_clvm(&mut a)?;

            let coin_spend = CoinSpend::new(
                coin,
                puzzle_reveal.clone(),
                Program::from_node_ptr(&a, solution)?,
            );

            let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

            let transaction_id = spend_bundle.name();
            let ack = peer.send_transaction(spend_bundle.clone()).await.unwrap();
            assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

            coin = Coin {
                parent_coin_info: coin.coin_id(),
                puzzle_hash,
                amount: coin.amount - 1,
            };

            let ack = peer.send_transaction(spend_bundle).await.unwrap();
            assert_eq!(ack.txid, transaction_id);
            assert_eq!(ack.status, 3);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_spend_unknown_coin() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let solution = [CreateCoin::new(puzzle_hash, 1000)].to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(
            Coin {
                parent_coin_info: Bytes32::new([42; 32]),
                puzzle_hash,
                amount: 1000,
            },
            puzzle_reveal,
            Program::from_node_ptr(&a, solution)?,
        );

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack.txid, transaction_id);
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_coin_subscriptions() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let mut cs = sim.generate_coin(puzzle_hash, 1000).await;

        // Subscribe and request initial state.
        let results = peer
            .register_for_coin_updates(vec![cs.coin.coin_id()], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        // The initial state should still be the same.
        let results = peer
            .register_for_coin_updates(vec![cs.coin.coin_id()], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        // Spend the coin.
        let solution = [CreateCoin::new(puzzle_hash, cs.coin.amount - 1)].to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(
            cs.coin,
            puzzle_reveal,
            Program::from_node_ptr(&a, solution)?,
        );

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        // We should have gotten a new peak and an update.
        // But the coin is spent now.
        cs.spent_height = Some(0);

        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PeerEvent::NewPeakWallet(..)));

        let event = receiver.recv().await.unwrap();
        match event {
            PeerEvent::CoinStateUpdate(update) => {
                assert_eq!(update.items, vec![cs]);
            }
            _ => panic!("unexpected event: {:?}", event),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_puzzle_subscriptions() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let mut cs = sim.generate_coin(puzzle_hash, 1000).await;

        // Subscribe and request initial state.
        let results = peer
            .register_for_ph_updates(vec![cs.coin.puzzle_hash], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        // The initial state should still be the same.
        let results = peer
            .register_for_ph_updates(vec![cs.coin.puzzle_hash], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        // Spend the coin.
        let solution = [CreateCoin::new(puzzle_hash, cs.coin.amount - 1)].to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(
            cs.coin,
            puzzle_reveal,
            Program::from_node_ptr(&a, solution)?,
        );

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        // We should have gotten a new peak and an update.
        // But the coin is spent now.
        cs.spent_height = Some(0);
        let new_cs = CoinState {
            coin: Coin {
                parent_coin_info: cs.coin.coin_id(),
                puzzle_hash,
                amount: cs.coin.amount - 1,
            },
            created_height: Some(0),
            spent_height: None,
        };

        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PeerEvent::NewPeakWallet(..)));

        let event = receiver.recv().await.unwrap();
        match event {
            PeerEvent::CoinStateUpdate(update) => {
                let items = update.items.into_iter().collect::<HashSet<_>>();
                let expected = vec![cs, new_cs].into_iter().collect::<HashSet<_>>();
                assert_eq!(items, expected);
            }
            _ => panic!("unexpected event: {:?}", event),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_hint_subscriptions() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let cs = sim.generate_coin(puzzle_hash, 1000).await;

        let hint = Bytes32::new([34; 32]);

        // Subscribe and request initial state.
        let results = peer.register_for_ph_updates(vec![hint], 0).await.unwrap();
        assert!(results.is_empty());

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        // Spend the coin.
        let solution = [CreateCoin::with_custom_hint(
            puzzle_hash,
            cs.coin.amount - 1,
            hint,
        )]
        .to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(
            cs.coin,
            puzzle_reveal,
            Program::from_node_ptr(&a, solution)?,
        );

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        // We should have gotten a new peak and an update.
        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PeerEvent::NewPeakWallet(..)));

        let event = receiver.recv().await.unwrap();
        match event {
            PeerEvent::CoinStateUpdate(update) => {
                let new_cs = CoinState {
                    coin: Coin {
                        parent_coin_info: cs.coin.coin_id(),
                        puzzle_hash,
                        amount: cs.coin.amount - 1,
                    },
                    created_height: Some(0),
                    spent_height: None,
                };

                assert_eq!(update.items, vec![new_cs]);
            }
            _ => panic!("unexpected event: {:?}", event),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_puzzle_solution() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let cs = sim.generate_coin(puzzle_hash, 1000).await;

        let solution = [CreateCoin::new(puzzle_hash, cs.coin.amount - 1)].to_clvm(&mut a)?;

        let solution = Program::from_node_ptr(&a, solution)?;
        let coin_spend = CoinSpend::new(cs.coin, puzzle_reveal.clone(), solution.clone());
        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        let response = peer
            .request_puzzle_and_solution(cs.coin.coin_id(), 0)
            .await
            .unwrap();

        assert_eq!(
            response,
            PuzzleSolutionResponse::new(cs.coin.coin_id(), 0, puzzle_reveal, solution)
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_request_children() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let cs = sim.generate_coin(puzzle_hash, 1000).await;

        let solution = [CreateCoin::new(puzzle_hash, cs.coin.amount - 1)].to_clvm(&mut a)?;
        let solution = Program::from_node_ptr(&a, solution)?;
        let coin_spend = CoinSpend::new(cs.coin, puzzle_reveal.clone(), solution.clone());
        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        let response = peer.request_children(cs.coin.coin_id()).await.unwrap();

        let new_cs = CoinState {
            coin: Coin {
                parent_coin_info: cs.coin.coin_id(),
                puzzle_hash,
                amount: cs.coin.amount - 1,
            },
            created_height: Some(0),
            spent_height: None,
        };

        assert_eq!(response, vec![new_cs]);

        Ok(())
    }
}
