//! Tests for the mempool mode of the [`PeerSimulator`] and its CHIP-26 mempool updates, over a
//! real websocket connection.

use std::time::Duration;

use chia_bls::Signature;
use chia_protocol::{
    Bytes32, Coin, CoinSpend, CoinStateFilters, CoinStateUpdate, Handshake, MempoolItemsAdded,
    MempoolItemsRemoved, MempoolRemoveReason, Message, NewPeakWallet, NodeType, Program,
    ProtocolMessageTypes, RejectStateReason, RemovedMempoolItem, SpendBundle,
};
use chia_sdk_client::Peer;
use chia_sdk_types::conditions::{AggSigMe, CreateCoin, Memos};
use chia_traits::Streamable;
use tokio::{sync::mpsc, time::timeout};

use crate::{BlsPair, FullNodeSimulator, sign_transaction, to_program, to_puzzle};

use super::PeerSimulator;

use super::notify::MEMPOOL_UPDATES_CAPABILITY as MEMPOOL_UPDATES;

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

#[tokio::test]
async fn test_spending_a_hinted_coin_matches_the_hint() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let hint = Bytes32::new([42; 32]);
    let other_puzzle_hash = Bytes32::new([7; 32]);

    // A coin on chain that is hinted to the wallet, like an NFT or CAT it holds.
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    let hinted = Coin::new(coin.coin_id(), puzzle_hash, 1);
    let create_hinted = spend(coin, &puzzle_reveal, &[(puzzle_hash, 1, Some(hint))])?;
    assert!(sim.lock().await.push_tx(create_hinted).success);
    sim.farm_block(1).await;

    // Subscribed to the hint only, as a wallet subscribes to its p2 puzzle hash.
    peer.register_for_ph_updates(vec![hint], 0).await?;
    received(&peer, &mut receiver).await?;

    // Sending the coin away creates nothing hinted to the wallet, but it spends a hinted coin.
    let send_away = spend(hinted, &puzzle_reveal, &[(other_puzzle_hash, 1, None)])?;
    let send_away_id = send_away.name();
    assert_eq!(peer.send_transaction(send_away).await?.status, 1);
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![send_away_id]);

    // The same holds for a hinted coin that only exists in the mempool.
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    let pending_hinted = Coin::new(coin.coin_id(), puzzle_hash, 1);
    let create_pending = spend(coin, &puzzle_reveal, &[(puzzle_hash, 1, Some(hint))])?;
    let spend_pending = spend(
        pending_hinted,
        &puzzle_reveal,
        &[(other_puzzle_hash, 1, None)],
    )?;
    let (create_id, spend_id) = (create_pending.name(), spend_pending.name());
    assert_eq!(peer.send_transaction(create_pending).await?.status, 1);
    assert_eq!(peer.send_transaction(spend_pending).await?.status, 1);
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(
        messages.iter().flat_map(added).collect::<Vec<_>>(),
        vec![create_id, spend_id]
    );

    // A new subscription to the hint replays all three.
    let (other, mut other_receiver) = connect_with_handshake(&sim, true).await?;
    other.register_for_ph_updates(vec![hint], 0).await?;
    let messages = received(&other, &mut other_receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![send_away_id, create_id, spend_id]);

    Ok(())
}

#[tokio::test]
async fn test_ephemeral_coin_is_sent_once() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = sim.connect_split().await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    let ephemeral = Coin::new(coin.coin_id(), puzzle_hash, 1);
    peer.register_for_coin_updates(vec![ephemeral.coin_id()], 0)
        .await?;

    let mut spend_bundle = spend(coin, &puzzle_reveal, &[(puzzle_hash, 1, None)])?;
    spend_bundle
        .coin_spends
        .extend(spend(ephemeral, &puzzle_reveal, &[])?.coin_spends);
    assert_eq!(peer.send_transaction(spend_bundle).await?.status, 1);

    let height = sim.farm_block(1).await[0].height;
    let messages = received(&peer, &mut receiver).await?;
    let update = CoinStateUpdate::from_bytes(&messages[1].data)?;
    assert_eq!(update.items.len(), 1);
    assert_eq!(update.items[0].coin, ephemeral);
    assert_eq!(update.items[0].created_height, Some(height));
    assert_eq!(update.items[0].spent_height, Some(height));

    Ok(())
}

