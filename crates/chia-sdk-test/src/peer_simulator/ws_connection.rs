use std::{net::SocketAddr, sync::Arc};

use chia_consensus::validation_error::{ErrorCode, ValidationErr};
use chia_protocol::{
    Bytes, Bytes32, CoinState, CoinStateUpdate, Message, NewPeakWallet, ProtocolMessageTypes,
    PuzzleSolutionResponse, RegisterForCoinUpdates, RegisterForPhUpdates, RejectCoinState,
    RejectPuzzleSolution, RejectPuzzleState, RejectStateReason, RequestChildren, RequestCoinState,
    RequestPuzzleSolution, RequestPuzzleState, RequestRemoveCoinSubscriptions,
    RequestRemovePuzzleSubscriptions, RespondChildren, RespondCoinState, RespondPuzzleSolution,
    RespondPuzzleState, RespondRemoveCoinSubscriptions, RespondRemovePuzzleSubscriptions,
    RespondToCoinUpdates, RespondToPhUpdates, SendTransaction, SpendBundle, TransactionAck,
};
use chia_traits::Streamable;
use clvmr::NodePtr;
use futures_channel::mpsc::{self, UnboundedSender};
use futures_util::{SinkExt, StreamExt};
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use tokio::{
    net::TcpStream,
    sync::{Mutex, MutexGuard},
};
use tokio_tungstenite::{
    tungstenite::{self, Message as WsMessage},
    WebSocketStream,
};

use crate::{Simulator, SimulatorError};

use super::{
    error::PeerSimulatorError, peer_map::Ws, simulator_config::SimulatorConfig,
    subscriptions::Subscriptions, PeerMap,
};

