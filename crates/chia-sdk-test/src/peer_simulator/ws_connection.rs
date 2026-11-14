use std::{net::SocketAddr, sync::Arc};

use chia_consensus::validation_error::{ErrorCode, ValidationErr};
use chia_protocol::{
    Bytes, Bytes32, ChiaProtocolMessage, CoinState, Handshake, Message, NewPeakWallet, NodeType,
    ProtocolMessageTypes, PuzzleSolutionResponse, RegisterForCoinUpdates, RegisterForPhUpdates,
    RejectCoinState, RejectPuzzleSolution, RejectPuzzleState, RejectStateReason, RequestChildren,
    RequestCoinState, RequestPuzzleSolution, RequestPuzzleState, RequestRemoveCoinSubscriptions,
    RequestRemovePuzzleSubscriptions, RequestTransaction, RespondChildren, RespondCoinState,
    RespondPuzzleSolution, RespondPuzzleState, RespondRemoveCoinSubscriptions,
    RespondRemovePuzzleSubscriptions, RespondToCoinUpdates, RespondToPhUpdates, RespondTransaction,
    SendTransaction, TransactionAck,
};
use chia_traits::Streamable;
use futures_channel::mpsc::{self, UnboundedSender};
use futures_util::{SinkExt, StreamExt};
use indexmap::IndexSet;
use itertools::Itertools;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{self, Message as WsMessage},
};

use crate::SimulatorError;

use super::{
    Shared,
    backend::Backend,
    config::PeerSimulatorConfig,
    error::PeerSimulatorError,
    notify::{MEMPOOL_UPDATES_CAPABILITY, mempool_items_added, message, notify_peers},
    subscriptions::Subscriptions,
};

pub(crate) async fn ws_connection<B: Backend>(
    shared: Arc<Shared<B>>,
    ws: WebSocketStream<TcpStream>,
    addr: SocketAddr,
) {
    let (mut tx, mut rx) = mpsc::unbounded();

    if let Err(error) = handle_initial_peak(&mut tx, &shared).await {
        tracing::error!("error sending initial peak: {}", error);
        return;
    }

    shared.peer_map.insert(addr, tx.clone()).await;

    let (mut sink, mut stream) = ws.split();

    tokio::spawn(async move {
        while let Some(message) = rx.next().await {
            if let Err(error) = sink.send(message).await {
                tracing::error!("error sending message to peer: {}", error);
            }
        }
    });

    while let Some(message) = stream.next().await {
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                tracing::info!("received error from stream: {:?}", error);
                break;
            }
        };

        if let Err(error) = handle_message(&shared, message, addr, tx.clone()).await {
            tracing::error!("error handling message: {}", error);
            break;
        }
    }

    shared.peer_map.remove(addr).await;
    shared.subscriptions.lock().await.remove_peer(addr);
}

async fn handle_initial_peak<B: Backend>(
    tx: &mut UnboundedSender<tungstenite::Message>,
    shared: &Shared<B>,
) -> Result<(), PeerSimulatorError> {
    let (header_hash, height) = {
        let sim = shared.simulator.lock().await;
        (sim.peak_header_hash(), sim.peak_height())
    };

    tx.send(message(&NewPeakWallet::new(
        header_hash,
        height,
        0,
        height,
    ))?)
    .await?;

    Ok(())
}

/// The response to send back for a request, if there is one.
struct Reply {
    response: Option<(ProtocolMessageTypes, Bytes)>,
}

impl Reply {
    /// A response whose type is the message's own, so that rejections are labelled as such.
    fn message<T>(body: &T) -> Result<Self, PeerSimulatorError>
    where
        T: Streamable + ChiaProtocolMessage,
    {
        Ok(Self::new(T::msg_type(), body.to_bytes()?.into()))
    }

    fn new(response_type: ProtocolMessageTypes, response: Bytes) -> Self {
        Self {
            response: Some((response_type, response)),
        }
    }
}

