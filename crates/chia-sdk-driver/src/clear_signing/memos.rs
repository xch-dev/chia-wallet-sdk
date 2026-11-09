use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_sdk_types::{Condition, conditions::CreateCoin};
use clvm_traits::{FromClvm, clvm_quote};
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{ClawbackV2, RevealedP2Puzzle, Reveals};

/// The destination and memos of a created coin.
#[derive(Debug, Clone)]
pub struct ParsedMemos {
    /// The puzzle hash that the coin is effectively sent to. For a clawback, this is the receiver's puzzle hash, and
    /// for a CAT or NFT, it's the p2 puzzle hash. Otherwise, it's the puzzle hash in the condition.
    pub p2_puzzle_hash: Bytes32,
    /// The clawback that the coin is locked in, if the memos describe one that matches the coin's puzzle hash. The
    /// coin only reaches [`p2_puzzle_hash`](ParsedMemos::p2_puzzle_hash) if the sender doesn't claw it back before
    /// the clawback expires.
    pub clawback: Option<ClawbackV2>,
    /// The memos that are valid UTF-8 strings, after the hint and any clawback or fixed conditions memo. These are
    /// arbitrary text chosen by whoever built the transaction. For an XCH child without a clawback or fixed
    /// conditions memo, the hint isn't removed, so it's included if it happens to be valid UTF-8.
    pub human_readable_memos: Vec<String>,
    /// The fixed conditions of a revealed [`P2ConditionsOrSingleton`](crate::P2ConditionsOrSingleton) destination,
    /// if they were revealed in the memos and match its puzzle hash.
    pub fixed_conditions: Option<Vec<Condition>>,
}

/// Parses the memos of a p2 `CREATE_COIN` condition. Only memos that are checked against the coin's puzzle hash
/// (clawbacks and fixed conditions) affect the destination, so the memos can't misrepresent where the coin goes.
///
/// If `requires_hint` is true (for CATs and NFTs), a leading 32 byte memo is treated as the hint and removed. It's also
/// used as the [`hinted`](ClawbackV2::hinted) flag of a clawback, which must match the coin's puzzle hash. The hint is always removed when a clawback or fixed conditions memo follows
/// it.
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
        && p2_conditions_or_singleton
            .match_quoted_conditions_hash(
                clvm_quote!(tree_hash(allocator, memo)).tree_hash().into(),
            )
            .is_some()
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