pub(crate) async fn ws_connection(
    peer_map: PeerMap,
    ws: WebSocketStream<TcpStream>,
    addr: SocketAddr,
    config: Arc<SimulatorConfig>,
    simulator: Arc<Mutex<Simulator>>,
    subscriptions: Arc<Mutex<Subscriptions>>,
) {
    let (mut tx, mut rx) = mpsc::unbounded();

    if let Err(error) = handle_initial_peak(&mut tx, &simulator).await {
        tracing::error!("error sending initial peak: {}", error);
        return;
    }

    peer_map.insert(addr, tx.clone()).await;

    let (mut sink, mut stream) = ws.split();

    tokio::spawn(async move {
        while let Some(message) = rx.next().await {
            if let Err(error) = sink.send(message).await {
                tracing::error!("error sending message to peer: {}", error);
                continue;
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

        if let Err(error) = handle_message(
            peer_map.clone(),
            &config,
            &simulator,
            &subscriptions,
            message,
            addr,
            tx.clone(),
        )
        .await
        {
            tracing::error!("error handling message: {}", error);
            break;
        }
    }

    peer_map.remove(addr).await;
}

async fn handle_initial_peak(
    tx: &mut UnboundedSender<tungstenite::Message>,
    sim: &Mutex<Simulator>,
) -> Result<(), PeerSimulatorError> {
    let (header_hash, height) = {
        let sim = sim.lock().await;
        (sim.header_hash(), sim.height())
    };

    tx.send(
        Message {
            msg_type: ProtocolMessageTypes::NewPeakWallet,
            id: None,
            data: NewPeakWallet::new(header_hash, height, 0, height)
                .to_bytes()
                .unwrap()
                .into(),
        }
        .to_bytes()?
        .into(),
    )
    .await?;

    Ok(())
}

async fn handle_message(
    peer_map: PeerMap,
    config: &SimulatorConfig,
    simulator: &Mutex<Simulator>,
    subscriptions: &Mutex<Subscriptions>,
    message: WsMessage,
    addr: SocketAddr,
    mut ws: Ws,
) -> Result<(), PeerSimulatorError> {
    let request = Message::from_bytes(&message.into_data())?;
    let simulator = simulator.lock().await;

    let (response_type, response_data) = match request.msg_type {
        ProtocolMessageTypes::SendTransaction => {
            let request = SendTransaction::from_bytes(&request.data)?;
            let subscriptions = subscriptions.lock().await;
            let response = send_transaction(peer_map, request, simulator, subscriptions).await?;
            (ProtocolMessageTypes::TransactionAck, response)
        }
        ProtocolMessageTypes::RegisterForCoinUpdates => {
            let request = RegisterForCoinUpdates::from_bytes(&request.data)?;
            let subscriptions = subscriptions.lock().await;
            let response = register_for_coin_updates(addr, request, &simulator, subscriptions)?;
            (ProtocolMessageTypes::RespondToCoinUpdates, response)
        }
        ProtocolMessageTypes::RegisterForPhUpdates => {
            let request = RegisterForPhUpdates::from_bytes(&request.data)?;
            let subscriptions = subscriptions.lock().await;
            let response = register_for_ph_updates(addr, request, &simulator, subscriptions)?;
            (ProtocolMessageTypes::RespondToPhUpdates, response)
        }
        ProtocolMessageTypes::RequestPuzzleSolution => {
            let request = RequestPuzzleSolution::from_bytes(&request.data)?;
            let response = request_puzzle_solution(&request, &simulator)?;
            (ProtocolMessageTypes::RespondPuzzleSolution, response)
        }
        ProtocolMessageTypes::RequestChildren => {
            let request = RequestChildren::from_bytes(&request.data)?;
            let response = request_children(&request, &simulator)?;
            (ProtocolMessageTypes::RespondChildren, response)
        }
        ProtocolMessageTypes::RequestCoinState => {
            let request = RequestCoinState::from_bytes(&request.data)?;
            let subscriptions = subscriptions.lock().await;
            let response = request_coin_state(addr, request, config, &simulator, subscriptions)?;
            (ProtocolMessageTypes::RespondCoinState, response)
        }
        ProtocolMessageTypes::RequestPuzzleState => {
            let request = RequestPuzzleState::from_bytes(&request.data)?;
            let subscriptions = subscriptions.lock().await;
            let response = request_puzzle_state(addr, request, config, &simulator, subscriptions)?;
            (ProtocolMessageTypes::RespondPuzzleState, response)
        }
        ProtocolMessageTypes::RequestRemoveCoinSubscriptions => {
            let request = RequestRemoveCoinSubscriptions::from_bytes(&request.data)?;
            let mut subscriptions = subscriptions.lock().await;
            let response = request_remove_coin_subscriptions(addr, request, &mut subscriptions)?;
            (
                ProtocolMessageTypes::RespondRemoveCoinSubscriptions,
                response,
            )
        }
        ProtocolMessageTypes::RequestRemovePuzzleSubscriptions => {
            let request = RequestRemovePuzzleSubscriptions::from_bytes(&request.data)?;
            let mut subscriptions = subscriptions.lock().await;
            let response = request_remove_puzzle_subscriptions(addr, request, &mut subscriptions)?;
            (
                ProtocolMessageTypes::RespondRemovePuzzleSubscriptions,
                response,
            )
        }
        message_type => {
            return Err(PeerSimulatorError::UnsupportedMessage(message_type));
        }
    };

    let message = Message {
        msg_type: response_type,
        data: response_data,
        id: request.id,
    }
    .to_bytes()?;

    ws.send(message.into()).await?;

    Ok(())
}

fn new_transaction(
    simulator: &mut MutexGuard<'_, Simulator>,
    subscriptions: &mut MutexGuard<'_, Subscriptions>,
    spend_bundle: SpendBundle,
) -> Result<IndexMap<SocketAddr, IndexSet<CoinState>>, PeerSimulatorError> {
    let updates = simulator.new_transaction(spend_bundle)?;
    let peers = subscriptions.peers();

    let mut peer_updates = IndexMap::new();

    // Send updates to peers.
    for peer in peers {
        let mut coin_states = IndexSet::new();

        let coin_subscriptions = subscriptions
            .coin_subscriptions(peer)
            .cloned()
            .unwrap_or_default();

        let puzzle_subscriptions = subscriptions
            .puzzle_subscriptions(peer)
            .cloned()
            .unwrap_or_default();

        for &coin_id in updates.keys() {
            let Some(coin_state) = simulator.coin_state(coin_id) else {
                continue;
            };

            if coin_subscriptions.contains(&coin_id)
                || puzzle_subscriptions.contains(&coin_state.coin.puzzle_hash)
            {
                coin_states.insert(coin_state);
            }
        }

        for &hint in &puzzle_subscriptions {
            let coin_ids = simulator.hinted_coins(hint);

            for coin_id in coin_ids {
                if updates.contains_key(&coin_id) {
                    coin_states.extend(simulator.coin_state(coin_id));
                }
            }
        }

        if coin_states.is_empty() {
            continue;
        };

        peer_updates.insert(peer, coin_states);
    }

    Ok(peer_updates)
}

async fn send_transaction(
    peer_map: PeerMap,
    request: SendTransaction,
    mut simulator: MutexGuard<'_, Simulator>,
    mut subscriptions: MutexGuard<'_, Subscriptions>,
) -> Result<Bytes, PeerSimulatorError> {
    let transaction_id = request.transaction.name();

    let updates = match new_transaction(&mut simulator, &mut subscriptions, request.transaction) {
        Ok(updates) => updates,
        Err(error) => {
            tracing::error!("error processing transaction: {:?}", &error);

            let error_code = match error {
                PeerSimulatorError::Simulator(SimulatorError::Validation(error_code)) => error_code,
                _ => ErrorCode::Unknown,
            };

            return Ok(TransactionAck::new(
                transaction_id,
                3,
                Some(format!("{:?}", ValidationErr(NodePtr::NIL, error_code))),
            )
            .to_bytes()?
            .into());
        }
    };

    let header_hash = simulator.header_hash();

    let new_peak = Message {
        msg_type: ProtocolMessageTypes::NewPeakWallet,
        id: None,
        data: NewPeakWallet::new(header_hash, simulator.height(), 0, simulator.height())
            .to_bytes()
            .unwrap()
            .into(),
    }
    .to_bytes()?;

    // Send updates to peers.
    for (addr, mut peer) in peer_map.peers().await {
        peer.send(new_peak.clone().into()).await?;

        let Some(peer_updates) = updates.get(&addr).cloned() else {
            continue;
        };

        let update = Message {
            msg_type: ProtocolMessageTypes::CoinStateUpdate,
            id: None,
            data: CoinStateUpdate::new(
                simulator.height(),
                simulator.height(),
                header_hash,
                peer_updates.into_iter().collect(),
            )
            .to_bytes()
            .unwrap()
            .into(),
        }
        .to_bytes()?;

        peer.send(update.into()).await?;
    }

    Ok(TransactionAck::new(transaction_id, 1, None)
        .to_bytes()?
        .into())
}

fn register_for_coin_updates(
    peer: SocketAddr,
    request: RegisterForCoinUpdates,
    simulator: &MutexGuard<'_, Simulator>,
    mut subscriptions: MutexGuard<'_, Subscriptions>,
) -> Result<Bytes, PeerSimulatorError> {
    let coin_ids: IndexSet<Bytes32> = request.coin_ids.iter().copied().collect();

    let coin_states: Vec<CoinState> = simulator
        .lookup_coin_ids(&coin_ids)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= request.min_height
        })
        .collect();

    subscriptions.add_coin_subscriptions(peer, coin_ids);

    Ok(RespondToCoinUpdates {
        coin_ids: request.coin_ids,
        min_height: request.min_height,
        coin_states,
    }
    .to_bytes()?
    .into())
}

fn register_for_ph_updates(
    peer: SocketAddr,
    request: RegisterForPhUpdates,
    simulator: &MutexGuard<'_, Simulator>,
    mut subscriptions: MutexGuard<'_, Subscriptions>,
) -> Result<Bytes, PeerSimulatorError> {
    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();

    let coin_states: Vec<CoinState> = simulator
        .lookup_puzzle_hashes(puzzle_hashes.clone(), true)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= request.min_height
        })
        .collect();

    subscriptions.add_puzzle_subscriptions(peer, puzzle_hashes);

    Ok(RespondToPhUpdates {
        puzzle_hashes: request.puzzle_hashes,
        min_height: request.min_height,
        coin_states,
    }
    .to_bytes()?
    .into())
}

