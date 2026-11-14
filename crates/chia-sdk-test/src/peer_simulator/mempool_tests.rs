//! Tests for the mempool mode of the [`PeerSimulator`] and its CHIP-26 mempool updates, over a
//! real websocket connection.

use std::time::Duration;

use chia_bls::Signature;
use chia_protocol::{
    Bytes32, Coin, CoinSpend, CoinStateFilters, CoinStateUpdate, Handshake, MempoolItemsAdded,
    MempoolItemsRemoved, MempoolRemoveReason, Message, NewPeakWallet, NodeType, Program,
    ProtocolMessageTypes, RemovedMempoolItem, SpendBundle,
};
use chia_sdk_client::Peer;
use chia_sdk_types::conditions::{AggSigMe, CreateCoin, Memos};
use chia_traits::Streamable;
use tokio::{sync::mpsc, time::timeout};

use crate::{BlsPair, FullNodeSimulator, sign_transaction, to_program, to_puzzle};

use super::PeerSimulator;

/// The CHIP-26 `MEMPOOL_UPDATES` capability.
const MEMPOOL_UPDATES: u16 = 5;

/// Connects a peer that advertises the base capabilities and, if `mempool_updates` is set,
/// `MEMPOOL_UPDATES`. The handshake is done by hand rather than with a helper, so this also
/// checks the simulator's side of it.
async fn connect_with_handshake(
    sim: &PeerSimulator<FullNodeSimulator>,
    mempool_updates: bool,
) -> anyhow::Result<(Peer, mpsc::Receiver<Message>)> {
    let (peer, mut receiver) = sim.connect_raw().await?;

    let mut capabilities = vec![
        (1, "1".to_string()),
        (2, "1".to_string()),
        (3, "1".to_string()),
    ];
    if mempool_updates {
        capabilities.push((MEMPOOL_UPDATES, "1".to_string()));
    }

    peer.send(Handshake {
        network_id: "testnet11".to_string(),
        protocol_version: "0.0.37".to_string(),
        software_version: "0.0.0".to_string(),
        server_port: 0,
        node_type: NodeType::Wallet,
        capabilities,
    })
    .await?;

    let message = receiver.recv().await.expect("expected NewPeakWallet");
    assert_eq!(message.msg_type, ProtocolMessageTypes::NewPeakWallet);

    let message = receiver.recv().await.expect("expected Handshake");
    assert_eq!(message.msg_type, ProtocolMessageTypes::Handshake);
    let handshake = Handshake::from_bytes(&message.data)?;
    assert_eq!(handshake.node_type, NodeType::FullNode);
    assert_eq!(handshake.network_id, "testnet11");
    assert!(
        handshake
            .capabilities
            .contains(&(MEMPOOL_UPDATES, "1".to_string()))
    );

    Ok((peer, receiver))
}

/// Makes a round trip on the connection, so every message the simulator sent before it has
/// been received.
async fn sync(peer: &Peer) -> anyhow::Result<()> {
    peer.request_children(Bytes32::default()).await?;
    Ok(())
}

/// Returns the messages that have been received so far, after a [`sync`].
async fn received(
    peer: &Peer,
    receiver: &mut mpsc::Receiver<Message>,
) -> anyhow::Result<Vec<Message>> {
    sync(peer).await?;
    let mut messages = Vec::new();
    while let Ok(message) = receiver.try_recv() {
        messages.push(message);
    }
    Ok(messages)
}

fn mempool_messages(messages: &[Message]) -> Vec<&Message> {
    messages
        .iter()
        .filter(|message| {
            matches!(
                message.msg_type,
                ProtocolMessageTypes::MempoolItemsAdded | ProtocolMessageTypes::MempoolItemsRemoved
            )
        })
        .collect()
}

fn added(message: &Message) -> Vec<Bytes32> {
    assert_eq!(message.msg_type, ProtocolMessageTypes::MempoolItemsAdded);
    MempoolItemsAdded::from_bytes(&message.data)
        .unwrap()
        .transaction_ids
}

fn removed(message: &Message) -> Vec<RemovedMempoolItem> {
    assert_eq!(message.msg_type, ProtocolMessageTypes::MempoolItemsRemoved);
    MempoolItemsRemoved::from_bytes(&message.data)
        .unwrap()
        .removed_items
}

