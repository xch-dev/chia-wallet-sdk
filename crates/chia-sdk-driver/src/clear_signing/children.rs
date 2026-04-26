use chia_protocol::Coin;
use chia_sdk_types::Condition;
use clvmr::Allocator;

use crate::{
    Cat, DriverError, Facts, Nft, ParsedAsset, ParsedMemos, RevealedCoinSpend, parse_memos,
};

#[derive(Debug, Clone)]
pub struct ParsedChild {
    pub asset: ParsedAsset,
    pub memos: ParsedMemos,
}

pub fn parse_children(
    facts: &mut Facts,
    allocator: &mut Allocator,
    asset: &ParsedAsset,
    spend: &RevealedCoinSpend,
    conditions: &[Condition],
    is_claw_back: bool,
) -> Result<Vec<ParsedChild>, DriverError> {
    // Now, we should be able to assume that the conditions will be output if the transaction is valid.
    let mut children = Vec::new();
    let mut child_index = 0;

    // We should parse CAT children up front, so that we can hydrate their details when adding children later.
    let cats = if matches!(asset, ParsedAsset::Cat(_)) {
        Cat::parse_children(allocator, spend.coin, spend.puzzle, spend.solution)?
            .unwrap_or_default()
    } else {
        vec![]
    };

    for condition in conditions {
        match condition {
            Condition::AssertPuzzleAnnouncement(condition) => {
                facts.assert_puzzle_announcement(condition.announcement_id);
            }
            Condition::AssertBeforeSecondsAbsolute(condition) => {
                // We shouldn't allow a claw back spend to say when the transaction will expire.
                // Otherwise, it could pretend that it's impossible for the clawback to expire
                // before the transaction expires, which would be a security vulnerability.
                if is_claw_back {
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
                        children.push(ParsedChild {
                            asset: ParsedAsset::Xch(Coin::new(
                                spend.coin.coin_id(),
                                condition.puzzle_hash,
                                condition.amount,
                            )),
                            memos: parse_memos(allocator, *condition, false),
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

                            children.push(ParsedChild {
                                asset: ParsedAsset::Nft(nft),
                                memos: parse_memos(allocator, *condition, true),
                            });
                        } else {
                            children.push(ParsedChild {
                                asset: ParsedAsset::Xch(Coin::new(
                                    spend.coin.coin_id(),
                                    condition.puzzle_hash,
                                    condition.amount,
                                )),
                                memos: parse_memos(allocator, *condition, false),
                            });
                        }
                    }
                    // CATs never output anything other than CAT children.
                    ParsedAsset::Cat(parent) => {
                        if let Some(cat) = cats.get(child_index) {
                            // This prevents an attack where someone tricks you into spending a CAT, sending it
                            // back to you, and wrapping it in a revocation layer that they control.
                            if cat.info.hidden_puzzle_hash.is_some()
                                && parent.info.hidden_puzzle_hash.is_none()
                            {
                                return Err(DriverError::RevocableChild);
                            }

                            children.push(ParsedChild {
                                asset: ParsedAsset::Cat(*cat),
                                memos: parse_memos(allocator, *condition, true),
                            });
                        } else {
                            return Err(DriverError::MissingChild);
                        }
                    }
                }

                // This is for indexing into the CAT children.
                child_index += 1;
            }
            _ => {}
        }
    }

    Ok(children)
}
