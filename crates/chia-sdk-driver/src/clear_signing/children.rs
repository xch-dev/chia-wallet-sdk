use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::Condition;
use clvmr::Allocator;

use crate::{
    BURN_PUZZLE_HASH, Cat, DriverError, Facts, Nft, ParsedAsset, ParsedMemos, RevealedCoinSpend,
    RevealedP2Puzzle, Reveals, parse_memos,
};

#[derive(Debug, Clone)]
pub struct ParsedChild {
    pub asset: ParsedAsset,
    pub memos: ParsedMemos,
    pub transfer_type: TransferType,
}

#[derive(Debug, Clone)]
pub enum TransferType {
    Sent,
    Burned,
    Offered,
    OfferPreSplit(OfferPreSplitInfo),
}

#[derive(Debug, Clone)]
pub struct OfferPreSplitInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub fixed_conditions: Vec<Condition>,
    pub settlement_amount: u64,
}

pub fn parse_children(
    reveals: &Reveals,
    facts: &mut Facts,
    allocator: &mut Allocator,
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
                match &asset {
                    // All XCH children are considered to be XCH by default.
                    ParsedAsset::Xch(_) => {
                        let memos = parse_memos(allocator, *condition, false);
                        let transfer_type = calculate_transfer_type(reveals, &memos);

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
                            let transfer_type = calculate_transfer_type(reveals, &memos);

                            children.push(ParsedChild {
                                asset: ParsedAsset::Nft(nft),
                                memos,
                                transfer_type,
                            });
                        } else {
                            let memos = parse_memos(allocator, *condition, false);
                            let transfer_type = calculate_transfer_type(reveals, &memos);

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
                        let cat = cats.remove(0);

                        // This prevents an attack where someone tricks you into spending a CAT, sending it
                        // back to you, and wrapping it in a revocation layer that they control.
                        if cat.info.hidden_puzzle_hash.is_some()
                            && parent.info.hidden_puzzle_hash.is_none()
                        {
                            return Err(DriverError::RevocableChild);
                        }

                        let memos = parse_memos(allocator, *condition, true);
                        let transfer_type = calculate_transfer_type(reveals, &memos);

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

fn calculate_transfer_type(reveals: &Reveals, memos: &ParsedMemos) -> TransferType {
    if memos.p2_puzzle_hash == BURN_PUZZLE_HASH {
        TransferType::Burned
    } else if memos.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
        TransferType::Offered
    } else if memos.clawback.is_none()
        && let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(p2_puzzle)) =
            reveals.p2_puzzle(memos.p2_puzzle_hash.into())
    {
        let mut settlement_amount = 0;

        for condition in &p2_puzzle.fixed_conditions {
            if let Some(condition) = condition.as_create_coin()
                && condition.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
            {
                settlement_amount += condition.amount;
            }
        }

        TransferType::OfferPreSplit(OfferPreSplitInfo {
            launcher_id: p2_puzzle.launcher_id,
            nonce: p2_puzzle.nonce,
            fixed_conditions: p2_puzzle.fixed_conditions.clone(),
            settlement_amount,
        })
    } else {
        TransferType::Sent
    }
}