/// The ids a request subscribed to, used to send the mempool items that already match them.
#[derive(Default)]
struct NewSubscriptions {
    coin_ids: IndexSet<Bytes32>,
    puzzle_hashes: IndexSet<Bytes32>,
}

async fn handle_message<B: Backend>(
    shared: &Shared<B>,
    message: WsMessage,
    addr: SocketAddr,
    mut ws: super::peer_map::Ws,
) -> Result<(), PeerSimulatorError> {
    let request = Message::from_bytes(&message.into_data())?;

    if request.msg_type == ProtocolMessageTypes::Handshake {
        let handshake = Handshake::from_bytes(&request.data)?;
        let response = handle_handshake::<B>(shared, addr, handshake).await?;
        ws.send(response).await?;
        return Ok(());
    }

    let mempool_updates = shared.peer_map.mempool_updates(addr).await;
    let config = &shared.config;
    let mut simulator = shared.simulator.lock().await;

    let mut new_subscriptions = NewSubscriptions::default();

    let reply = match request.msg_type {
        ProtocolMessageTypes::SendTransaction => {
            let request = SendTransaction::from_bytes(&request.data)?;
            let response = send_transaction(shared, request, &mut *simulator).await?;
            Reply::new(ProtocolMessageTypes::TransactionAck, response)
        }
        ProtocolMessageTypes::RequestTransaction => {
            let request = RequestTransaction::from_bytes(&request.data)?;
            // Like a full node, don't respond if the transaction isn't in the mempool.
            let response = simulator
                .mempool_transaction(request.transaction_id)
                .map(|transaction| {
                    Ok::<_, PeerSimulatorError>((
                        ProtocolMessageTypes::RespondTransaction,
                        RespondTransaction::new(transaction).to_bytes()?.into(),
                    ))
                })
                .transpose()?;
            Reply { response }
        }
        ProtocolMessageTypes::RegisterForCoinUpdates => {
            let request = RegisterForCoinUpdates::from_bytes(&request.data)?;
            let mut subscriptions = shared.subscriptions.lock().await;
            let response = register_for_coin_updates(
                addr,
                request,
                &*simulator,
                &mut subscriptions,
                &mut new_subscriptions,
            )?;
            Reply::new(ProtocolMessageTypes::RespondToCoinUpdates, response)
        }
        ProtocolMessageTypes::RegisterForPhUpdates => {
            let request = RegisterForPhUpdates::from_bytes(&request.data)?;
            let mut subscriptions = shared.subscriptions.lock().await;
            let response = register_for_ph_updates(
                addr,
                request,
                &*simulator,
                &mut subscriptions,
                &mut new_subscriptions,
            )?;
            Reply::new(ProtocolMessageTypes::RespondToPhUpdates, response)
        }
        ProtocolMessageTypes::RequestPuzzleSolution => {
            let request = RequestPuzzleSolution::from_bytes(&request.data)?;
            request_puzzle_solution(&request, &*simulator)?
        }
        ProtocolMessageTypes::RequestChildren => {
            let request = RequestChildren::from_bytes(&request.data)?;
            let response = request_children(&request, &*simulator)?;
            Reply::new(ProtocolMessageTypes::RespondChildren, response)
        }
        ProtocolMessageTypes::RequestCoinState => {
            let request = RequestCoinState::from_bytes(&request.data)?;
            let mut subscriptions = shared.subscriptions.lock().await;
            request_coin_state(
                addr,
                request,
                config,
                &*simulator,
                &mut subscriptions,
                &mut new_subscriptions,
            )?
        }
        ProtocolMessageTypes::RequestPuzzleState => {
            let request = RequestPuzzleState::from_bytes(&request.data)?;
            let mut subscriptions = shared.subscriptions.lock().await;
            request_puzzle_state(
                addr,
                request,
                config,
                &*simulator,
                &mut subscriptions,
                &mut new_subscriptions,
            )?
        }
        ProtocolMessageTypes::RequestRemoveCoinSubscriptions => {
            let request = RequestRemoveCoinSubscriptions::from_bytes(&request.data)?;
            let mut subscriptions = shared.subscriptions.lock().await;
            let response = request_remove_coin_subscriptions(addr, request, &mut subscriptions)?;
            Reply::new(
                ProtocolMessageTypes::RespondRemoveCoinSubscriptions,
                response,
            )
        }
        ProtocolMessageTypes::RequestRemovePuzzleSubscriptions => {
            let request = RequestRemovePuzzleSubscriptions::from_bytes(&request.data)?;
            let mut subscriptions = shared.subscriptions.lock().await;
            let response = request_remove_puzzle_subscriptions(addr, request, &mut subscriptions)?;
            Reply::new(
                ProtocolMessageTypes::RespondRemovePuzzleSubscriptions,
                response,
            )
        }
        message_type => {
            return Err(PeerSimulatorError::UnsupportedMessage(message_type));
        }
    };

    // CHIP-26: a new subscription is followed by the mempool items that already match it.
    let mempool_replay = if mempool_updates
        && (!new_subscriptions.coin_ids.is_empty() || !new_subscriptions.puzzle_hashes.is_empty())
    {
        mempool_items_added(
            &simulator.mempool_items(),
            &new_subscriptions.coin_ids,
            &new_subscriptions.puzzle_hashes,
        )?
    } else {
        None
    };

    // The simulator stays locked until the messages are queued, so that a block farmed at the
    // same time can't be sent to this peer before the response that describes the older state.
    if let Some((response_type, response_data)) = reply.response {
        let message = Message {
            msg_type: response_type,
            data: response_data,
            id: request.id,
        }
        .to_bytes()?;
        ws.send(message.into()).await?;
    }

    if let Some(mempool_replay) = mempool_replay {
        ws.send(mempool_replay).await?;
    }

    drop(simulator);

    Ok(())
}

