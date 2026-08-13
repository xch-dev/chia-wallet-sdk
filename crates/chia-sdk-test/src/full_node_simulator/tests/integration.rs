use chia_bls::{SecretKey, Signature, master_to_wallet_hardened};
use chia_protocol::{Bytes32, Coin, CoinSpend, Program, SpendBundle};
use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};
use chia_sdk_types::conditions::{CreateCoin, Memos};
use clvmr::NodePtr;

use crate::{FullNodeSimulator, FullNodeSimulatorEvent, to_program, to_puzzle};

use super::super::BLOCK_REWARD_AMOUNT;

fn spend_to_child(
    coin: Coin,
    puzzle_reveal: Program,
    puzzle_hash: Bytes32,
    amount: u64,
) -> anyhow::Result<SpendBundle> {
    Ok(SpendBundle::new(
        vec![CoinSpend::new(
            coin,
            puzzle_reveal,
            to_program([CreateCoin::<NodePtr>::new(puzzle_hash, amount, Memos::None)])?,
        )],
        Signature::default(),
    ))
}

fn simulator_with_multiblock_parent_child() -> anyhow::Result<(
    FullNodeSimulator,
    SpendBundle,
    SpendBundle,
    Coin,
    Coin,
    Coin,
)> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    sim.set_farming_ph(puzzle_hash);
    let parent = sim.farm_block(1)[0]
        .reward_claims_incorporated
        .as_ref()
        .unwrap()[0];
    let child = Coin::new(parent.coin_id(), puzzle_hash, 99);
    let grandchild = Coin::new(child.coin_id(), puzzle_hash, 98);
    let parent_bundle = spend_to_child(parent, puzzle_reveal.clone(), puzzle_hash, 99)?;
    let child_bundle = spend_to_child(child, puzzle_reveal, puzzle_hash, 98)?;

    assert!(sim.push_tx(parent_bundle.clone()).success);
    sim.farm_block(1);
    assert!(sim.push_tx(child_bundle.clone()).success);
    sim.farm_block(1);

    Ok((sim, parent_bundle, child_bundle, parent, child, grandchild))
}

#[test]
fn genesis_contains_prefarm_rewards() {
    let sim = FullNodeSimulator::new();
    let prefarm_puzzle_hash = sim.get_prefarm_puzzle_hash();
    assert_eq!(sim.height(), 1);
    assert_eq!(sim.get_farming_ph(), prefarm_puzzle_hash);

    let prefarm_records = sim
        .get_coin_records_by_puzzle_hash(prefarm_puzzle_hash, None, None, None)
        .coin_records
        .unwrap();
    assert_eq!(prefarm_records.len(), 2);
    assert!(prefarm_records.iter().all(|record| record.coinbase));
    assert!(prefarm_records.iter().all(|record| !record.spent));
    assert_eq!(
        prefarm_records
            .iter()
            .map(|record| u128::from(record.coin.amount))
            .sum::<u128>(),
        21_000_000_000_000_000_000_u128
    );

    let genesis = sim.get_block_record_by_height(1).block_record.unwrap();
    let reward_claims = genesis.reward_claims_incorporated.unwrap();
    assert_eq!(reward_claims.len(), 2);
    assert_eq!(
        reward_claims
            .iter()
            .map(|coin| u128::from(coin.amount))
            .sum::<u128>(),
        21_000_000_000_000_000_000_u128
    );
    assert!(
        reward_claims
            .iter()
            .all(|coin| coin.puzzle_hash == prefarm_puzzle_hash)
    );
}

#[test]
fn explicit_secret_key_derives_prefarm_wallet_index_one() {
    let root_secret_key = SecretKey::from_seed(&[42; 32]);
    let sim = FullNodeSimulator::with_secret_key(root_secret_key.clone());
    let expected_secret_key = master_to_wallet_hardened(&root_secret_key, 1).derive_synthetic();
    let expected_puzzle_hash =
        StandardArgs::curry_tree_hash(expected_secret_key.public_key()).into();

    assert_eq!(sim.get_prefarm_puzzle_hash(), expected_puzzle_hash);
}

#[test]
fn push_tx_waits_for_manual_farming() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 100);
    let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;

    assert!(sim.push_tx(spend_bundle).success);
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        1
    );
    assert_eq!(sim.height(), 1);

    sim.farm_block(1);
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        0
    );
    let record = sim
        .get_coin_record_by_name(coin.coin_id())
        .coin_record
        .unwrap();
    assert!(record.spent);
    assert_eq!(record.spent_block_index, 2);
    assert_eq!(sim.height(), 2);
    assert_eq!(
        sim.get_block_spends(sim.header_hash())
            .block_spends
            .unwrap()
            .len(),
        1
    );
    Ok(())
}

