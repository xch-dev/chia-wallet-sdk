use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program, SpendBundle};
use chia_sdk_types::conditions::{CreateCoin, Memos};
use clvmr::NodePtr;

use crate::{FullNodeSimulator, to_program, to_puzzle};

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
    let coin = sim.new_coin(puzzle_hash, 100);
    let first = spend_to_child(coin, puzzle_reveal.clone(), puzzle_hash, 99)?;
    let replacement = spend_to_child(coin, puzzle_reveal, puzzle_hash, 98)?;
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
fn replacing_an_item_removes_the_items_that_spend_its_children() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 100);
    let parent = spend_to_child(coin, puzzle_reveal.clone(), puzzle_hash, 100)?;
    let child = Coin::new(coin.coin_id(), puzzle_hash, 100);
    let dependent = spend_to_child(child, puzzle_reveal.clone(), puzzle_hash, 100)?;
    let replacement = spend_to_child(coin, puzzle_reveal, puzzle_hash, 99)?;
    let dependent_tx_id = dependent.name();

    assert!(sim.push_tx(parent).success);
    assert!(sim.push_tx(dependent).success);
    assert!(sim.push_tx(replacement.clone()).success);

    assert!(
        sim.get_mempool_item_by_tx_id(dependent_tx_id)
            .mempool_item
            .is_none()
    );
    assert!(
        sim.get_mempool_item_by_tx_id(replacement.name())
            .mempool_item
            .is_some()
    );

    Ok(())
}

#[test]
fn farming_removes_items_that_are_no_longer_valid() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 100);
    let parent = spend_to_child(coin, puzzle_reveal.clone(), puzzle_hash, 100)?;
    let child = Coin::new(coin.coin_id(), puzzle_hash, 100);
    let dependent = spend_to_child(child, puzzle_reveal, puzzle_hash, 100)?;
    let dependent_tx_id = dependent.name();

    assert!(sim.push_tx(parent).success);
    sim.farm_block(1);
    assert!(sim.push_tx(dependent).success);

    // The reorg drops the block that created the child, so the dependent item can't be included
    // in the replacement block and is removed from the mempool.
    sim.reorg_blocks(1, 0);

    assert!(
        sim.get_mempool_item_by_tx_id(dependent_tx_id)
            .mempool_item
            .is_none()
    );
    assert!(
        sim.get_coin_record_by_name(child.coin_id())
            .coin_record
            .is_none()
    );

    Ok(())
}

#[test]
fn set_genesis_challenge_changes_the_agg_sig_me_additional_data() -> anyhow::Result<()> {
    use chia_sdk_types::{TESTNET11_CONSTANTS, conditions::AggSigMe};

    use crate::{BlsPair, sign_transaction};

    let mut sim = FullNodeSimulator::new();
    let pair = BlsPair::new(0);
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 0);
    let coin_spends = vec![CoinSpend::new(
        coin,
        puzzle_reveal,
        to_program([AggSigMe::new(pair.pk, b"message".to_vec().into())])?,
    )];
    // Signed with the testnet11 additional data.
    let signature = sign_transaction(&coin_spends, &[pair.sk])?;
    let spend_bundle = SpendBundle::new(coin_spends, signature);

    assert!(!sim.push_tx(spend_bundle.clone()).success);

    sim.set_genesis_challenge(TESTNET11_CONSTANTS.genesis_challenge);
    assert_eq!(
        sim.get_aggsig_additional_data(),
        TESTNET11_CONSTANTS.genesis_challenge
    );
    assert_eq!(
        sim.get_network_info().genesis_challenge,
        Some(TESTNET11_CONSTANTS.genesis_challenge)
    );
    assert!(sim.push_tx(spend_bundle).success);

    Ok(())
}

#[test]
fn farming_removes_items_whose_signature_no_longer_matches() -> anyhow::Result<()> {
    use chia_sdk_types::{TESTNET11_CONSTANTS, conditions::AggSigMe};

    use crate::{BlsPair, sign_transaction};

    let mut sim = FullNodeSimulator::new();
    sim.set_genesis_challenge(TESTNET11_CONSTANTS.genesis_challenge);
    let pair = BlsPair::new(0);
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let coin = sim.new_coin(puzzle_hash, 0);
    let coin_spends = vec![CoinSpend::new(
        coin,
        puzzle_reveal,
        to_program([AggSigMe::new(pair.pk, b"message".to_vec().into())])?,
    )];
    let signature = sign_transaction(&coin_spends, &[pair.sk])?;
    let spend_bundle = SpendBundle::new(coin_spends, signature);
    let tx_id = spend_bundle.name();
    assert!(sim.push_tx(spend_bundle).success);

    // The item can't be included once the additional data changes, so it's dropped instead of
    // staying in the mempool.
    sim.set_genesis_challenge(FullNodeSimulator::new().get_aggsig_additional_data());
    sim.farm_block(1);

    assert!(sim.get_mempool_item_by_tx_id(tx_id).mempool_item.is_none());
    assert!(
        !sim.get_coin_record_by_name(coin.coin_id())
            .coin_record
            .unwrap()
            .spent
    );

    Ok(())
}