/// A spend of a coin locked to the puzzle `1` that creates the given children.
fn spend(
    coin: Coin,
    puzzle_reveal: &Program,
    children: &[(Bytes32, u64, Option<Bytes32>)],
) -> anyhow::Result<SpendBundle> {
    let conditions: Vec<CreateCoin<Vec<Bytes32>>> = children
        .iter()
        .map(|(puzzle_hash, amount, hint)| {
            CreateCoin::new(
                *puzzle_hash,
                *amount,
                hint.map_or(Memos::None, |hint| Memos::Some(vec![hint])),
            )
        })
        .collect();
    Ok(SpendBundle::new(
        vec![CoinSpend::new(
            coin,
            puzzle_reveal.clone(),
            to_program(conditions)?,
        )],
        Signature::default(),
    ))
}

#[tokio::test]
async fn test_transaction_waits_in_mempool_until_farmed() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    let child = Coin::new(coin.coin_id(), puzzle_hash, 900);
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;
    let height = sim.lock().await.height();

    let spend_bundle = spend(coin, &puzzle_reveal, &[(puzzle_hash, 900, None)])?;
    let transaction_id = spend_bundle.name();
    let ack = peer.send_transaction(spend_bundle.clone()).await?;
    assert_eq!(ack.status, 1);
    assert_eq!(ack.txid, transaction_id);

    // The transaction is pending: no block was made, and the coin is still unspent.
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![transaction_id]);
    assert_eq!(sim.lock().await.height(), height);
    let coin_state = peer
        .request_coin_state(
            vec![coin.coin_id()],
            None,
            sim.config().genesis_challenge,
            false,
        )
        .await?
        .unwrap()
        .coin_states[0];
    assert_eq!(coin_state.spent_height, None);

    // Sending it again is accepted without another update.
    assert_eq!(peer.send_transaction(spend_bundle).await?.status, 1);
    assert!(received(&peer, &mut receiver).await?.is_empty());

    let records = sim.farm_block(1).await;
    let header_hash = records[0].header_hash;
    let height = height + 1;

    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 3);

    assert_eq!(messages[0].msg_type, ProtocolMessageTypes::NewPeakWallet);
    let new_peak = NewPeakWallet::from_bytes(&messages[0].data)?;
    assert_eq!(new_peak.header_hash, header_hash);
    assert_eq!(new_peak.height, height);

    assert_eq!(messages[1].msg_type, ProtocolMessageTypes::CoinStateUpdate);
    let update = CoinStateUpdate::from_bytes(&messages[1].data)?;
    assert_eq!(update.height, height);
    assert_eq!(update.peak_hash, header_hash);
    assert_eq!(update.items.len(), 2);
    let spent = update
        .items
        .iter()
        .find(|cs| cs.coin == coin)
        .expect("missing spent coin");
    assert_eq!(spent.spent_height, Some(height));
    let created = update
        .items
        .iter()
        .find(|cs| cs.coin == child)
        .expect("missing created coin");
    assert_eq!(created.created_height, Some(height));
    assert_eq!(created.spent_height, None);

    assert_eq!(
        removed(&messages[2]),
        vec![RemovedMempoolItem::new(
            transaction_id,
            MempoolRemoveReason::BlockInclusion
        )]
    );

    Ok(())
}