async fn handle_handshake<B: Backend>(
    shared: &Shared<B>,
    addr: SocketAddr,
    handshake: Handshake,
) -> Result<WsMessage, PeerSimulatorError> {
    let mempool_updates = B::HAS_MEMPOOL
        && handshake
            .capabilities
            .iter()
            .any(|(capability, value)| *capability == MEMPOOL_UPDATES_CAPABILITY && value == "1");

    shared
        .peer_map
        .set_mempool_updates(addr, mempool_updates)
        .await;

    let mut capabilities = vec![
        (1, "1".to_string()),
        (2, "1".to_string()),
        (3, "1".to_string()),
    ];

    if B::HAS_MEMPOOL {
        capabilities.push((MEMPOOL_UPDATES_CAPABILITY, "1".to_string()));
    }

    message(&Handshake {
        network_id: handshake.network_id,
        protocol_version: "0.0.37".to_string(),
        software_version: "0.0.0".to_string(),
        server_port: 0,
        node_type: NodeType::FullNode,
        capabilities,
    })
}

async fn send_transaction<B: Backend>(
    shared: &Shared<B>,
    request: SendTransaction,
    simulator: &mut B,
) -> Result<Bytes, PeerSimulatorError> {
    let transaction_id = request.transaction.name();

    let events = match simulator.submit_transaction(request.transaction) {
        Ok(events) => events,
        Err(error) => {
            tracing::error!("error processing transaction: {:?}", &error);

            let error_code = match error {
                SimulatorError::Validation(error_code) => error_code,
                _ => ErrorCode::Unknown,
            };

            return Ok(TransactionAck::new(
                transaction_id,
                3,
                Some(format!("{:?}", ValidationErr::Err(error_code))),
            )
            .to_bytes()?
            .into());
        }
    };

    let subscriptions = shared.subscriptions.lock().await;
    notify_peers(&shared.peer_map, &subscriptions, &events).await?;

    Ok(TransactionAck::new(transaction_id, 1, None)
        .to_bytes()?
        .into())
}