fn request_puzzle_solution(
    request: &RequestPuzzleSolution,
    simulator: &MutexGuard<'_, Simulator>,
) -> Result<Bytes, PeerSimulatorError> {
    let reject = RejectPuzzleSolution {
        coin_name: request.coin_name,
        height: request.height,
    }
    .to_bytes()?
    .into();

    let Some(coin_state) = simulator.coin_state(request.coin_name) else {
        return Ok(reject);
    };

    if coin_state.spent_height != Some(request.height) {
        return Ok(reject);
    }

    let Some(puzzle_reveal) = simulator.puzzle_reveal(request.coin_name) else {
        return Ok(reject);
    };

    let Some(solution) = simulator.solution(request.coin_name) else {
        return Ok(reject);
    };

    Ok(RespondPuzzleSolution::new(PuzzleSolutionResponse::new(
        request.coin_name,
        request.height,
        puzzle_reveal,
        solution,
    ))
    .to_bytes()?
    .into())
}

fn request_children(
    request: &RequestChildren,
    simulator: &MutexGuard<'_, Simulator>,
) -> Result<Bytes, PeerSimulatorError> {
    Ok(RespondChildren::new(simulator.children(request.coin_name))
        .to_bytes()?
        .into())
}

fn request_coin_state(
    peer: SocketAddr,
    request: RequestCoinState,
    config: &SimulatorConfig,
    simulator: &MutexGuard<'_, Simulator>,
    mut subscriptions: MutexGuard<'_, Subscriptions>,
) -> Result<Bytes, PeerSimulatorError> {
    if let Some(previous_height) = request.previous_height {
        if Some(request.header_hash) != simulator.header_hash_of(previous_height) {
            return Ok(RejectCoinState::new(RejectStateReason::Reorg)
                .to_bytes()?
                .into());
        }
    } else if request.header_hash != config.constants.genesis_challenge {
        return Ok(RejectCoinState::new(RejectStateReason::Reorg)
            .to_bytes()?
            .into());
    }

    let coin_ids: IndexSet<Bytes32> = request.coin_ids.iter().copied().collect();
    let min_height = request.previous_height.map_or(0, |height| height + 1);
    let subscription_count = subscriptions.subscription_count(peer);

    if subscription_count + coin_ids.len() > config.max_subscriptions && request.subscribe {
        return Ok(
            RejectCoinState::new(RejectStateReason::ExceededSubscriptionLimit)
                .to_bytes()?
                .into(),
        );
    }

    let coin_states: Vec<CoinState> = simulator
        .lookup_coin_ids(&coin_ids)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= min_height
        })
        .collect();

    if request.subscribe {
        subscriptions.add_coin_subscriptions(peer, coin_ids);
    }

    Ok(RespondCoinState {
        coin_ids: request.coin_ids,
        coin_states,
    }
    .to_bytes()?
    .into())
}

