use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::Condition;
use clvmr::Allocator;

use crate::{
    BURN_PUZZLE_HASH, Cat, DriverError, Facts, Nft, OfferPreSplitInfo, P2PuzzleType, ParsedAsset,
    ParsedMemos, RevealedCoinSpend, RevealedP2Puzzle, Reveals, parse_memos,
};

#[derive(Debug, Clone)]
pub struct ParsedChild {
    pub asset: ParsedAsset,
    pub memos: ParsedMemos,
    /// Classification of where this child's value is going, based on its inner p2 puzzle hash.
    pub p2_puzzle_type: P2PuzzleType,
}

pub fn parse_children(
    facts: &mut Facts,
    allocator: &mut Allocator,
    reveals: &Reveals,
    asset: &ParsedAsset,
    spend: &RevealedCoinSpend,
    conditions: &[Condition],
    is_claw_back: bool,
) -> Result<Vec<ParsedChild>, DriverError> {
    // Now, we should be able to assume that the conditions will be output if the transaction is valid.
    let mut children = Vec::new();

    // We should parse CAT children up front, so that we can hydrate their details when adding children later.
    let mut cats = if matches!(asset, ParsedAsset::Cat(_)) {
        if let Some(cats) =
            Cat::parse_children(allocator, spend.coin, spend.puzzle, spend.solution)?
        {
            cats
        } else {
            return Err(DriverError::MissingChild);
        }
    } else {
        Vec::new()
    };

    for condition in conditions {
        match condition {
            Condition::AssertPuzzleAnnouncement(condition) => {
                facts.assert_puzzle_announcement(condition.announcement_id);
            }
            Condition::AssertConcurrentSpend(condition) => {
                facts.assert_spend(condition.coin_id);
            }
            Condition::AssertBeforeSecondsAbsolute(condition) => {
                // We shouldn't allow a claw back spend to say when the transaction will expire.
                // Otherwise, it could pretend that it's impossible for the clawback to expire
                // before the transaction expires, which would be a security vulnerability.
                if !is_claw_back {
                    facts.update_actual_expiration_time(condition.seconds);
                }
            }
            Condition::ReserveFee(condition) => {
                facts.add_reserved_fees(condition.amount);
            }
            Condition::CreateCoin(condition) => {
                let parsed_child = match &asset {
                    // All XCH children are considered to be XCH by default.
                    ParsedAsset::Xch(_) => {
                        let memos = parse_memos(allocator, *condition, false);
                        ParsedChild {
                            asset: ParsedAsset::Xch(Coin::new(
                                spend.coin.coin_id(),
                                condition.puzzle_hash,
                                condition.amount,
                            )),
                            p2_puzzle_type: classify_p2_puzzle(&memos, None, reveals),
                            memos,
                        }
                    }
                    // For NFTs, even amount children are XCH coins. If the amount is odd, it's the
                    // next NFT singleton coin in the lineage.
                    ParsedAsset::Nft(_) => {
                        if condition.amount % 2 == 1 {
                            let Some(nft) = Nft::parse_child(
                                allocator,
                                spend.coin,
                                spend.puzzle,
                                spend.solution,
                            )?
                            else {
                                return Err(DriverError::MissingChild);
                            };

                            let memos = parse_memos(allocator, *condition, true);
                            ParsedChild {
                                p2_puzzle_type: classify_p2_puzzle(
                                    &memos,
                                    Some(nft.info.p2_puzzle_hash),
                                    reveals,
                                ),
                                asset: ParsedAsset::Nft(nft),
                                memos,
                            }
                        } else {
                            let memos = parse_memos(allocator, *condition, false);
                            ParsedChild {
                                asset: ParsedAsset::Xch(Coin::new(
                                    spend.coin.coin_id(),
                                    condition.puzzle_hash,
                                    condition.amount,
                                )),
                                p2_puzzle_type: classify_p2_puzzle(&memos, None, reveals),
                                memos,
                            }
                        }
                    }
                    // CATs never output anything other than CAT children.
                    ParsedAsset::Cat(parent) => {
                        let cat = cats.remove(0);

                        // This prevents an attack where someone tricks you into spending a CAT, sending it
                        // back to you, and wrapping it in a revocation layer that they control.
                        if cat.info.hidden_puzzle_hash.is_some()
                            && parent.info.hidden_puzzle_hash.is_none()
                        {
                            return Err(DriverError::RevocableChild);
                        }

                        let memos = parse_memos(allocator, *condition, true);
                        ParsedChild {
                            p2_puzzle_type: classify_p2_puzzle(&memos, None, reveals),
                            asset: ParsedAsset::Cat(cat),
                            memos,
                        }
                    }
                };
                children.push(parsed_child);
            }
            _ => {}
        }
    }

    Ok(children)
}

/// Classify a child by its inner p2 puzzle hash.
///
/// `nft_inner_p2` is `Some(_)` only for NFT children, where the inner p2 lives in the parsed NFT
/// info rather than in `memos.p2_puzzle_hash` (which would be the full singleton puzzle hash for
/// NFTs). For all other asset kinds, `memos.p2_puzzle_hash` already gives the inner p2, including
/// the receiver puzzle hash for clawback-wrapped outputs.
fn classify_p2_puzzle(
    memos: &ParsedMemos,
    nft_inner_p2: Option<Bytes32>,
    reveals: &Reveals,
) -> P2PuzzleType {
    let inner_p2 = nft_inner_p2.unwrap_or(memos.p2_puzzle_hash);

    if inner_p2 == SETTLEMENT_PAYMENT_HASH.into() {
        return P2PuzzleType::Offered;
    }

    if inner_p2 == BURN_PUZZLE_HASH {
        return P2PuzzleType::Burned;
    }

    // A clawback-wrapped output can't safely be classified as `OfferPreSplit`, because the
    // on-chain puzzle is the clawback rather than the `P2ConditionsOrSingleton` itself. The
    // singleton-path cancellation that justifies the trust wouldn't be available.
    if memos.clawback.is_some() {
        return P2PuzzleType::Unknown;
    }

    let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(reveal)) =
        reveals.p2_puzzle(inner_p2.into())
    else {
        return P2PuzzleType::Unknown;
    };

    P2PuzzleType::OfferPreSplit(OfferPreSplitInfo {
        launcher_id: reveal.launcher_id,
        nonce: reveal.nonce,
        fixed_delegated_puzzle_hash: reveal.fixed_delegated_puzzle_hash,
        fixed_conditions: reveal.fixed_conditions.clone(),
        settlement_amount: settlement_amount(&reveal.fixed_conditions),
    })
}

/// Sum of `CreateCoin` amounts in the fixed conditions whose puzzle hash is the settlement puzzle
/// hash. For CAT pre-splits, the inner conditions' `CreateCoin` already targets the unwrapped
/// settlement puzzle hash, so this comparison works for both XCH and CAT.
fn settlement_amount(fixed_conditions: &[Condition]) -> u64 {
    fixed_conditions
        .iter()
        .filter_map(|condition| match condition {
            Condition::CreateCoin(create_coin)
                if create_coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() =>
            {
                Some(create_coin.amount)
            }
            _ => None,
        })
        .sum()
}