#[tokio::test]
async fn test_mempool_updates_require_the_capability() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (sender, mut sender_receiver) = connect_with_handshake(&sim, true).await?;
    let (without_capability, mut without_capability_receiver) =
        connect_with_handshake(&sim, false).await?;
    let (without_handshake, mut without_handshake_receiver) = sim.connect_split().await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    for peer in [&sender, &without_capability, &without_handshake] {
        peer.register_for_coin_updates(vec![coin.coin_id()], 0)
            .await?;
    }

    let spend_bundle = spend(coin, &puzzle_reveal, &[])?;
    assert_eq!(sender.send_transaction(spend_bundle).await?.status, 1);
    sim.farm_block(1).await;

    let messages = received(&sender, &mut sender_receiver).await?;
    assert_eq!(mempool_messages(&messages).len(), 2);

    for (peer, receiver) in [
        (&without_capability, &mut without_capability_receiver),
        (&without_handshake, &mut without_handshake_receiver),
    ] {
        let messages = received(peer, receiver).await?;
        assert!(mempool_messages(&messages).is_empty());
        let types: Vec<_> = messages.iter().map(|message| message.msg_type).collect();
        assert_eq!(
            types,
            vec![
                ProtocolMessageTypes::NewPeakWallet,
                ProtocolMessageTypes::CoinStateUpdate
            ]
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_mempool_updates_only_match_subscriptions() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let hint = Bytes32::new([42; 32]);
    let other_puzzle_hash = Bytes32::new([7; 32]);

    // Subscribed to the hint only.
    peer.register_for_ph_updates(vec![hint], 0).await?;

    let unrelated = sim.lock().await.new_coin(puzzle_hash, 1);
    let unrelated_spend = spend(unrelated, &puzzle_reveal, &[(other_puzzle_hash, 1, None)])?;
    assert_eq!(peer.send_transaction(unrelated_spend).await?.status, 1);
    assert!(received(&peer, &mut receiver).await?.is_empty());

    let hinted = sim.lock().await.new_coin(puzzle_hash, 1);
    let hinted_spend = spend(
        hinted,
        &puzzle_reveal,
        &[(other_puzzle_hash, 1, Some(hint))],
    )?;
    let transaction_id = hinted_spend.name();
    assert_eq!(peer.send_transaction(hinted_spend).await?.status, 1);

    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![transaction_id]);

    // Only the hinted coin is in the coin state update, and only its transaction is removed.
    sim.farm_block(1).await;
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 3);
    let update = CoinStateUpdate::from_bytes(&messages[1].data)?;
    assert_eq!(update.items.len(), 1);
    assert_eq!(
        update.items[0].coin,
        Coin::new(hinted.coin_id(), other_puzzle_hash, 1)
    );
    assert_eq!(
        removed(&messages[2]),
        vec![RemovedMempoolItem::new(
            transaction_id,
            MempoolRemoveReason::BlockInclusion
        )]
    );

    Ok(())
}

#[tokio::test]
async fn test_replace_by_fee() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    peer.register_for_coin_updates(vec![coin.coin_id()], 0)
        .await?;

    let original = spend(coin, &puzzle_reveal, &[(puzzle_hash, 1_000, None)])?;
    let original_id = original.name();
    assert_eq!(peer.send_transaction(original).await?.status, 1);

    // The same fee isn't enough to replace it.
    let conflicting = spend(
        coin,
        &puzzle_reveal,
        &[(Bytes32::new([1; 32]), 1_000, None)],
    )?;
    let ack = peer.send_transaction(conflicting).await?;
    assert_eq!(ack.status, 3);
    assert_eq!(ack.error.as_deref(), Some("Err(MempoolConflict)"));

    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![original_id]);

    let replacement = spend(coin, &puzzle_reveal, &[(puzzle_hash, 999, None)])?;
    let replacement_id = replacement.name();
    assert_eq!(peer.send_transaction(replacement).await?.status, 1);

    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 2);
    assert_eq!(
        removed(&messages[0]),
        vec![RemovedMempoolItem::new(
            original_id,
            MempoolRemoveReason::Conflict
        )]
    );
    assert_eq!(added(&messages[1]), vec![replacement_id]);

    assert!(
        timeout(
            Duration::from_millis(200),
            peer.request_transaction(original_id)
        )
        .await
        .is_err()
    );
    assert_eq!(
        peer.request_transaction(replacement_id)
            .await?
            .transaction
            .name(),
        replacement_id
    );

    Ok(())
}

#[tokio::test]
async fn test_replacement_removes_dependent_transactions() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;

    let parent = spend(coin, &puzzle_reveal, &[(puzzle_hash, 1_000, None)])?;
    let child = Coin::new(coin.coin_id(), puzzle_hash, 1_000);
    let dependent = spend(child, &puzzle_reveal, &[(puzzle_hash, 1_000, None)])?;
    let replacement = spend(coin, &puzzle_reveal, &[(puzzle_hash, 999, None)])?;
    let (parent_id, dependent_id, replacement_id) =
        (parent.name(), dependent.name(), replacement.name());

    assert_eq!(peer.send_transaction(parent).await?.status, 1);
    assert_eq!(peer.send_transaction(dependent).await?.status, 1);
    assert_eq!(received(&peer, &mut receiver).await?.len(), 2);

    assert_eq!(peer.send_transaction(replacement).await?.status, 1);
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 3);
    assert_eq!(
        removed(&messages[0]),
        vec![RemovedMempoolItem::new(
            parent_id,
            MempoolRemoveReason::Conflict
        )]
    );
    assert_eq!(added(&messages[1]), vec![replacement_id]);
    assert_eq!(
        removed(&messages[2]),
        vec![RemovedMempoolItem::new(
            dependent_id,
            MempoolRemoveReason::Conflict
        )]
    );

    Ok(())
}

