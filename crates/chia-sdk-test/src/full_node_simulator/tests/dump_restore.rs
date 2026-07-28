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

fn simulator_with_spend_and_hint() -> anyhow::Result<FullNodeSimulator> {
    let mut sim = FullNodeSimulator::with_seed(123);
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    sim.set_farming_ph(puzzle_hash);
    let parent = sim.farm_block(1)[0]
        .reward_claims_incorporated
        .as_ref()
        .unwrap()[0];
    let hint = Bytes32::new([7; 32]);
    let spend = SpendBundle::new(
        vec![CoinSpend::new(
            parent,
            puzzle_reveal,
            to_program([CreateCoin::new(puzzle_hash, 99, Memos::Some([hint]))])?,
        )],
        Signature::default(),
    );
    assert!(sim.push_tx(spend).success);
    sim.farm_block(1);
    Ok(sim)
}

fn assert_restore_rejected(value: &serde_json::Value) -> anyhow::Result<()> {
    let mut restored = FullNodeSimulator::with_seed(999);
    assert!(
        restored
            .restore_state(&serde_json::to_string(value)?)
            .is_err()
    );
    Ok(())
}

#[test]
fn dump_restore_accepts_canonical_ephemeral_spends() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    sim.set_farming_ph(puzzle_hash);
    let parent = sim.farm_block(1)[0]
        .reward_claims_incorporated
        .as_ref()
        .unwrap()[0];
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

    let state = sim.dump_state()?;
    let mut restored = FullNodeSimulator::new();
    restored.restore_state(&state)?;

    let child_record = restored
        .get_coin_record_by_name(child.coin_id())
        .coin_record
        .unwrap();
    assert!(child_record.spent);
    assert_eq!(child_record.spent_block_index, 3);
    let grandchild_record = restored
        .get_coin_record_by_name(grandchild.coin_id())
        .coin_record
        .unwrap();
    assert!(!grandchild_record.spent);
    assert_eq!(grandchild_record.confirmed_block_index, 3);
    Ok(())
}

#[test]
fn dump_restore_accepts_separate_parent_child_bundles_in_one_block() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    sim.set_farming_ph(puzzle_hash);
    let parent = sim.farm_block(1)[0]
        .reward_claims_incorporated
        .as_ref()
        .unwrap()[0];
    let child = Coin::new(parent.coin_id(), puzzle_hash, 99);
    let grandchild = Coin::new(child.coin_id(), puzzle_hash, 98);

    assert!(
        sim.push_tx(spend_to_child(
            parent,
            puzzle_reveal.clone(),
            puzzle_hash,
            99
        )?)
        .success
    );
    assert!(
        sim.push_tx(spend_to_child(child, puzzle_reveal, puzzle_hash, 98)?)
            .success
    );
    sim.farm_block(1);

    let state = sim.dump_state()?;
    let value: serde_json::Value = serde_json::from_str(&state)?;
    assert_eq!(value["version"], 1);
    let mut restored = FullNodeSimulator::new();
    restored.restore_state(&state)?;

    assert!(
        restored
            .get_coin_record_by_name(parent.coin_id())
            .coin_record
            .unwrap()
            .spent
    );
    assert!(
        restored
            .get_coin_record_by_name(child.coin_id())
            .coin_record
            .unwrap()
            .spent
    );
    assert!(
        !restored
            .get_coin_record_by_name(grandchild.coin_id())
            .coin_record
            .unwrap()
            .spent
    );
    Ok(())
}

