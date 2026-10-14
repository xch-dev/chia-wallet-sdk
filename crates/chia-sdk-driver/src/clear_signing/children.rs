use std::collections::VecDeque;

use chia_consensus::opcodes::{CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, SEND_MESSAGE};
use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::CreateCoin};
use clvm_traits::FromClvm;
use clvmr::NodePtr;
use num_bigint::BigInt;

use crate::{
    BURN_PUZZLE_HASH, Cat, DriverError, Facts, Nft, ParsedAsset, ParsedMemos, RevealedCoinSpend,
    RevealedP2Puzzle, Reveals, SpendContext, parse_memos,
};

#[derive(Debug, Clone)]
pub struct ParsedChild {
    pub asset: ParsedAsset,
    pub memos: ParsedMemos,
    pub transfer_type: TransferType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferType {
    Sent,
    Burned,
    Offered,
    OfferPreSplit(OfferPreSplitInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferPreSplitInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub fixed_conditions: Vec<Condition>,
    pub settlement_amount: u64,
}

#[derive(Debug, Clone)]
pub struct ParsedChildren {
    pub children: Vec<ParsedChild>,
    /// Announcements and messages created by the spend, which aren't otherwise reflected in the summary.
    pub extra_conditions: Vec<Condition>,
}

pub fn parse_children(
    reveals: &mut Reveals,
    facts: &mut Facts,
    ctx: &mut SpendContext,
    asset: &ParsedAsset,
    spend: RevealedCoinSpend,
    conditions: &[Condition],
    is_claw_back: bool,
) -> Result<ParsedChildren, DriverError> {
    // Now, we should be able to assume that the conditions will be output if the transaction is valid.
    let mut children = Vec::new();
    let mut extra_conditions = Vec::new();

    // We should parse CAT children up front, so that we can hydrate their details when adding children later.
    let mut cats = if matches!(asset, ParsedAsset::Cat(_)) {
        if let Some(cats) = Cat::parse_children(ctx, spend.coin, spend.puzzle, spend.solution)? {
            VecDeque::from(cats)
        } else {
            return Err(DriverError::MissingChild);
        }
    } else {
        VecDeque::new()
    };

    for condition in conditions {
        match condition {
            Condition::AssertPuzzleAnnouncement(condition) => {
                facts.assert_puzzle_announcement(condition.announcement_id);
            }
            Condition::AssertConcurrentSpend(condition) => {
                facts.assert_spend(condition.coin_id);
            }
            // We shouldn't allow a claw back spend to say when the transaction will expire.
            // Otherwise, it could pretend that it's impossible for the clawback to expire
            // before the transaction expires, which would be a security vulnerability.
            Condition::AssertBeforeSecondsAbsolute(condition) if !is_claw_back => {
                facts.update_actual_expiration_time(condition.seconds);
            }
            Condition::ReserveFee(condition) => {
                facts.add_reserved_fees(condition.amount);
            }
            // These conditions can authorize other spends which assert them, so they must be surfaced.
            Condition::CreateCoinAnnouncement(_)
            | Condition::CreatePuzzleAnnouncement(_)
            | Condition::SendMessage(_) => {
                extra_conditions.push(condition.clone());
            }
            Condition::Other(raw_condition) => {
                let (opcode, _) = <(BigInt, NodePtr)>::from_clvm(&**ctx, *raw_condition)?;

                if [
                    CREATE_COIN_ANNOUNCEMENT,
                    CREATE_PUZZLE_ANNOUNCEMENT,
                    SEND_MESSAGE,
                ]
                .into_iter()
                .any(|extra_opcode| opcode == BigInt::from(extra_opcode))
                {
                    extra_conditions.push(condition.clone());
                }
            }
            Condition::CreateCoin(condition) => {
                match &asset {
                    // All XCH and bulletin children are considered to be XCH.
                    // However, ephemeral bulletin children are hydrated later based on the spend.
                    ParsedAsset::Xch(_) | ParsedAsset::Bulletin(_) => {
                        let memos = parse_memos(reveals, ctx, *condition, false);
                        let transfer_type = calculate_transfer_type(reveals, &memos)?;

                        children.push(ParsedChild {
                            asset: ParsedAsset::Xch(Coin::new(
                                spend.coin.coin_id(),
                                condition.puzzle_hash,
                                condition.amount,
                            )),
                            memos,
                            transfer_type,
                        });
                    }
                    // For NFTs, even amount children are XCH coins. If the amount is odd, it's the
                    // next NFT singleton coin in the lineage.
                    ParsedAsset::Nft(_) => {
                        if condition.amount % 2 == 1 {
                            let Some(nft) =
                                Nft::parse_child(ctx, spend.coin, spend.puzzle, spend.solution)?
                            else {
                                return Err(DriverError::MissingChild);
                            };

                            let memos = parse_memos(reveals, ctx, *condition, true);
                            let transfer_type = calculate_transfer_type(reveals, &memos)?;

                            children.push(ParsedChild {
                                asset: ParsedAsset::Nft(nft),
                                memos,
                                transfer_type,
                            });
                        } else {
                            let memos = parse_memos(reveals, ctx, *condition, false);
                            let transfer_type = calculate_transfer_type(reveals, &memos)?;

                            children.push(ParsedChild {
                                asset: ParsedAsset::Xch(Coin::new(
                                    spend.coin.coin_id(),
                                    condition.puzzle_hash,
                                    condition.amount,
                                )),
                                memos,
                                transfer_type,
                            });
                        }
                    }
                    // CATs never output anything other than CAT children.
                    ParsedAsset::Cat(parent) => {
                        let cat = cats.pop_front().ok_or(DriverError::MissingChild)?;

                        // This prevents an attack where someone tricks you into spending a CAT, sending it
                        // back to you, and wrapping it in a revocation layer that they control.
                        if cat.info.hidden_puzzle_hash != parent.info.hidden_puzzle_hash {
                            return Err(DriverError::RevocableChild);
                        }

                        let memos = parse_memos(
                            reveals,
                            ctx,
                            CreateCoin::new(
                                cat.info.p2_puzzle_hash,
                                condition.amount,
                                condition.memos,
                            ),
                            true,
                        );
                        let transfer_type = calculate_transfer_type(reveals, &memos)?;

                        children.push(ParsedChild {
                            asset: ParsedAsset::Cat(cat),
                            memos,
                            transfer_type,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if !cats.is_empty() {
        return Err(DriverError::MissingChild);
    }

    Ok(ParsedChildren {
        children,
        extra_conditions,
    })
}

fn calculate_transfer_type(
    reveals: &Reveals,
    memos: &ParsedMemos,
) -> Result<TransferType, DriverError> {
    // The p2 puzzle hash of a clawback is the receiver's, but the coin can also be clawed back by
    // the sender, so it can't be treated as anything other than a regular send.
    if memos.clawback.is_some() {
        return Ok(TransferType::Sent);
    }

    Ok(if memos.p2_puzzle_hash == BURN_PUZZLE_HASH {
        TransferType::Burned
    } else if memos.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
        TransferType::Offered
    } else if let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(reveal)) =
        reveals.p2_puzzle(memos.p2_puzzle_hash.into())
        && let Some(fixed_conditions) = &memos.fixed_conditions
    {
        TransferType::OfferPreSplit(OfferPreSplitInfo {
            launcher_id: reveal.launcher_id,
            nonce: reveal.nonce,
            fixed_conditions: fixed_conditions.clone(),
            settlement_amount: calculate_settlement_amount(fixed_conditions)?,
        })
    } else {
        TransferType::Sent
    })
}

/// The fixed conditions of an offer pre-split coin can be output by anyone, so we only allow
/// creating settlement coins, reserving fees, and assertions which restrict when they can be output.
fn calculate_settlement_amount(fixed_conditions: &[Condition]) -> Result<u64, DriverError> {
    let mut settlement_amount = 0_u128;

    for condition in fixed_conditions {
        match condition {
            Condition::CreateCoin(condition)
                if condition.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() =>
            {
                settlement_amount += u128::from(condition.amount);
            }
            Condition::ReserveFee(_)
            | Condition::AssertPuzzleAnnouncement(_)
            | Condition::AssertCoinAnnouncement(_)
            | Condition::AssertConcurrentSpend(_)
            | Condition::AssertConcurrentPuzzle(_)
            | Condition::AssertMyCoinId(_)
            | Condition::AssertMyParentId(_)
            | Condition::AssertMyPuzzleHash(_)
            | Condition::AssertMyAmount(_)
            | Condition::AssertSecondsRelative(_)
            | Condition::AssertSecondsAbsolute(_)
            | Condition::AssertHeightRelative(_)
            | Condition::AssertHeightAbsolute(_)
            | Condition::AssertBeforeSecondsRelative(_)
            | Condition::AssertBeforeSecondsAbsolute(_)
            | Condition::AssertBeforeHeightRelative(_)
            | Condition::AssertBeforeHeightAbsolute(_) => {}
            _ => return Err(DriverError::InvalidOfferPreSplitConditions),
        }
    }

    Ok(settlement_amount.try_into()?)
}