#[tokio::test]
async fn test_puzzle_solution_and_children() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let peer = sim.connect().await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 3);
    let spend_bundle = spend(
        coin,
        &puzzle_reveal,
        &[(puzzle_hash, 1, None), (puzzle_hash, 2, None)],
    )?;
    let solution = spend_bundle.coin_spends[0].solution.clone();
    assert_eq!(peer.send_transaction(spend_bundle).await?.status, 1);

    // Nothing is spent or created while the transaction is pending.
    assert!(
        peer.request_children(coin.coin_id())
            .await?
            .coin_states
            .is_empty()
    );
    let height = sim.lock().await.height() + 1;
    assert!(
        peer.request_puzzle_and_solution(coin.coin_id(), height)
            .await?
            .is_err()
    );

    sim.farm_block(1).await;

    let response = peer
        .request_puzzle_and_solution(coin.coin_id(), height)
        .await?
        .unwrap();
    assert_eq!(response.puzzle, puzzle_reveal);
    assert_eq!(response.solution, solution);
    assert!(
        peer.request_puzzle_and_solution(coin.coin_id(), height - 1)
            .await?
            .is_err()
    );

    let children = peer.request_children(coin.coin_id()).await?.coin_states;
    let mut amounts: Vec<_> = children.iter().map(|cs| cs.coin.amount).collect();
    amounts.sort_unstable();
    assert_eq!(amounts, vec![1, 2]);
    assert!(
        children
            .iter()
            .all(|cs| cs.created_height == Some(height) && cs.spent_height.is_none())
    );

    Ok(())
}

#[tokio::test]
async fn test_revert_requeues_transactions() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    peer.register_for_coin_updates(vec![coin.coin_id()], 0)
        .await?;

    let spend_bundle = spend(coin, &puzzle_reveal, &[])?;
    let transaction_id = spend_bundle.name();
    assert_eq!(peer.send_transaction(spend_bundle).await?.status, 1);
    sim.farm_block(1).await;
    received(&peer, &mut receiver).await?;

    // Reverting the block puts its transaction back in the mempool.
    sim.lock().await.revert_blocks(1);
    sim.notify_peers().await;
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![transaction_id]);

    Ok(())
}

#[tokio::test]
async fn test_reorg_rejections_are_labelled_as_rejections() -> anyhow::Result<()> {
    let sim = PeerSimulator::new().await?;
    let peer = sim.connect().await?;
    let wrong_header_hash = Bytes32::new([1; 32]);

    let rejection = peer
        .request_coin_state(vec![Bytes32::default()], None, wrong_header_hash, false)
        .await?
        .unwrap_err();
    assert_eq!(rejection.reason, RejectStateReason::Reorg);

    let rejection = peer
        .request_puzzle_state(
            vec![Bytes32::default()],
            None,
            wrong_header_hash,
            CoinStateFilters::new(true, true, true, 0),
            false,
        )
        .await?
        .unwrap_err();
    assert_eq!(rejection.reason, RejectStateReason::Reorg);

    Ok(())
}

#[tokio::test]
async fn test_incoming_payment_matches_the_puzzle_hash() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    // Someone else's coin pays the wallet's puzzle hash, without a hint.
    let (sender_puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let wallet_puzzle_hash = Bytes32::new([9; 32]);
    let coin = sim.lock().await.new_coin(sender_puzzle_hash, 1);
    let payment = Coin::new(coin.coin_id(), wallet_puzzle_hash, 1);
    peer.register_for_ph_updates(vec![wallet_puzzle_hash], 0)
        .await?;

    let spend_bundle = spend(coin, &puzzle_reveal, &[(wallet_puzzle_hash, 1, None)])?;
    let transaction_id = spend_bundle.name();
    assert_eq!(
        sim.connect()
            .await?
            .send_transaction(spend_bundle)
            .await?
            .status,
        1
    );

    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![transaction_id]);

    // A wallet that subscribes to the created coin's id also finds it, live and on replay.
    let (by_coin, mut by_coin_receiver) = connect_with_handshake(&sim, true).await?;
    by_coin
        .register_for_coin_updates(vec![payment.coin_id()], 0)
        .await?;
    let messages = received(&by_coin, &mut by_coin_receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![transaction_id]);

    let (replayed, mut replayed_receiver) = connect_with_handshake(&sim, true).await?;
    replayed
        .register_for_ph_updates(vec![wallet_puzzle_hash], 0)
        .await?;
    let messages = received(&replayed, &mut replayed_receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![transaction_id]);

    Ok(())
}

