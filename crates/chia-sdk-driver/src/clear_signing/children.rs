use std::collections::VecDeque;

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::CreateCoin};

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

pub fn parse_children(
    reveals: &mut Reveals,
    facts: &mut Facts,
    ctx: &mut SpendContext,
    asset: &ParsedAsset,
    spend: RevealedCoinSpend,
    conditions: &[Condition],
    is_claw_back: bool,
) -> Result<Vec<ParsedChild>, DriverError> {
    // Now, we should be able to assume that the conditions will be output if the transaction is valid.
    let mut children = Vec::new();

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
            Condition::CreateCoin(condition) => {
                match &asset {
                    // All XCH and bulletin children are considered to be XCH.
                    // However, ephemeral bulletin children are hydrated later based on the spend.
                    ParsedAsset::Xch(_) | ParsedAsset::Bulletin(_) => {
                        let memos = parse_memos(reveals, ctx, *condition, false);
                        let transfer_type =
                            calculate_transfer_type(reveals, &memos, condition.amount);

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
                            let transfer_type =
                                calculate_transfer_type(reveals, &memos, condition.amount);

                            children.push(ParsedChild {
                                asset: ParsedAsset::Nft(nft),
                                memos,
                                transfer_type,
                            });
                        } else {
                            let memos = parse_memos(reveals, ctx, *condition, false);
                            let transfer_type =
                                calculate_transfer_type(reveals, &memos, condition.amount);

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
                        let transfer_type =
                            calculate_transfer_type(reveals, &memos, condition.amount);

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

    Ok(children)
}

fn calculate_transfer_type(
    reveals: &Reveals,
    memos: &ParsedMemos,
    input_amount: u64,
) -> TransferType {
    if memos.p2_puzzle_hash == BURN_PUZZLE_HASH {
        TransferType::Burned
    } else if memos.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
        TransferType::Offered
    } else if memos.clawback.is_none()
        && let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(reveal)) =
            reveals.p2_puzzle(memos.p2_puzzle_hash.into())
        && let Some(fixed_conditions) = &memos.fixed_conditions
    {
        let mut reserved_fee = 0;

        for condition in fixed_conditions {
            if let Condition::ReserveFee(condition) = condition {
                reserved_fee += condition.amount;
            }
        }

        TransferType::OfferPreSplit(OfferPreSplitInfo {
            launcher_id: reveal.launcher_id,
            nonce: reveal.nonce,
            fixed_conditions: fixed_conditions.clone(),
            settlement_amount: input_amount.saturating_sub(reserved_fee),
        })
    } else {
        TransferType::Sent
    }
}