#[test]
fn dump_restore_preserves_canonical_state_and_future_rng() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::with_seed(123);
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    sim.set_farming_ph(puzzle_hash);
    let coin = sim.farm_block(1)[0]
        .reward_claims_incorporated
        .as_ref()
        .unwrap()[0];
    let spend_bundle = spend_to_child(coin, puzzle_reveal, puzzle_hash, 100)?;
    assert!(sim.push_tx(spend_bundle.clone()).success);
    sim.farm_block(1);
    let spent_coin_id = spend_bundle.coin_spends[0].coin.coin_id();
    let state = sim.dump_state()?;

    let mut expected = sim.clone();
    let expected_next_block = expected.farm_block(1)[0].clone();
    let mut restored = FullNodeSimulator::with_seed(999);
    restored.restore_state(&state)?;

    assert_eq!(restored.height(), sim.height());
    assert_eq!(restored.header_hash(), sim.header_hash());
    assert_eq!(restored.get_farming_ph(), sim.get_farming_ph());
    assert_eq!(
        restored.get_master_secret_key().to_bytes(),
        sim.get_master_secret_key().to_bytes()
    );
    assert_eq!(
        restored
            .get_blockchain_state()
            .blockchain_state
            .unwrap()
            .node_id,
        sim.get_blockchain_state().blockchain_state.unwrap().node_id
    );
    assert_eq!(
        restored
            .get_puzzle_and_solution(spent_coin_id, None)
            .coin_solution
            .unwrap()
            .coin,
        spend_bundle.coin_spends[0].coin
    );
    assert_eq!(
        restored.farm_block(1)[0].header_hash,
        expected_next_block.header_hash
    );
    Ok(())
}

#[test]
fn dump_restore_drops_pending_mempool() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    sim.set_farming_ph(puzzle_hash);
    let coin = sim.farm_block(1)[0]
        .reward_claims_incorporated
        .as_ref()
        .unwrap()[0];
    assert!(
        sim.push_tx(spend_to_child(coin, puzzle_reveal, puzzle_hash, 100)?)
            .success
    );
    assert_eq!(
        sim.get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        1
    );

    let state = sim.dump_state()?;
    let mut restored = FullNodeSimulator::new();
    restored.restore_state(&state)?;
    assert_eq!(
        restored
            .get_blockchain_state()
            .blockchain_state
            .unwrap()
            .mempool_size,
        0
    );
    Ok(())
}

#[test]
fn dump_restore_drops_orphaned_blocks() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let old_peak = sim.farm_block(1)[0].header_hash;
    sim.reorg_blocks(1, 1);
    assert!(sim.get_block_record(old_peak).block_record.is_some());

    let state = sim.dump_state()?;
    let mut restored = FullNodeSimulator::new();
    restored.restore_state(&state)?;
    assert!(restored.get_block_record(old_peak).block_record.is_none());
    assert_eq!(restored.header_hash(), sim.header_hash());
    Ok(())
}

#[test]
fn dump_restore_drops_unspent_manual_coins() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, _) = to_puzzle(1)?;
    let manual_coin = sim.new_coin(puzzle_hash, 100);

    let state = sim.dump_state()?;
    let mut restored = FullNodeSimulator::new();
    restored.restore_state(&state)?;
    assert!(
        restored
            .get_coin_record_by_name(manual_coin.coin_id())
            .coin_record
            .is_none()
    );
    Ok(())
}

#[test]
fn dump_fails_when_canonical_chain_spends_manual_coin() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
    let manual_coin = sim.new_coin(puzzle_hash, 100);
    assert!(
        sim.push_tx(spend_to_child(manual_coin, puzzle_reveal, puzzle_hash, 99)?)
            .success
    );
    sim.farm_block(1);

    let error = sim.dump_state().unwrap_err().to_string();
    assert!(error.contains("unsupported manual coin"));
    Ok(())
}

#[test]
fn restore_rejects_invalid_state_atomically() -> anyhow::Result<()> {
    let mut source = FullNodeSimulator::with_seed(123);
    source.farm_block(2);
    let mut value: serde_json::Value = serde_json::from_str(&source.dump_state()?)?;
    value["format"] = serde_json::Value::String("wrong".to_string());
    let invalid_state = serde_json::to_string(&value)?;

    let mut target = FullNodeSimulator::with_seed(999);
    let original_height = target.height();
    let original_peak = target.header_hash();
    assert!(target.restore_state(&invalid_state).is_err());
    assert_eq!(target.height(), original_height);
    assert_eq!(target.header_hash(), original_peak);
    assert!(target.restore_state("{").is_err());
    assert_eq!(target.height(), original_height);
    assert_eq!(target.header_hash(), original_peak);
    Ok(())
}

#[test]
fn restore_clears_event_queue() -> anyhow::Result<()> {
    let mut sim = FullNodeSimulator::new();
    sim.farm_block(1);
    let state = sim.dump_state()?;

    let mut restored = FullNodeSimulator::new();
    restored.restore_state(&state)?;
    assert!(restored.drain_events().is_empty());
    Ok(())
}