fn register_for_coin_updates<B: Backend>(
    peer: SocketAddr,
    request: RegisterForCoinUpdates,
    simulator: &B,
    subscriptions: &mut Subscriptions,
    new_subscriptions: &mut NewSubscriptions,
) -> Result<Bytes, PeerSimulatorError> {
    let coin_ids: IndexSet<Bytes32> = request.coin_ids.iter().copied().collect();

    let coin_states: Vec<CoinState> = simulator
        .coin_states(&coin_ids)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= request.min_height
        })
        .collect();

    new_subscriptions.coin_ids.extend(coin_ids.iter().copied());
    subscriptions.add_coin_subscriptions(peer, coin_ids);

    Ok(RespondToCoinUpdates {
        coin_ids: request.coin_ids,
        min_height: request.min_height,
        coin_states,
    }
    .to_bytes()?
    .into())
}

fn register_for_ph_updates<B: Backend>(
    peer: SocketAddr,
    request: RegisterForPhUpdates,
    simulator: &B,
    subscriptions: &mut Subscriptions,
    new_subscriptions: &mut NewSubscriptions,
) -> Result<Bytes, PeerSimulatorError> {
    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();

    let coin_states: Vec<CoinState> = simulator
        .coin_states_by_puzzle_hashes(&puzzle_hashes, true)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= request.min_height
        })
        .collect();

    new_subscriptions
        .puzzle_hashes
        .extend(puzzle_hashes.iter().copied());
    subscriptions.add_puzzle_subscriptions(peer, puzzle_hashes);

    Ok(RespondToPhUpdates {
        puzzle_hashes: request.puzzle_hashes,
        min_height: request.min_height,
        coin_states,
    }
    .to_bytes()?
    .into())
}

fn request_puzzle_solution<B: Backend>(
    request: &RequestPuzzleSolution,
    simulator: &B,
) -> Result<Reply, PeerSimulatorError> {
    let Some((puzzle_reveal, solution)) =
        simulator.puzzle_and_solution(request.coin_name, request.height)
    else {
        return Reply::message(&RejectPuzzleSolution {
            coin_name: request.coin_name,
            height: request.height,
        });
    };

    Reply::message(&RespondPuzzleSolution::new(PuzzleSolutionResponse::new(
        request.coin_name,
        request.height,
        puzzle_reveal,
        solution,
    )))
}

fn request_children<B: Backend>(
    request: &RequestChildren,
    simulator: &B,
) -> Result<Bytes, PeerSimulatorError> {
    Ok(RespondChildren::new(simulator.children(request.coin_name))
        .to_bytes()?
        .into())
}

fn request_coin_state<B: Backend>(
    peer: SocketAddr,
    request: RequestCoinState,
    config: &PeerSimulatorConfig,
    simulator: &B,
    subscriptions: &mut Subscriptions,
    new_subscriptions: &mut NewSubscriptions,
) -> Result<Reply, PeerSimulatorError> {
    if let Some(previous_height) = request.previous_height {
        if Some(request.header_hash) != simulator.header_hash_at(previous_height) {
            return Reply::message(&RejectCoinState::new(RejectStateReason::Reorg));
        }
    } else if request.header_hash != config.genesis_challenge {
        return Reply::message(&RejectCoinState::new(RejectStateReason::Reorg));
    }

    let coin_ids: IndexSet<Bytes32> = request.coin_ids.iter().copied().collect();
    let min_height = request.previous_height.map_or(0, |height| height + 1);
    let subscription_count = subscriptions.subscription_count(peer);

    if subscription_count + coin_ids.len() > config.max_subscriptions && request.subscribe {
        return Reply::message(&RejectCoinState::new(
            RejectStateReason::ExceededSubscriptionLimit,
        ));
    }

    let coin_states: Vec<CoinState> = simulator
        .coin_states(&coin_ids)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= min_height
        })
        .collect();

    if request.subscribe {
        new_subscriptions.coin_ids.extend(coin_ids.iter().copied());
        subscriptions.add_coin_subscriptions(peer, coin_ids);
    }

    Reply::message(&RespondCoinState {
        coin_ids: request.coin_ids,
        coin_states,
    })
}

