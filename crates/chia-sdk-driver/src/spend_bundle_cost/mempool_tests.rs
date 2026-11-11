//! The spend cost tests from chia-blockchain's mempool test suite, ported to [`spend_bundle_cost`]
//! and [`spend_bundle_conditions`]. They come from `chia/_tests/core/mempool/test_mempool.py`,
//! `chia/_tests/core/mempool/test_mempool_manager.py` and `chia/_tests/core/test_cost_calculation.py`.
//!
//! The Python tests run conditions through a block generator. These run the same conditions as a
//! spend bundle, the way the mempool admits a transaction, at a height on each side of the 3.0 hard
//! fork.

use chia_bls::{PublicKey, SecretKey};
use chia_consensus::{
    opcodes::{AGG_SIG_COST, CREATE_COIN_COST, NEW_CREATE_COIN_COST, SPEND_COST},
    solution_generator::calculate_generator_length,
};
use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzle_types::{Memos, standard::StandardArgs};
use chia_sdk_types::{Conditions, MAINNET_CONSTANTS};
use chialisp::classic::clvm_tools::binutils::assemble;
use clvm_utils::tree_hash;
use clvmr::{Allocator, serde::node_to_bytes};
use rstest::rstest;

use crate::{SpendContext, StandardLayer};

use super::*;

const PUZZLE_HASH: &str = "abababababababababababababababab";

fn public_key() -> PublicKey {
    SecretKey::from_seed(&[1; 32]).public_key()
}

fn pre_fork() -> u32 {
    MAINNET_CONSTANTS.hard_fork2_height - 1
}

fn post_fork() -> u32 {
    MAINNET_CONSTANTS.hard_fork2_height
}

fn is_post_fork(height: u32) -> bool {
    height >= MAINNET_CONSTANTS.hard_fork2_height
}

/// Spends a coin whose puzzle is `program` with a nil solution, like chia-blockchain's
/// `generator_condition_tester`. With `quote`, the program is `(q conditions...)`.
fn spend(program: &str, quote: bool, amount: u64) -> anyhow::Result<CoinSpend> {
    let mut allocator = Allocator::new();
    let source = if quote {
        format!("(q {program})")
    } else {
        program.to_string()
    };
    let puzzle = assemble(&mut allocator, &source)?;
    let coin = Coin::new(
        Bytes32::new([1; 32]),
        tree_hash(&allocator, puzzle).into(),
        amount,
    );
    Ok(CoinSpend::new(
        coin,
        Program::from(node_to_bytes(&allocator, puzzle)?),
        Program::from(vec![0x80]),
    ))
}

fn conditions(program: &str, height: u32) -> Result<OwnedSpendBundleConditions, DriverError> {
    spend_bundle_conditions(
        &[spend(program, true, 123).expect("valid program")],
        &MAINNET_CONSTANTS,
        height,
    )
}

/// The generator byte cost before the hard fork, which doesn't include the quote that wraps it.
fn pre_fork_byte_cost(coin_spends: &[CoinSpend]) -> u64 {
    (calculate_generator_length(coin_spends) as u64 - 2) * MAINNET_CONSTANTS.cost_per_byte
}

/// The cost of a condition, plus the cost charged for each spend after the hard fork.
fn condition_cost(height: u32, pre_fork_cost: u64, post_fork_cost: u64) -> u64 {
    if is_post_fork(height) {
        post_fork_cost + SPEND_COST
    } else {
        pre_fork_cost
    }
}