#[test]
fn farm_block_includes_mempool_and_emits_event() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 100);
    let child = Coin::new(coin.coin_id(), puzzle_hash, 99);
    let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;
    assert!(sim.push_tx(spend_bundle).success);

    let records = sim.farm_block(1);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].height, 2);
    let reward_claims = records[0].reward_claims_incorporated.clone().unwrap();
    assert_eq!(reward_claims.len(), 1);
    assert_eq!(reward_claims[0].amount, BLOCK_REWARD_AMOUNT);
    assert_eq!(reward_claims[0].puzzle_hash, sim.get_prefarm_puzzle_hash());
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        0
    );

    let spent = sim
        .get_coin_record_by_name(coin.coin_id())
        .coin_record
        .unwrap();
    assert!(spent.spent);
    assert_eq!(spent.spent_block_index, 2);
    let created = sim
        .get_coin_record_by_name(child.coin_id())
        .coin_record
        .unwrap();
    assert!(!created.spent);
    assert_eq!(created.confirmed_block_index, 2);
    assert_eq!(
        sim.get_block_spends(records[0].header_hash)
            .block_spends
            .unwrap()
            .len(),
        1
    );

    let events = sim.drain_events();
    assert!(matches!(
        events.as_slice(),
        [FullNodeSimulatorEvent::Block {
            height: 2,
            additions,
            ..
        }] if additions.iter().any(|record| record.coin.coin_id() == reward_claims[0].coin_id())
    ));
    Ok(())
}

#[test]
fn set_farming_ph_changes_future_reward_destination() {
    let mut sim = FullNodeSimulator::new();
    let (new_farming_ph, _) = to_puzzle(99).unwrap();
    sim.set_farming_ph(new_farming_ph);

    let record = sim.farm_block(1).pop().unwrap();
    let reward_claims = record.reward_claims_incorporated.unwrap();
    assert_eq!(reward_claims.len(), 1);
    assert_eq!(reward_claims[0].amount, BLOCK_REWARD_AMOUNT);
    assert_eq!(reward_claims[0].puzzle_hash, new_farming_ph);
}

#[test]
fn push_tx_accepts_ephemeral_spends_in_same_bundle() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let parent = sim.new_coin(puzzle_hash, 100);
    let child = Coin::new(parent.coin_id(), puzzle_hash, 99);
    let grandchild = Coin::new(child.coin_id(), puzzle_hash, 98);

    let parent_spend = spend_to_child(parent, puzzle_reveal.clone(), puzzle_hash, 99)?;
    let child_spend = CoinSpend::new(
        child,
        puzzle_reveal,
        to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 98, Memos::None)])?,
    );
    let spend_bundle = SpendBundle::new(
        vec![parent_spend.coin_spends[0].clone(), child_spend],
        Signature::default(),
    );
    assert!(sim.push_tx(spend_bundle).success);
    sim.farm_block(1);

    let parent_record = sim
        .get_coin_record_by_name(parent.coin_id())
        .coin_record
        .unwrap();
    assert!(parent_record.spent);
    let child_record = sim
        .get_coin_record_by_name(child.coin_id())
        .coin_record
        .unwrap();
    assert!(child_record.spent);
    assert_eq!(child_record.confirmed_block_index, 2);
    assert_eq!(child_record.spent_block_index, 2);
    let grandchild_record = sim
        .get_coin_record_by_name(grandchild.coin_id())
        .coin_record
        .unwrap();
    assert!(!grandchild_record.spent);
    assert_eq!(grandchild_record.confirmed_block_index, 2);

    sim.revert_blocks(1);
    let restored_parent = sim
        .get_coin_record_by_name(parent.coin_id())
        .coin_record
        .unwrap();
    assert!(!restored_parent.spent);
    assert!(
        sim.get_coin_record_by_name(child.coin_id())
            .coin_record
            .is_none()
    );
    assert!(
        sim.get_coin_record_by_name(grandchild.coin_id())
            .coin_record
            .is_none()
    );
    assert!(
        sim.get_puzzle_and_solution(parent.coin_id(), None)
            .coin_solution
            .is_none()
    );
    assert!(
        sim.get_puzzle_and_solution(child.coin_id(), None)
            .coin_solution
            .is_none()
    );
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        1
    );
    Ok(())
}

#[test]
fn revert_removes_farmed_reward() {
    let mut sim = FullNodeSimulator::new();
    let reward = sim
        .farm_block(1)
        .pop()
        .unwrap()
        .reward_claims_incorporated
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        sim.get_coin_record_by_name(reward.coin_id())
            .coin_record
            .is_some()
    );

    sim.revert_blocks(1);
    assert!(
        sim.get_coin_record_by_name(reward.coin_id())
            .coin_record
            .is_none()
    );
}

