use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program, SpendBundle};
use chia_sdk_types::conditions::{CreateCoin, Memos};
use clvmr::NodePtr;

use crate::{FullNodeSimulator, MEMPOOL_MIN_FEE_INCREASE, to_program, to_puzzle};

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

#[test]
fn push_tx_rejects_mempool_conflict() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 100);
    let first = spend_to_child(coin, puzzle_reveal.clone(), puzzle_hash, 98)?;
    let conflicting = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;

    assert!(sim.push_tx(first).success);
    let response = sim.push_tx(conflicting);

    assert!(!response.success);
    assert_eq!(
        response.error.as_deref(),
        Some("Validation error: MempoolConflict")
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
fn push_tx_replaces_mempool_conflict_with_higher_fee_superset() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let amount = 100 + MEMPOOL_MIN_FEE_INCREASE;
    let coin = sim.new_coin(puzzle_hash, amount);
    let first = spend_to_child(coin, puzzle_reveal.clone(), puzzle_hash, amount - 1)?;
    let replacement = spend_to_child(
        coin,
        puzzle_reveal,
        puzzle_hash,
        amount - 1 - MEMPOOL_MIN_FEE_INCREASE,
    )?;
    let first_tx_id = first.name();
    let replacement_tx_id = replacement.name();

    assert!(sim.push_tx(first).success);
    assert!(sim.push_tx(replacement).success);

    assert!(
        sim.get_mempool_item_by_tx_id(first_tx_id)
            .mempool_item
            .is_none()
    );
    assert!(
        sim.get_mempool_item_by_tx_id(replacement_tx_id)
            .mempool_item
            .is_some()
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
fn push_tx_does_not_replace_conflict_that_is_not_a_superset() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin_a = sim.new_coin(puzzle_hash, 100);
    let coin_b = sim.new_coin(puzzle_hash, 100);
    let first_spend_a = spend_to_child(coin_a, puzzle_reveal.clone(), puzzle_hash, 99)?;
    let first_spend_b = spend_to_child(coin_b, puzzle_reveal.clone(), puzzle_hash, 99)?;
    let first = SpendBundle::new(
        vec![
            first_spend_a.coin_spends[0].clone(),
            first_spend_b.coin_spends[0].clone(),
        ],
        Signature::default(),
    );
    let conflicting = spend_to_child(coin_a, puzzle_reveal, puzzle_hash, 50)?;

    assert!(sim.push_tx(first).success);
    let response = sim.push_tx(conflicting);

    assert!(!response.success);
    assert_eq!(
        response.error.as_deref(),
        Some("Validation error: MempoolConflict")
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
fn push_tx_allows_dedup_compatible_mempool_overlap() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let shared_coin = sim.new_coin(puzzle_hash, 100);
    let extra_coin = sim.new_coin(puzzle_hash, 100);
    let shared_spend = spend_to_child(shared_coin, puzzle_reveal.clone(), puzzle_hash, 100)?;
    let extra_spend = spend_to_child(extra_coin, puzzle_reveal, puzzle_hash, 99)?;
    let second_bundle = SpendBundle::new(
        vec![
            shared_spend.coin_spends[0].clone(),
            extra_spend.coin_spends[0].clone(),
        ],
        Signature::default(),
    );

    assert!(sim.push_tx(shared_spend).success);
    assert!(sim.push_tx(second_bundle).success);
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        2
    );

    sim.farm_block(1);
    let spends = sim
        .get_block_spends(sim.header_hash())
        .block_spends
        .unwrap();
    assert_eq!(spends.len(), 2);
    assert_eq!(
        spends
            .iter()
            .filter(|spend| spend.coin.coin_id() == shared_coin.coin_id())
            .count(),
        1
    );

    Ok(())
}

#[test]
fn push_tx_rejects_replacement_below_min_fee_increase() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let amount = 100 + MEMPOOL_MIN_FEE_INCREASE;
    let coin = sim.new_coin(puzzle_hash, amount);
    let first = spend_to_child(coin, puzzle_reveal.clone(), puzzle_hash, amount - 1)?;
    let replacement = spend_to_child(
        coin,
        puzzle_reveal,
        puzzle_hash,
        amount - MEMPOOL_MIN_FEE_INCREASE,
    )?;

    assert!(sim.push_tx(first).success);
    let response = sim.push_tx(replacement);

    assert!(!response.success);
    assert_eq!(
        response.error.as_deref(),
        Some("Validation error: MempoolConflict")
    );

    Ok(())
}

#[test]
fn push_tx_rejects_replacement_with_lower_fee_per_cost() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let fee = 1_000_000_000_000;
    let coin_a = sim.new_coin(puzzle_hash, fee + 1);
    let coin_b = sim.new_coin(puzzle_hash, MEMPOOL_MIN_FEE_INCREASE + 1);
    let first = spend_to_child(coin_a, puzzle_reveal.clone(), puzzle_hash, 1)?;

    // The replacement pays the minimum fee increase, but roughly doubles the cost.
    let spend_a = spend_to_child(coin_a, puzzle_reveal.clone(), puzzle_hash, 1)?;
    let spend_b = spend_to_child(coin_b, puzzle_reveal, puzzle_hash, 1)?;
    let replacement = SpendBundle::new(
        vec![
            spend_a.coin_spends[0].clone(),
            spend_b.coin_spends[0].clone(),
        ],
        Signature::default(),
    );

    assert!(sim.push_tx(first).success);
    let response = sim.push_tx(replacement);

    assert!(!response.success);
    assert_eq!(
        response.error.as_deref(),
        Some("Validation error: MempoolConflict")
    );

    Ok(())
}

#[test]
fn farm_block_respects_block_cost_limit() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

    // Each transaction costs roughly a third of the block cost limit, so only three fit.
    let outputs = 1500;
    for _ in 0..4 {
        let coin = sim.new_coin(puzzle_hash, outputs * outputs);
        let conditions = (1..=outputs)
            .map(|amount| CreateCoin::<NodePtr>::new(puzzle_hash, amount, Memos::None))
            .collect::<Vec<_>>();
        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal.clone(),
                to_program(conditions)?,
            )],
            Signature::default(),
        );
        assert!(sim.push_tx(spend_bundle).success);
    }

    sim.farm_block(1);
    let spends = sim
        .get_block_spends(sim.header_hash())
        .block_spends
        .unwrap();
    assert_eq!(spends.len(), 3);
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        1
    );

    sim.farm_block(1);
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        0
    );

    Ok(())
}