#[tokio::test]
async fn test_hint_matches_are_removed_after_the_hinted_coin_is_gone() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let hint = Bytes32::new([42; 32]);
    let other_puzzle_hash = Bytes32::new([7; 32]);
    peer.register_for_ph_updates(vec![hint], 0).await?;

    // The parent creates a hinted coin, the dependent spends it without a hint, and both match
    // only through the hint.
    let coin = sim.lock().await.new_coin(puzzle_hash, 2);
    let hinted = Coin::new(coin.coin_id(), puzzle_hash, 2);
    let parent = spend(coin, &puzzle_reveal, &[(puzzle_hash, 2, Some(hint))])?;
    let dependent = spend(hinted, &puzzle_reveal, &[(other_puzzle_hash, 2, None)])?;
    let (parent_id, dependent_id) = (parent.name(), dependent.name());
    assert_eq!(peer.send_transaction(parent).await?.status, 1);
    assert_eq!(peer.send_transaction(dependent).await?.status, 1);
    received(&peer, &mut receiver).await?;

    // The replacement doesn't create the hinted coin, so the dependent is evicted, and its
    // removal is still sent to the peer it was added for.
    let replacement = spend(coin, &puzzle_reveal, &[(other_puzzle_hash, 1, None)])?;
    assert_eq!(peer.send_transaction(replacement).await?.status, 1);
    let removals: Vec<_> = received(&peer, &mut receiver)
        .await?
        .iter()
        .filter(|message| message.msg_type == ProtocolMessageTypes::MempoolItemsRemoved)
        .flat_map(removed)
        .collect();
    assert_eq!(
        removals,
        vec![
            RemovedMempoolItem::new(parent_id, MempoolRemoveReason::Conflict),
            RemovedMempoolItem::new(dependent_id, MempoolRemoveReason::Conflict),
        ]
    );

    // The same after a reorg removes a hinted coin from the chain.
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    let hinted = Coin::new(coin.coin_id(), puzzle_hash, 1);
    let parent = spend(coin, &puzzle_reveal, &[(puzzle_hash, 1, Some(hint))])?;
    assert_eq!(peer.send_transaction(parent).await?.status, 1);
    sim.farm_block(1).await;
    let dependent = spend(hinted, &puzzle_reveal, &[(other_puzzle_hash, 1, None)])?;
    let dependent_id = dependent.name();
    assert_eq!(peer.send_transaction(dependent).await?.status, 1);
    received(&peer, &mut receiver).await?;

    sim.lock().await.reorg_blocks(1, 0);
    sim.notify_peers().await;
    let removals: Vec<_> = received(&peer, &mut receiver)
        .await?
        .iter()
        .filter(|message| message.msg_type == ProtocolMessageTypes::MempoolItemsRemoved)
        .flat_map(removed)
        .collect();
    assert!(removals.contains(&RemovedMempoolItem::new(
        dependent_id,
        MempoolRemoveReason::Conflict
    )));

    Ok(())
}

