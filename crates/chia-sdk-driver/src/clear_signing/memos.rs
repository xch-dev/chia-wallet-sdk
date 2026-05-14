use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_sdk_types::{Condition, conditions::CreateCoin};
use clvm_traits::{FromClvm, clvm_quote};
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{ClawbackV2, RevealedP2Puzzle, Reveals};

#[derive(Debug, Clone)]
pub struct ParsedMemos {
    pub p2_puzzle_hash: Bytes32,
    pub clawback: Option<ClawbackV2>,
    pub human_readable_memos: Vec<String>,
    pub fixed_conditions: Option<Vec<Condition>>,
}

pub fn parse_memos(
    reveals: &Reveals,
    allocator: &Allocator,
    p2_create_coin: CreateCoin<NodePtr>,
    requires_hint: bool,
) -> ParsedMemos {
    // If there is no memo list, there's nothing to parse and we can assume there's no clawback
    let Memos::Some(memos) = p2_create_coin.memos else {
        return ParsedMemos {
            p2_puzzle_hash: p2_create_coin.puzzle_hash,
            clawback: None,
            human_readable_memos: Vec::new(),
            fixed_conditions: None,
        };
    };

    // If there is both a hint and a valid clawback memo that correctly calculates the puzzle hash,
    // then we can parse the clawback and return the rest of the memos, if they are bytes.
    if let Ok((hint, (clawback_memo, rest))) =
        <(Bytes32, (NodePtr, NodePtr))>::from_clvm(allocator, memos)
        && let Some(clawback) = ClawbackV2::from_memo(
            allocator,
            clawback_memo,
            hint,
            p2_create_coin.amount,
            requires_hint,
            p2_create_coin.puzzle_hash,
        )
    {
        return ParsedMemos {
            p2_puzzle_hash: clawback.receiver_puzzle_hash,
            clawback: Some(clawback),
            human_readable_memos: parse_memo_list(allocator, rest),
            fixed_conditions: None,
        };
    }

    // If we're parsing a p2 conditions or singleton, we can try to parse the fixed conditions.
    if let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(p2_conditions_or_singleton)) =
        reveals.p2_puzzle(p2_create_coin.puzzle_hash.into())
        && let Ok((_hint, (memo, rest))) =
            <(Bytes32, (NodePtr, NodePtr))>::from_clvm(allocator, memos)
        && let Ok(conditions) = Vec::<Condition>::from_clvm(allocator, memo)
        && clvm_quote!(tree_hash(allocator, memo)).tree_hash()
            == p2_conditions_or_singleton
                .fixed_delegated_puzzle_hash
                .into()
    {
        return ParsedMemos {
            p2_puzzle_hash: p2_create_coin.puzzle_hash,
            clawback: None,
            human_readable_memos: parse_memo_list(allocator, rest),
            fixed_conditions: Some(conditions),
        };
    }

    // If we're parsing a CAT output, we can remove the hint from the memos if applicable.
    if requires_hint && let Ok((_hint, rest)) = <(Bytes32, NodePtr)>::from_clvm(allocator, memos) {
        return ParsedMemos {
            p2_puzzle_hash: p2_create_coin.puzzle_hash,
            clawback: None,
            human_readable_memos: parse_memo_list(allocator, rest),
            fixed_conditions: None,
        };
    }

    // Otherwise, we assume there's no clawback and return the memos as is, if they are bytes.
    ParsedMemos {
        p2_puzzle_hash: p2_create_coin.puzzle_hash,
        clawback: None,
        human_readable_memos: parse_memo_list(allocator, memos),
        fixed_conditions: None,
    }
}

fn parse_memo_list(allocator: &Allocator, memos: NodePtr) -> Vec<String> {
    let memos = Vec::<NodePtr>::from_clvm(allocator, memos).unwrap_or_default();

    let mut result = Vec::new();

    for memo in memos {
        let Ok(memo) = String::from_clvm(allocator, memo) else {
            continue;
        };

        result.push(memo);
    }

    result
}