#[test]
fn revert_requeues_multiblock_parent_child_in_order() -> anyhow::Result<()> {
    let (mut sim, parent_bundle, child_bundle, parent, child, grandchild) =
        simulator_with_multiblock_parent_child()?;

    sim.revert_blocks(2);

    assert_eq!(
        sim.mempool.keys().copied().collect::<Vec<_>>(),
        vec![parent_bundle.name(), child_bundle.name()]
    );
    assert!(
        !sim.get_coin_record_by_name(parent.coin_id())
            .coin_record
            .unwrap()
            .spent
    );
    assert!(
        sim.get_coin_record_by_name(child.coin_id())
            .coin_record
            .is_none()
    );

    sim.farm_block(1);
    assert!(
        sim.get_coin_record_by_name(child.coin_id())
            .coin_record
            .unwrap()
            .spent
    );
    assert!(
        !sim.get_coin_record_by_name(grandchild.coin_id())
            .coin_record
            .unwrap()
            .spent
    );
    Ok(())
}

#[test]
fn prune_mempool_retries_out_of_order_dependencies() -> anyhow::Result<()> {
    let (mut sim, parent_bundle, child_bundle, _, _, _) = simulator_with_multiblock_parent_child()?;
    sim.revert_blocks(2);
    sim.mempool.swap_indices(0, 1);

    sim.prune_mempool();

    assert_eq!(
        sim.mempool.keys().copied().collect::<Vec<_>>(),
        vec![parent_bundle.name(), child_bundle.name()]
    );
    Ok(())
}

#[test]
fn reorg_replaces_peak_and_emits_reorg() {
    let mut sim = FullNodeSimulator::new();
    let old_blocks = sim.farm_block(2);
    let old_peak = old_blocks.last().unwrap().header_hash;
    let old_reward = old_blocks
        .last()
        .unwrap()
        .reward_claims_incorporated
        .clone()
        .unwrap()
        .pop()
        .unwrap();

    let new_blocks = sim.reorg_blocks(1, 2);
    assert_eq!(new_blocks.len(), 2);
    assert_ne!(sim.header_hash(), old_peak);
    assert_eq!(sim.height(), 4);
    assert!(
        sim.get_coin_record_by_name(old_reward.coin_id())
            .coin_record
            .is_none()
    );
    let orphan = sim.get_block_record(old_peak).block_record.unwrap();
    assert_eq!(
        orphan.reward_claims_incorporated.unwrap()[0].coin_id(),
        old_reward.coin_id()
    );
    assert!(
        new_blocks
            .iter()
            .all(|block| block.reward_claims_incorporated.as_ref().unwrap().len() == 1)
    );

    let events = sim.drain_events();
    assert!(events.iter().any(|event| {
        matches!(
            event,
            FullNodeSimulatorEvent::Reorg {
                old_peak_hash,
                new_peak_hash,
                ..
            } if *old_peak_hash == old_peak && *new_peak_hash == sim.header_hash()
        )
    }));
}

#[test]
fn reorg_does_not_requeue_reverted_transactions() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 100);
    let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;

    assert!(sim.push_tx(spend_bundle).success);
    sim.farm_block(1);
    assert!(
        sim.get_coin_record_by_name(coin.coin_id())
            .coin_record
            .unwrap()
            .spent
    );

    let replacement = sim.reorg_blocks(1, 1);

    assert_eq!(replacement.len(), 1);
    assert!(
        !sim.get_coin_record_by_name(coin.coin_id())
            .coin_record
            .unwrap()
            .spent
    );
    assert_eq!(
        sim.get_block_spends(replacement[0].header_hash)
            .block_spends
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        0
    );

    Ok(())
}

#[test]
fn reorg_does_not_requeue_multiblock_parent_child() -> anyhow::Result<()> {
    let (mut sim, _, _, _, child, grandchild) = simulator_with_multiblock_parent_child()?;

    let replacement = sim.reorg_blocks(2, 0);

    assert!(replacement.is_empty());
    assert!(sim.mempool.is_empty());

    let replacement = sim.farm_block(1);
    assert_eq!(
        sim.get_block_spends(replacement[0].header_hash)
            .block_spends
            .unwrap()
            .len(),
        0
    );
    assert!(
        sim.get_coin_record_by_name(child.coin_id())
            .coin_record
            .is_none()
    );
    assert!(
        sim.get_coin_record_by_name(grandchild.coin_id())
            .coin_record
            .is_none()
    );
    Ok(())
}