fn request_puzzle_state<B: Backend>(
    peer: SocketAddr,
    request: RequestPuzzleState,
    config: &PeerSimulatorConfig,
    simulator: &B,
    subscriptions: &mut Subscriptions,
    new_subscriptions: &mut NewSubscriptions,
) -> Result<Reply, PeerSimulatorError> {
    if let Some(previous_height) = request.previous_height {
        if Some(request.header_hash) != simulator.header_hash_at(previous_height) {
            return Reply::message(&RejectPuzzleState::new(RejectStateReason::Reorg));
        }
    } else if request.header_hash != config.genesis_challenge {
        return Reply::message(&RejectPuzzleState::new(RejectStateReason::Reorg));
    }

    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();
    let min_height = request.previous_height.map_or(0, |height| height + 1);
    let subscription_count = subscriptions.subscription_count(peer);

    if subscription_count + puzzle_hashes.len() > config.max_subscriptions
        && request.subscribe_when_finished
    {
        return Reply::message(&RejectPuzzleState::new(
            RejectStateReason::ExceededSubscriptionLimit,
        ));
    }

    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();

    let mut coin_states: Vec<CoinState> = simulator
        .coin_states_by_puzzle_hashes(&puzzle_hashes, request.filters.include_hinted)
        .into_iter()
        .filter(|cs| {
            if cs.spent_height.is_none() && !request.filters.include_unspent {
                return false;
            }

            if cs.spent_height.is_some() && !request.filters.include_spent {
                return false;
            }

            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= min_height
        })
        .sorted_by_key(|cs| u32::max(cs.created_height.unwrap_or(0), cs.spent_height.unwrap_or(0)))
        .take(config.max_response_coins + 1)
        .collect();

    let next_height = if coin_states.len() > config.puzzle_state_batch_size {
        coin_states
            .last()
            .map(|cs| u32::max(cs.created_height.unwrap_or(0), cs.spent_height.unwrap_or(0)))
    } else {
        None
    };

    if let Some(next_height) = next_height {
        while coin_states.last().is_some_and(|cs| {
            u32::max(cs.created_height.unwrap_or(0), cs.spent_height.unwrap_or(0)) == next_height
        }) {
            coin_states.pop();
        }
    }

    if request.subscribe_when_finished && next_height.is_none() {
        new_subscriptions
            .puzzle_hashes
            .extend(puzzle_hashes.iter().copied());
        subscriptions.add_puzzle_subscriptions(peer, puzzle_hashes);
    }

    let height = next_height.unwrap_or(simulator.peak_height());

    Reply::message(&RespondPuzzleState {
        height,
        header_hash: simulator.header_hash_at(height).unwrap(),
        puzzle_hashes: request.puzzle_hashes,
        coin_states,
        is_finished: next_height.is_none(),
    })
}

fn request_remove_coin_subscriptions(
    peer: SocketAddr,
    request: RequestRemoveCoinSubscriptions,
    subscriptions: &mut Subscriptions,
) -> Result<Bytes, PeerSimulatorError> {
    let coin_ids = if let Some(coin_ids) = request.coin_ids {
        subscriptions.remove_coin_subscriptions(peer, &coin_ids)
    } else {
        subscriptions.remove_all_coin_subscriptions(peer)
    };

    Ok(RespondRemoveCoinSubscriptions { coin_ids }
        .to_bytes()?
        .into())
}

fn request_remove_puzzle_subscriptions(
    peer: SocketAddr,
    request: RequestRemovePuzzleSubscriptions,
    subscriptions: &mut Subscriptions,
) -> Result<Bytes, PeerSimulatorError> {
    let puzzle_hashes = if let Some(puzzle_hashes) = request.puzzle_hashes {
        subscriptions.remove_puzzle_subscriptions(peer, &puzzle_hashes)
    } else {
        subscriptions.remove_all_puzzle_subscriptions(peer)
    };

    Ok(RespondRemovePuzzleSubscriptions { puzzle_hashes }
        .to_bytes()?
        .into())
}