#[tokio::test]
async fn test_conflict_when_farming() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    let child = Coin::new(coin.coin_id(), puzzle_hash, 1_000);
    peer.register_for_coin_updates(vec![child.coin_id()], 0)
        .await?;

    let parent = spend(coin, &puzzle_reveal, &[(puzzle_hash, 1_000, None)])?;
    assert_eq!(peer.send_transaction(parent).await?.status, 1);
    sim.farm_block(1).await;

    let dependent = spend(child, &puzzle_reveal, &[])?;
    let dependent_id = dependent.name();
    assert_eq!(peer.send_transaction(dependent).await?.status, 1);
    received(&peer, &mut receiver).await?;

    // A reorg drops the block that created the child, so the new block can't include the
    // dependent transaction. The reorg is made directly on the simulator, so the peers are told
    // about it explicitly.
    sim.lock().await.reorg_blocks(1, 0);
    sim.notify_peers().await;

    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages[0].msg_type, ProtocolMessageTypes::NewPeakWallet);
    assert_eq!(
        mempool_messages(&messages)
            .into_iter()
            .flat_map(removed)
            .collect::<Vec<_>>(),
        vec![RemovedMempoolItem::new(
            dependent_id,
            MempoolRemoveReason::Conflict
        )]
    );

    Ok(())
}

#[tokio::test]
async fn test_new_subscriptions_replay_the_mempool() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let hint = Bytes32::new([42; 32]);
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    let child = Coin::new(coin.coin_id(), Bytes32::new([7; 32]), 1_000);
    let spend_bundle = spend(
        coin,
        &puzzle_reveal,
        &[(child.puzzle_hash, 1_000, Some(hint))],
    )?;
    let transaction_id = spend_bundle.name();

    // A different peer sent the transaction, as if the wallet is resyncing after a restart.
    let sender = sim.connect().await?;
    assert_eq!(sender.send_transaction(spend_bundle).await?.status, 1);

    let genesis_challenge = sim.config().genesis_challenge;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    // Requests that don't subscribe don't replay anything, and neither do subscriptions that
    // don't match.
    peer.request_puzzle_state(
        vec![puzzle_hash],
        None,
        genesis_challenge,
        CoinStateFilters::new(true, true, true, 0),
        false,
    )
    .await?
    .unwrap();
    peer.request_coin_state(vec![coin.coin_id()], None, genesis_challenge, false)
        .await?
        .unwrap();
    peer.register_for_ph_updates(vec![Bytes32::new([1; 32])], 0)
        .await?;
    assert!(received(&peer, &mut receiver).await?.is_empty());

    // Each way of subscribing replays the matching transaction. Duplicates are allowed.
    peer.request_puzzle_state(
        vec![puzzle_hash],
        None,
        genesis_challenge,
        CoinStateFilters::new(true, true, true, 0),
        true,
    )
    .await?
    .unwrap();
    peer.request_coin_state(vec![child.coin_id()], None, genesis_challenge, true)
        .await?
        .unwrap();
    peer.register_for_coin_updates(vec![coin.coin_id()], 0)
        .await?;
    peer.register_for_ph_updates(vec![hint], 0).await?;

    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 4);
    for message in &messages {
        assert_eq!(added(message), vec![transaction_id]);
    }

    // A peer without the capability doesn't get the replay.
    let (other, mut other_receiver) = connect_with_handshake(&sim, false).await?;
    other.register_for_ph_updates(vec![puzzle_hash], 0).await?;
    assert!(received(&other, &mut other_receiver).await?.is_empty());

    // Once the transaction is confirmed, there's nothing left to replay.
    sim.farm_block(1).await;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;
    assert!(received(&peer, &mut receiver).await?.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_request_transaction() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let peer = sim.connect().await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    let spend_bundle = spend(coin, &puzzle_reveal, &[])?;
    let transaction_id = spend_bundle.name();

    // There's no response for a transaction that isn't in the mempool.
    assert!(
        timeout(
            Duration::from_millis(200),
            peer.request_transaction(transaction_id)
        )
        .await
        .is_err()
    );

    assert_eq!(peer.send_transaction(spend_bundle.clone()).await?.status, 1);
    let response = peer.request_transaction(transaction_id).await?;
    assert_eq!(response.transaction, spend_bundle);

    sim.farm_block(1).await;
    assert!(
        timeout(
            Duration::from_millis(200),
            peer.request_transaction(transaction_id)
        )
        .await
        .is_err()
    );

    Ok(())
}