/// `test_create_coin_cost`
#[rstest]
fn test_create_coin_cost(#[values(pre_fork(), post_fork())] height: u32) -> anyhow::Result<()> {
    let program = format!(r#"(51 "{PUZZLE_HASH}" 10)"#);
    let conds = conditions(&program, height)?;

    assert_eq!(
        conds.condition_cost,
        condition_cost(height, CREATE_COIN_COST, NEW_CREATE_COIN_COST)
    );
    assert_eq!(conds.spends.len(), 1);
    assert_eq!(conds.spends[0].create_coin.len(), 1);

    if !is_post_fork(height) {
        let coin_spends = [spend(&program, true, 123)?];
        assert_eq!(
            conds.cost,
            conds.execution_cost + conds.condition_cost + pre_fork_byte_cost(&coin_spends)
        );
    }

    Ok(())
}

/// `test_agg_sig_cost`
#[rstest]
fn test_agg_sig_cost(
    #[values(43, 44, 45, 46, 47, 48, 49, 50)] opcode: u8,
    #[values(pre_fork(), post_fork())] height: u32,
) -> anyhow::Result<()> {
    let public_key = hex::encode(public_key().to_bytes());
    let program = format!(r#"({opcode} 0x{public_key} "foobar")"#);
    let conds = conditions(&program, height)?;

    assert_eq!(
        conds.condition_cost,
        condition_cost(height, AGG_SIG_COST, AGG_SIG_COST)
    );
    assert_eq!(conds.spends.len(), 1);

    if !is_post_fork(height) {
        let coin_spends = [spend(&program, true, 123)?];
        assert_eq!(
            conds.cost,
            conds.execution_cost + conds.condition_cost + pre_fork_byte_cost(&coin_spends)
        );
    }

    Ok(())
}

/// `test_agg_sig_extra_arg`, in mempool mode, which doesn't allow extra arguments.
#[rstest]
fn test_agg_sig_extra_arg(
    #[values(43, 44, 45, 46, 47, 48, 49, 50)] opcode: u8,
    #[values(pre_fork(), post_fork())] height: u32,
) {
    let public_key = hex::encode(public_key().to_bytes());
    assert!(conditions(&format!(r#"({opcode} 0x{public_key} "foobar")"#), height).is_ok());
    assert!(
        conditions(
            &format!(r#"({opcode} 0x{public_key} "foobar" "baz")"#),
            height
        )
        .is_err()
    );
}

/// `test_unknown_condition`, in mempool mode, where unknown conditions are an error.
#[rstest]
fn test_unknown_condition(
    #[values(
        r#"(2 100 "foo" "bar")"#,
        "(100)",
        "(4 1) (2 2) (3 3)",
        r#"("foobar")"#,
        r#"(0x100 "foobar")"#,
        r#"(0x1ff "foobar")"#
    )]
    program: &str,
    #[values(pre_fork(), post_fork())] height: u32,
) {
    assert!(conditions(program, height).is_err());
}

/// `test_softfork_condition`, in mempool mode, where the softfork condition is always an error.
#[rstest]
fn test_softfork_condition(
    #[values("(90)", "(90 1000000)")] program: &str,
    #[values(pre_fork(), post_fork())] height: u32,
) {
    assert!(conditions(program, height).is_err());
}

/// `test_message_condition`
#[rstest]
fn test_message_condition(#[values(pre_fork(), post_fork())] height: u32) {
    assert!(conditions(r#"(66 0 "foo") (67 0 "bar")"#, height).is_err());
    assert!(conditions(r#"(66 0 "foo") (67 0 "foo")"#, height).is_ok());
}

/// `test_duplicate_outputs`
#[rstest]
fn test_duplicate_outputs(#[values(pre_fork(), post_fork())] height: u32) {
    let program = format!(r#"(51 "{PUZZLE_HASH}" 10) (51 "{PUZZLE_HASH}" 10)"#);
    assert!(matches!(
        conditions(&program, height),
        Err(DriverError::Validation(_))
    ));
}

/// `test_multiple_reserve_fee`
#[rstest]
fn test_multiple_reserve_fee(#[values(pre_fork(), post_fork())] height: u32) -> anyhow::Result<()> {
    let conds = conditions("(52 10) (52 10) (52 10)", height)?;
    assert_eq!(conds.reserve_fee, 30);
    assert_eq!(conds.spends.len(), 1);
    Ok(())
}

/// `test_many_create_coin`
#[rstest]
fn test_many_create_coin(#[values(pre_fork(), post_fork())] height: u32) -> anyhow::Result<()> {
    let program = format!(
        r#"(a (q 2 6 (c 2 (c (q 51 "{PUZZLE_HASH}") (c 5 ())))) (c (q (a (i 5 (q 4 9 (a 4 (c 2 (c 13 (c 11 ()))))) (q 4 11 ())) 1) 2 (i 11 (q 4 (a 4 (c 2 (c 5 (c 11 ())))) (a 6 (c 2 (c 5 (c (- 11 (q . 1)) ()))))) ()) 1) (q 600)))"#
    );
    let coin_spends = [spend(&program, false, 123_000_000)?];
    let conds = spend_bundle_conditions(&coin_spends, &MAINNET_CONSTANTS, height)?;
    assert_eq!(conds.spends.len(), 1);
    assert_eq!(conds.spends[0].create_coin.len(), 600);
    assert_eq!(
        conds.condition_cost,
        condition_cost(height, 600 * CREATE_COIN_COST, 600 * NEW_CREATE_COIN_COST)
    );
    Ok(())
}

/// `test_block_cost_exceeds_max`, from the mempool manager tests: a transaction can cost at most
/// half of a block.
#[rstest]
fn test_block_cost_exceeds_max(#[values(pre_fork(), post_fork())] height: u32) {
    let program = (0..2400)
        .map(|i| format!(r#"(51 "{PUZZLE_HASH}" {i})"#))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(conditions(&program, height).is_err());
}

/// `test_keccak`: the keccak operator is only available outside the softfork guard after the hard
/// fork, so the mempool rejects it before then.
#[test]
fn test_keccak() -> anyhow::Result<()> {
    let program = r#"(a (i (= (62 (q . "foobar")) (q . 0x38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e)) (q q (51 0x0101010101010101010101010101010101010101010101010101010101010101 1)) (q x)) ())"#;
    let coin_spends = [spend(program, false, 1)?];

    assert!(spend_bundle_conditions(&coin_spends, &MAINNET_CONSTANTS, pre_fork()).is_err());
    assert!(spend_bundle_conditions(&coin_spends, &MAINNET_CONSTANTS, post_fork()).is_ok());

    // With no height, the cost can't be calculated, since the spend is invalid before the fork.
    assert!(spend_bundle_cost(&coin_spends, &MAINNET_CONSTANTS, None).is_err());

    Ok(())
}

/// `test_basics`, from the cost calculation tests: a standard transaction's cost is its condition
/// cost, plus its execution cost, plus its byte cost.
#[rstest]
fn test_standard_transaction(#[values(pre_fork(), post_fork())] height: u32) -> anyhow::Result<()> {
    let mut ctx = SpendContext::new();
    let public_key = public_key();
    let layer = StandardLayer::new(public_key);
    let coin = Coin::new(
        Bytes32::default(),
        StandardArgs::curry_tree_hash(public_key).into(),
        250_000_000_000,
    );
    layer.spend(
        &mut ctx,
        coin,
        Conditions::new().create_coin(Bytes32::new([b'0'; 32]), coin.amount, Memos::None),
    )?;
    let coin_spends = ctx.take();

    let conds = spend_bundle_conditions(&coin_spends, &MAINNET_CONSTANTS, height)?;

    assert_eq!(
        conds.condition_cost,
        condition_cost(
            height,
            CREATE_COIN_COST + AGG_SIG_COST,
            NEW_CREATE_COIN_COST + AGG_SIG_COST
        )
    );
    assert_eq!(conds.spends[0].coin_id, coin.coin_id());

    if !is_post_fork(height) {
        assert_eq!(
            conds.cost,
            conds.execution_cost + conds.condition_cost + pre_fork_byte_cost(&coin_spends)
        );
    }

    assert_eq!(
        spend_bundle_cost(&coin_spends, &MAINNET_CONSTANTS, Some(height))?,
        conds.cost
    );

    Ok(())
}

/// The fee the mempool sees is the amount a transaction removes minus the amount it adds. A fee of
/// [`minimum_fee`] passes both the full mempool check (`fees / cost >= 5`, from
/// `nonzero_fee_minimum_fpc` in `mempool_manager.py`) and the minimum block fee from
/// chia-blockchain#21649 (`fee >= ceil(cost * 4_000_000 / 1_000_000)`).
#[rstest]
fn test_minimum_fee_passes_the_mempool(
    #[values(pre_fork(), post_fork())] height: u32,
) -> anyhow::Result<()> {
    let public_key = public_key();
    let coin = Coin::new(
        Bytes32::default(),
        StandardArgs::curry_tree_hash(public_key).into(),
        1_000_000_000_000,
    );

    let build = |fee: u64| -> anyhow::Result<Vec<CoinSpend>> {
        let mut ctx = SpendContext::new();
        StandardLayer::new(public_key).spend(
            &mut ctx,
            coin,
            Conditions::new()
                .create_coin(Bytes32::new([b'0'; 32]), coin.amount - fee, Memos::None)
                .reserve_fee(fee),
        )?;
        Ok(ctx.take())
    };

    // The fee is part of the spend, so its cost is computed with the fee in place.
    let mut fee = 0;
    loop {
        let cost = spend_bundle_cost(&build(fee)?, &MAINNET_CONSTANTS, Some(height))?;
        let needed = minimum_fee(cost);
        if fee >= needed {
            break;
        }
        fee = needed;
    }

    let conds = spend_bundle_conditions(&build(fee)?, &MAINNET_CONSTANTS, height)?;
    let mempool_fee = u64::try_from(conds.removal_amount - conds.addition_amount)?;
    assert_eq!(mempool_fee, fee);

    #[allow(clippy::cast_precision_loss)]
    let fees_per_cost = mempool_fee as f64 / conds.cost as f64;
    assert!(fees_per_cost >= 5.0);
    assert!(mempool_fee >= (conds.cost * 4_000_000).div_ceil(1_000_000));

    Ok(())
}