#[test]
fn restore_accepts_valid_v1_state() -> anyhow::Result<()> {
    let source = simulator_with_spend_and_hint()?;
    let state = source.dump_state()?;
    let value: serde_json::Value = serde_json::from_str(&state)?;
    assert_eq!(value["version"], 1);

    let mut restored = FullNodeSimulator::new();
    restored.restore_state(&state)?;
    assert_eq!(restored.height(), source.height());
    assert_eq!(restored.header_hash(), source.header_hash());
    Ok(())
}

#[test]
fn restore_rejects_tampered_coin_records() -> anyhow::Result<()> {
    let source = simulator_with_spend_and_hint()?;
    let mut value: serde_json::Value = serde_json::from_str(&source.dump_state()?)?;
    let timestamp = value["coins"][0][1]["timestamp"].as_u64().unwrap();
    value["coins"][0][1]["timestamp"] = (timestamp + 1).into();

    assert_restore_rejected(&value)
}

#[test]
fn restore_rejects_tampered_spends_and_hints() -> anyhow::Result<()> {
    let source = simulator_with_spend_and_hint()?;
    let value: serde_json::Value = serde_json::from_str(&source.dump_state()?)?;

    let mut tampered_spend = value.clone();
    tampered_spend["coin_spends"][0][0] = serde_json::to_value(Bytes32::default())?;
    assert_restore_rejected(&tampered_spend)?;

    let mut tampered_hint = value;
    tampered_hint["coin_hints"][0][1] = serde_json::to_value(Bytes32::default())?;
    assert_restore_rejected(&tampered_hint)
}

#[test]
fn restore_rejects_broken_order_links_heights_and_timestamps() -> anyhow::Result<()> {
    let source = simulator_with_spend_and_hint()?;
    let value: serde_json::Value = serde_json::from_str(&source.dump_state()?)?;

    let mut broken_order = value.clone();
    broken_order["blocks"].as_array_mut().unwrap().swap(0, 1);
    assert_restore_rejected(&broken_order)?;

    let mut broken_link = value.clone();
    broken_link["blocks"][1]["record"]["prev_hash"] = serde_json::to_value(Bytes32::new([9; 32]))?;
    assert_restore_rejected(&broken_link)?;

    let mut broken_height = value.clone();
    broken_height["blocks"][1]["record"]["height"] = 99.into();
    assert_restore_rejected(&broken_height)?;

    let mut broken_timestamp = value;
    let timestamp = broken_timestamp["blocks"][1]["record"]["timestamp"]
        .as_u64()
        .unwrap();
    broken_timestamp["blocks"][1]["record"]["timestamp"] = (timestamp + 1).into();
    assert_restore_rejected(&broken_timestamp)
}

#[test]
fn restore_rejects_duplicate_serialized_keys() -> anyhow::Result<()> {
    let source = simulator_with_spend_and_hint()?;
    let value: serde_json::Value = serde_json::from_str(&source.dump_state()?)?;

    for field in ["coins", "coin_spends", "coin_hints"] {
        let mut duplicate = value.clone();
        let entry = duplicate[field][0].clone();
        duplicate[field].as_array_mut().unwrap().push(entry);
        assert_restore_rejected(&duplicate)?;
    }

    let mut duplicate_header = value;
    let header = duplicate_header["header_hashes"][0].clone();
    duplicate_header["header_hashes"][1] = header;
    assert_restore_rejected(&duplicate_header)
}

#[test]
fn tampered_restore_failure_is_atomic() -> anyhow::Result<()> {
    let source = simulator_with_spend_and_hint()?;
    let mut value: serde_json::Value = serde_json::from_str(&source.dump_state()?)?;
    value["blocks"][1]["record"]["height"] = 99.into();

    let mut target = FullNodeSimulator::with_seed(999);
    target.farm_block(2);
    let before = target.dump_state()?;
    assert!(
        target
            .restore_state(&serde_json::to_string(&value)?)
            .is_err()
    );
    assert_eq!(target.dump_state()?, before);
    Ok(())
}