#[tokio::test]
async fn test_rejected_transaction_is_not_added() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;

    // Creates more value than it spends.
    let spend_bundle = spend(coin, &puzzle_reveal, &[(puzzle_hash, 2, None)])?;
    let ack = peer.send_transaction(spend_bundle).await?;
    assert_eq!(ack.status, 3);
    assert!(received(&peer, &mut receiver).await?.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_signed_transaction_uses_the_config_genesis_challenge() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let peer = sim.connect().await?;
    let pair = BlsPair::new(0);

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 0);
    let coin_spends = vec![CoinSpend::new(
        coin,
        puzzle_reveal,
        to_program([AggSigMe::new(pair.pk, b"Hello, world!".to_vec().into())])?,
    )];
    let signature = sign_transaction(&coin_spends, &[pair.sk])?;

    let ack = peer
        .send_transaction(SpendBundle::new(coin_spends, signature))
        .await?;
    assert_eq!(ack.error, None);
    assert_eq!(ack.status, 1);

    Ok(())
}

#[tokio::test]
async fn test_direct_changes_are_sent_by_notify_peers() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = sim
        .connect_split_with_capabilities(vec![(MEMPOOL_UPDATES, "1".to_string())])
        .await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;

    let spend_bundle = spend(coin, &puzzle_reveal, &[])?;
    let transaction_id = spend_bundle.name();
    assert!(sim.lock().await.push_tx(spend_bundle).success);
    assert!(received(&peer, &mut receiver).await?.is_empty());

    sim.notify_peers().await;
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![transaction_id]);

    Ok(())
}

#[tokio::test]
async fn test_reset_clears_the_mempool() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let peer = sim.connect().await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1_000);
    let spend_bundle = spend(coin, &puzzle_reveal, &[])?;
    let transaction_id = spend_bundle.name();
    assert_eq!(peer.send_transaction(spend_bundle).await?.status, 1);

    sim.reset().await?;
    assert!(
        timeout(
            Duration::from_millis(200),
            peer.request_transaction(transaction_id)
        )
        .await
        .is_err()
    );

    // The new simulator still accepts spends signed for the configured network.
    let pair = BlsPair::new(0);
    let coin = sim.lock().await.new_coin(puzzle_hash, 0);
    let coin_spends = vec![CoinSpend::new(
        coin,
        puzzle_reveal,
        to_program([AggSigMe::new(pair.pk, b"Hello, world!".to_vec().into())])?,
    )];
    let signature = sign_transaction(&coin_spends, &[pair.sk])?;
    let ack = peer
        .send_transaction(SpendBundle::new(coin_spends, signature))
        .await?;
    assert_eq!(ack.status, 1);

    Ok(())
}

#[tokio::test]
async fn test_instant_mode_handshake() -> anyhow::Result<()> {
    let sim = PeerSimulator::new().await?;
    let (peer, mut receiver) = sim.connect_raw().await?;

    peer.send(Handshake {
        network_id: "testnet11".to_string(),
        protocol_version: "0.0.37".to_string(),
        software_version: "0.0.0".to_string(),
        server_port: 0,
        node_type: NodeType::Wallet,
        capabilities: vec![(1, "1".to_string()), (MEMPOOL_UPDATES, "1".to_string())],
    })
    .await?;

    assert_eq!(
        receiver.recv().await.unwrap().msg_type,
        ProtocolMessageTypes::NewPeakWallet
    );
    let message = receiver.recv().await.unwrap();
    let handshake = Handshake::from_bytes(&message.data)?;
    assert_eq!(handshake.node_type, NodeType::FullNode);
    // Without a mempool, the simulator doesn't offer mempool updates.
    assert!(
        !handshake
            .capabilities
            .iter()
            .any(|(capability, _)| *capability == MEMPOOL_UPDATES)
    );

    // Transactions are still included right away, without mempool updates.
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 0);
    peer.register_for_coin_updates(vec![coin.coin_id()], 0)
        .await?;
    let spend_bundle = spend(coin, &puzzle_reveal, &[])?;
    assert_eq!(peer.send_transaction(spend_bundle).await?.status, 1);

    let types: Vec<_> = received(&peer, &mut receiver)
        .await?
        .iter()
        .map(|message| message.msg_type)
        .collect();
    assert_eq!(
        types,
        vec![
            ProtocolMessageTypes::NewPeakWallet,
            ProtocolMessageTypes::CoinStateUpdate
        ]
    );
    Ok(())
}
