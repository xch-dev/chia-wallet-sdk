use std::{net::SocketAddr, sync::Arc};

use chia_consensus::gen::validation_error::{ErrorCode, ValidationErr};
use chia_protocol::{
    Bytes, Bytes32, CoinState, CoinStateUpdate, Message, NewPeakWallet, ProtocolMessageTypes,
    RegisterForCoinUpdates, RegisterForPhUpdates, RejectCoinState, RejectPuzzleSolution,
    RejectPuzzleState, RejectStateReason, RequestChildren, RequestCoinState, RequestPuzzleSolution,
    RequestPuzzleState, RespondChildren, RespondCoinState, RespondPuzzleSolution,
    RespondPuzzleState, RespondToCoinUpdates, RespondToPhUpdates, SendTransaction, TransactionAck,
};
use chia_traits::Streamable;
use clvmr::NodePtr;
use futures_channel::mpsc;
use futures_util::{SinkExt, StreamExt};
use indexmap::IndexSet;
use itertools::Itertools;
use tokio::{
    net::TcpStream,
    sync::{Mutex, MutexGuard},
};
use tokio_tungstenite::{tungstenite::Message as WsMessage, WebSocketStream};

use super::{
    peer_map::Ws,
    simulator_config::SimulatorConfig,
    simulator_data::{new_transaction, SimulatorData},
    simulator_error::SimulatorError,
    PeerMap,
};

pub(crate) async fn ws_connection(
    peer_map: PeerMap,
    ws: WebSocketStream<TcpStream>,
    addr: SocketAddr,
    config: Arc<SimulatorConfig>,
    data: Arc<Mutex<SimulatorData>>,
) {
    let (tx, mut rx) = mpsc::unbounded();
    peer_map.insert(addr, tx.clone()).await;

    let (mut sink, mut stream) = ws.split();

    tokio::spawn(async move {
        while let Some(message) = rx.next().await {
            if let Err(error) = sink.send(message).await {
                log::error!("error sending message to peer: {}", error);
                continue;
            }
        }
    });

    while let Some(message) = stream.next().await {
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                log::info!("received error from stream: {:?}", error);
                break;
            }
        };

        if let Err(error) =
            handle_message(peer_map.clone(), &config, &data, message, addr, tx.clone()).await
        {
            log::error!("error handling message: {}", error);
            break;
        }
    }

    peer_map.remove(addr).await;
}