#[tokio::test]
async fn test_capability_must_be_enabled() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = sim
        .connect_split_with_capabilities(vec![(MEMPOOL_UPDATES, "0".to_string())])
        .await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;
    assert_eq!(
        peer.send_transaction(spend(coin, &puzzle_reveal, &[])?)
            .await?
            .status,
        1
    );
    assert!(received(&peer, &mut receiver).await?.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_subscription_limit_rejections_are_labelled_as_rejections() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_config(super::PeerSimulatorConfig {
        max_subscriptions: 1,
        ..Default::default()
    })
    .await?;
    let peer = sim.connect().await?;
    let genesis_challenge = sim.config().genesis_challenge;
    let ids = vec![Bytes32::new([1; 32]), Bytes32::new([2; 32])];

    let rejection = peer
        .request_coin_state(ids.clone(), None, genesis_challenge, true)
        .await?
        .unwrap_err();
    assert_eq!(
        rejection.reason,
        RejectStateReason::ExceededSubscriptionLimit
    );

    let rejection = peer
        .request_puzzle_state(
            ids,
            None,
            genesis_challenge,
            CoinStateFilters::new(true, true, true, 0),
            true,
        )
        .await?
        .unwrap_err();
    assert_eq!(
        rejection.reason,
        RejectStateReason::ExceededSubscriptionLimit
    );

    Ok(())
}

#[tokio::test]
async fn test_items_that_survive_a_revert_are_not_announced_again() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;

    let confirmed = sim.lock().await.new_coin(puzzle_hash, 1);
    let confirmed_spend = spend(confirmed, &puzzle_reveal, &[])?;
    let confirmed_id = confirmed_spend.name();
    assert_eq!(peer.send_transaction(confirmed_spend).await?.status, 1);
    sim.farm_block(1).await;

    let pending = sim.lock().await.new_coin(puzzle_hash, 1);
    assert_eq!(
        peer.send_transaction(spend(pending, &puzzle_reveal, &[])?)
            .await?
            .status,
        1
    );
    received(&peer, &mut receiver).await?;

    // Only the reverted block's transaction is new to the mempool.
    sim.lock().await.revert_blocks(1);
    sim.notify_peers().await;
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![confirmed_id]);

    // The reorg prune puts both back without announcing either.
    sim.lock().await.reorg_blocks(0, 0);
    sim.notify_peers().await;
    assert!(mempool_messages(&received(&peer, &mut receiver).await?).is_empty());

    Ok(())
}

#[tokio::test]
async fn test_removed_subscriptions_stop_updates() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;
    peer.register_for_coin_updates(vec![coin.coin_id()], 0)
        .await?;

    let removed_puzzle_hashes = peer
        .remove_puzzle_subscriptions(Some(vec![puzzle_hash]))
        .await?
        .puzzle_hashes;
    assert_eq!(removed_puzzle_hashes, vec![puzzle_hash]);
    let removed_coin_ids = peer.remove_coin_subscriptions(None).await?.coin_ids;
    assert_eq!(removed_coin_ids, vec![coin.coin_id()]);

    assert_eq!(
        peer.send_transaction(spend(coin, &puzzle_reveal, &[(puzzle_hash, 1, None)])?)
            .await?
            .status,
        1
    );
    sim.farm_block(1).await;

    let types: Vec<_> = received(&peer, &mut receiver)
        .await?
        .iter()
        .map(|message| message.msg_type)
        .collect();
    assert_eq!(types, vec![ProtocolMessageTypes::NewPeakWallet]);

    Ok(())
}

#[cfg(feature = "serde")]
#[tokio::test]
async fn test_restore_state_removes_the_mempool() -> anyhow::Result<()> {
    let sim = PeerSimulator::with_mempool().await?;
    let (peer, mut receiver) = connect_with_handshake(&sim, true).await?;

    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    peer.register_for_ph_updates(vec![puzzle_hash], 0).await?;
    let dump = sim.lock().await.dump_state()?;

    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    let pending = spend(coin, &puzzle_reveal, &[])?;
    let pending_id = pending.name();
    assert_eq!(peer.send_transaction(pending).await?.status, 1);
    received(&peer, &mut receiver).await?;

    sim.lock().await.restore_state(&dump)?;
    sim.notify_peers().await;
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(
        removed(&messages[0]),
        vec![RemovedMempoolItem::new(
            pending_id,
            MempoolRemoveReason::Conflict
        )]
    );

    // Mempool updates keep working after the restore.
    let coin = sim.lock().await.new_coin(puzzle_hash, 1);
    let next = spend(coin, &puzzle_reveal, &[])?;
    let next_id = next.name();
    assert_eq!(peer.send_transaction(next).await?.status, 1);
    let messages = received(&peer, &mut receiver).await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(added(&messages[0]), vec![next_id]);

    Ok(())
}
