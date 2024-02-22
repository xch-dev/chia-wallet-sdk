use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_wallet::cat::{
    CatArgs, CatSolution, CoinProof, EverythingWithSignatureTailArgs, CAT_PUZZLE_HASH,
};
use clvm_traits::{clvm_quote, FromNodePtr, ToClvmError, ToNodePtr};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{Allocator, NodePtr};

use crate::{CatCondition, Condition, RunTail};

/// The information required to create and spend an eve CAT coin.
pub struct EveSpendInfo {
    /// The full puzzle hash of the eve CAT coin.
    pub puzzle_hash: [u8; 32],
    /// The coin spend for the eve CAT.
    pub coin_spend: CoinSpend,
}

/// Constructs a coin spend to issue more of an `EverythingWithSignature` CAT.
pub fn issue_cat_with_signature(
    a: &mut Allocator,
    cat_puzzle_ptr: NodePtr,
    tail_puzzle_ptr: NodePtr,
    public_key: PublicKey,
    parent_coin_id: Bytes32,
    amount: u64,
    conditions: &[Condition<NodePtr>],
) -> Result<EveSpendInfo, ToClvmError> {
    let mut cat_conditions: Vec<CatCondition<NodePtr>> = Vec::with_capacity(conditions.len() + 1);
    cat_conditions.extend(
        conditions
            .iter()
            .map(|condition| CatCondition::Normal(condition.clone())),
    );

    let tail = CurriedProgram {
        program: tail_puzzle_ptr,
        args: EverythingWithSignatureTailArgs { public_key },
    }
    .to_node_ptr(a)?;

    cat_conditions.push(CatCondition::RunTail(RunTail {
        program: tail,
        solution: NodePtr::NIL,
    }));

    issue_cat_eve(
        a,
        cat_puzzle_ptr,
        parent_coin_id,
        tree_hash(a, tail),
        amount,
        &cat_conditions,
    )
}

/// Creates an eve CAT coin and spends it.
pub fn issue_cat_eve(
    a: &mut Allocator,
    cat_puzzle_ptr: NodePtr,
    parent_coin_id: Bytes32,
    tail_program_hash: [u8; 32],
    amount: u64,
    conditions: &[CatCondition<NodePtr>],
) -> Result<EveSpendInfo, ToClvmError> {
    let inner_puzzle = clvm_quote!(conditions).to_node_ptr(a)?;
    let inner_puzzle_hash = tree_hash(a, inner_puzzle);

    let puzzle = CurriedProgram {
        program: cat_puzzle_ptr,
        args: CatArgs {
            mod_hash: CAT_PUZZLE_HASH.into(),
            tail_program_hash: tail_program_hash.into(),
            inner_puzzle,
        },
    }
    .to_node_ptr(a)?;

    let puzzle_hash = tree_hash(a, puzzle);
    let coin = Coin::new(parent_coin_id, puzzle_hash.into(), amount);

    let solution = CatSolution {
        inner_puzzle_solution: (),
        lineage_proof: None,
        prev_coin_id: coin.coin_id().into(),
        this_coin_info: coin.clone(),
        next_coin_proof: CoinProof {
            parent_coin_info: parent_coin_id,
            inner_puzzle_hash: inner_puzzle_hash.into(),
            amount,
        },
        prev_subtotal: 0,
        extra_delta: 0,
    }
    .to_node_ptr(a)?;

    let coin_spend = CoinSpend::new(
        coin,
        Program::from_node_ptr(a, puzzle).unwrap(),
        Program::from_node_ptr(a, solution).unwrap(),
    );

    Ok(EveSpendInfo {
        puzzle_hash,
        coin_spend,
    })
}