fn request_puzzle_state(
    peer: SocketAddr,
    request: RequestPuzzleState,
    config: &SimulatorConfig,
    simulator: &MutexGuard<'_, Simulator>,
    mut subscriptions: MutexGuard<'_, Subscriptions>,
) -> Result<Bytes, PeerSimulatorError> {
    if let Some(previous_height) = request.previous_height {
        if Some(request.header_hash) != simulator.header_hash_of(previous_height) {
            return Ok(RejectPuzzleState::new(RejectStateReason::Reorg)
                .to_bytes()?
                .into());
        }
    } else if request.header_hash != config.constants.genesis_challenge {
        return Ok(RejectPuzzleState::new(RejectStateReason::Reorg)
            .to_bytes()?
            .into());
    }

    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();
    let min_height = request.previous_height.map_or(0, |height| height + 1);
    let subscription_count = subscriptions.subscription_count(peer);

    if subscription_count + puzzle_hashes.len() > config.max_subscriptions
        && request.subscribe_when_finished
    {
        return Ok(
            RejectPuzzleState::new(RejectStateReason::ExceededSubscriptionLimit)
                .to_bytes()?
                .into(),
        );
    }

    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();

    let mut coin_states: Vec<CoinState> = simulator
        .lookup_puzzle_hashes(puzzle_hashes.clone(), request.filters.include_hinted)
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
        subscriptions.add_puzzle_subscriptions(peer, puzzle_hashes);
    }

    let height = next_height.unwrap_or(simulator.height());

    Ok(RespondPuzzleState {
        height,
        header_hash: simulator.header_hash_of(height).unwrap(),
        puzzle_hashes: request.puzzle_hashes,
        coin_states,
        is_finished: next_height.is_none(),
    }
    .to_bytes()?
    .into())
}

fn request_remove_coin_subscriptions(
    peer: SocketAddr,
    request: RequestRemoveCoinSubscriptions,
    subscriptions: &mut MutexGuard<'_, Subscriptions>,
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
    subscriptions: &mut MutexGuard<'_, Subscriptions>,
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