async fn handle_message(
    peer_map: PeerMap,
    config: &SimulatorConfig,
    data: &Mutex<SimulatorData>,
    message: WsMessage,
    addr: SocketAddr,
    mut ws: Ws,
) -> Result<(), SimulatorError> {
    let request = Message::from_bytes(&message.into_data())?;
    let data = data.lock().await;

    let (response_type, response_data) = match request.msg_type {
        ProtocolMessageTypes::SendTransaction => {
            let request = SendTransaction::from_bytes(&request.data)?;
            let response = send_transaction(peer_map, request, config, data).await?;
            (ProtocolMessageTypes::TransactionAck, response)
        }
        ProtocolMessageTypes::RegisterForCoinUpdates => {
            let request = RegisterForCoinUpdates::from_bytes(&request.data)?;
            let response = register_for_coin_updates(addr, request, data)?;
            (ProtocolMessageTypes::RespondToCoinUpdates, response)
        }
        ProtocolMessageTypes::RegisterForPhUpdates => {
            let request = RegisterForPhUpdates::from_bytes(&request.data)?;
            let response = register_for_ph_updates(addr, request, data)?;
            (ProtocolMessageTypes::RespondToPhUpdates, response)
        }
        ProtocolMessageTypes::RequestPuzzleSolution => {
            let request = RequestPuzzleSolution::from_bytes(&request.data)?;
            let response = request_puzzle_solution(&request, &data)?;
            (ProtocolMessageTypes::RespondPuzzleSolution, response)
        }
        ProtocolMessageTypes::RequestChildren => {
            let request = RequestChildren::from_bytes(&request.data)?;
            let response = request_children(&request, &data)?;
            (ProtocolMessageTypes::RespondChildren, response)
        }
        ProtocolMessageTypes::RequestCoinState => {
            let request = RequestCoinState::from_bytes(&request.data)?;
            let response = request_coin_state(addr, request, config, data)?;
            (ProtocolMessageTypes::RespondCoinState, response)
        }
        ProtocolMessageTypes::RequestPuzzleState => {
            let request = RequestPuzzleState::from_bytes(&request.data)?;
            let response = request_puzzle_state(addr, request, config, data)?;
            (ProtocolMessageTypes::RespondPuzzleState, response)
        }
        message_type => {
            return Err(SimulatorError::UnsupportedMessage(message_type));
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

async fn send_transaction(
    peer_map: PeerMap,
    request: SendTransaction,
    config: &SimulatorConfig,
    mut data: MutexGuard<'_, SimulatorData>,
) -> Result<Bytes, SimulatorError> {
    let transaction_id = request.transaction.name();

    let updates = match new_transaction(config, &mut data, request.transaction, 6_600_000_000) {
        Ok(updates) => updates,
        Err(error) => {
            log::error!("error processing transaction: {:?}", &error);

            let validation_error = match error {
                SimulatorError::Validation(validation_error) => validation_error,
                _ => ValidationErr(NodePtr::NIL, ErrorCode::Unknown),
            };

            return Ok(TransactionAck::new(
                transaction_id,
                3,
                Some(format!("{validation_error:?}")),
            )
            .to_bytes()?
            .into());
        }
    };

    let header_hash = data.header_hash(data.height());

    let new_peak = Message {
        msg_type: ProtocolMessageTypes::NewPeakWallet,
        id: None,
        data: NewPeakWallet::new(header_hash, data.height(), 0, data.height())
            .to_bytes()
            .unwrap()
            .into(),
    }
    .to_bytes()?;

    // Send updates to peers.
    for (addr, mut peer) in peer_map.peers().await {
        peer.send(new_peak.clone().into()).await.unwrap();

        let Some(peer_updates) = updates.get(&addr).cloned() else {
            continue;
        };

        let update = Message {
            msg_type: ProtocolMessageTypes::CoinStateUpdate,
            id: None,
            data: CoinStateUpdate::new(
                data.height(),
                data.height(),
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
    mut data: MutexGuard<'_, SimulatorData>,
) -> Result<Bytes, SimulatorError> {
    let coin_ids: IndexSet<Bytes32> = request.coin_ids.iter().copied().collect();

    let coin_states: Vec<CoinState> = data
        .lookup_coin_ids(&coin_ids)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= request.min_height
        })
        .collect();

    data.add_coin_subscriptions(peer, coin_ids);

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
    mut data: MutexGuard<'_, SimulatorData>,
) -> Result<Bytes, SimulatorError> {
    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();

    let coin_states: Vec<CoinState> = data
        .lookup_puzzle_hashes(puzzle_hashes.clone(), true)
        .into_iter()
        .filter(|cs| {
            let created_height = cs.created_height.unwrap_or(0);
            let spent_height = cs.spent_height.unwrap_or(0);
            let height = u32::max(created_height, spent_height);
            height >= request.min_height
        })
        .collect();

    data.add_puzzle_subscriptions(peer, puzzle_hashes);

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
    data: &MutexGuard<'_, SimulatorData>,
) -> Result<Bytes, SimulatorError> {
    let reject = RejectPuzzleSolution {
        coin_name: request.coin_name,
        height: request.height,
    }
    .to_bytes()?
    .into();

    let Some(puzzle_solution) = data.puzzle_and_solution(request.coin_name) else {
        return Ok(reject);
    };

    if puzzle_solution.height != request.height {
        return Ok(reject);
    }

    Ok(RespondPuzzleSolution::new(puzzle_solution)
        .to_bytes()?
        .into())
}

fn request_children(
    request: &RequestChildren,
    data: &MutexGuard<'_, SimulatorData>,
) -> Result<Bytes, SimulatorError> {
    Ok(RespondChildren::new(data.children(request.coin_name))
        .to_bytes()?
        .into())
}

fn request_coin_state(
    peer: SocketAddr,
    request: RequestCoinState,
    config: &SimulatorConfig,
    mut data: MutexGuard<'_, SimulatorData>,
) -> Result<Bytes, SimulatorError> {
    if (request.previous_height.is_some()
        && request.header_hash != data.header_hash(request.previous_height.unwrap()))
        || (request.previous_height.is_none() && request.header_hash != config.genesis_challenge)
    {
        return Ok(RejectCoinState::new(RejectStateReason::Reorg)
            .to_bytes()?
            .into());
    }

    let coin_ids: IndexSet<Bytes32> = request.coin_ids.iter().copied().collect();
    let min_height = request.previous_height.map_or(0, |height| height + 1);
    let subscription_count = data.subscription_count(peer);

    if subscription_count + coin_ids.len() > config.max_subscriptions && request.subscribe {
        return Ok(
            RejectCoinState::new(RejectStateReason::ExceededSubscriptionLimit)
                .to_bytes()?
                .into(),
        );
    }

    let coin_states: Vec<CoinState> = data
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
        data.add_coin_subscriptions(peer, coin_ids);
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
    mut data: MutexGuard<'_, SimulatorData>,
) -> Result<Bytes, SimulatorError> {
    if (request.previous_height.is_some()
        && request.header_hash != data.header_hash(request.previous_height.unwrap()))
        || (request.previous_height.is_none() && request.header_hash != config.genesis_challenge)
    {
        return Ok(RejectPuzzleState::new(RejectStateReason::Reorg)
            .to_bytes()?
            .into());
    }

    let puzzle_hashes: IndexSet<Bytes32> = request.puzzle_hashes.iter().copied().collect();
    let min_height = request.previous_height.map_or(0, |height| height + 1);
    let subscription_count = data.subscription_count(peer);

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

    let mut coin_states: Vec<CoinState> = data
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
        while coin_states.last().map_or(false, |cs| {
            u32::max(cs.created_height.unwrap_or(0), cs.spent_height.unwrap_or(0)) == next_height
        }) {
            coin_states.pop();
        }
    }

    if request.subscribe_when_finished && next_height.is_none() {
        data.add_puzzle_subscriptions(peer, puzzle_hashes);
    }

    let height = next_height.unwrap_or(data.height());

    Ok(RespondPuzzleState {
        height,
        header_hash: data.header_hash(height),
        puzzle_hashes: request.puzzle_hashes,
        coin_states,
        is_finished: next_height.is_none(),
    }
    .to_bytes()?
    .into())
}
