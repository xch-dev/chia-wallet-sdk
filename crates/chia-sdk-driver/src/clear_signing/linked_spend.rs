use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::{Condition, conditions::CreateCoin};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{
    Cat, ClawbackInfo, ClawbackPath, ClawbackV2, CustodyInfo, DriverError, Facts, Nft,
    P2SingletonInfo, VaultMessage, parse_inner_spend,
};

#[derive(Debug, Clone)]
pub struct LinkedSpendSummary {
    pub asset: ParsedAsset,
    pub clawback: Option<ClawbackInfo>,
    pub p2_singleton: P2SingletonInfo,
    pub children: Vec<ParsedChild>,
}

#[derive(Debug, Clone, Copy)]
pub enum ParsedAsset {
    Cat(Cat),
    Nft(Nft),
    Xch(Coin),
}

impl ParsedAsset {
    pub fn coin(&self) -> Coin {
        match self {
            Self::Cat(cat) => cat.coin,
            Self::Nft(nft) => nft.coin,
            Self::Xch(coin) => *coin,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedChild {
    pub asset: ParsedAsset,
    pub memos: ParsedMemos,
}

#[derive(Debug, Clone)]
pub struct ParsedMemos {
    pub p2_puzzle_hash: Bytes32,
    pub clawback: Option<ClawbackV2>,
    pub human_readable_memos: Vec<String>,
}

pub fn parse_linked_spend(
    facts: &mut Facts,
    allocator: &mut Allocator,
    vault_message: VaultMessage,
) -> Result<LinkedSpendSummary, DriverError> {
    let Some(spend) = facts.coin_spend(vault_message.spent_coin_id).copied() else {
        return Err(DriverError::MissingSpend);
    };

    // The default is to treat the spend as XCH if we don't have a more complex asset to try and parse.
    let mut asset = ParsedAsset::Xch(spend.coin);
    let mut inner_puzzle = spend.puzzle;
    let mut inner_solution = spend.solution;

    if let Some((cat, parsed_inner_puzzle, parsed_inner_solution)) =
        Cat::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        asset = ParsedAsset::Cat(cat);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
    } else if let Some((nft, parsed_inner_puzzle, parsed_inner_solution)) =
        Nft::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        asset = ParsedAsset::Nft(nft);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
    }

    let inner_spend = parse_inner_spend(facts, allocator, inner_puzzle, inner_solution)?;

    let Some(CustodyInfo::P2Singleton(
        p2_singleton_info @ P2SingletonInfo {
            // We don't need to check the launcher id, since we know that this coin must receive a
            // message from the vault spend in order for the transaction to be valid. Thus, the launcher
            // id of the vault must be identical to the launcher id of this coin.
            launcher_id: _,

            // There's also no reason to check the nonce used for the coin, since it's owned by the vault
            // regardless.
            nonce: _,

            // We are only really interested in the conditions of the custody spend.
            conditions,

            // The p2 puzzle hash is derived from the launcher id and nonce anyways. It's here for convenience.
            p2_puzzle_hash: _,
        },
    )) = &inner_spend.custody
    else {
        return Err(DriverError::InvalidLinkedCustody);
    };

    // If we're clawing a coin back, we need to keep track of its expiration time.
    // This will be used to ensure that the clawback won't expire before the rest of
    // the transaction. If it might, the facts of this spend will be disregarded.
    let mut required_expiration_time = None;

    if let Some(clawback_info) = &inner_spend.clawback
        && clawback_info.path == ClawbackPath::Sender
    {
        required_expiration_time = Some(clawback_info.clawback.seconds);
    }

    if let Some(required_expiration_time) = required_expiration_time {
        facts.update_required_expiration_time(required_expiration_time);
    }

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
                if required_expiration_time.is_none() {
                    facts.update_expiration_time(condition.seconds);
                }
            }
            Condition::ReserveFee(condition) => {
                facts.add_reserved_fees(condition.amount);
            }
            Condition::CreateCoin(condition) => {
                let child_coin = Coin::new(
                    spend.coin.coin_id(),
                    condition.puzzle_hash,
                    condition.amount,
                );

                match &asset {
                    // All XCH children are considered to be XCH by default.
                    ParsedAsset::Xch(_) => {
                        children.push(ParsedChild {
                            asset: ParsedAsset::Xch(child_coin),
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
                            .filter(|nft| nft.coin.coin_id() == child_coin.coin_id()) else {
                                return Err(DriverError::MissingChild);
                            };

                            children.push(ParsedChild {
                                asset: ParsedAsset::Nft(nft),
                                memos: parse_memos(allocator, *condition, true),
                            });
                        } else {
                            children.push(ParsedChild {
                                asset: ParsedAsset::Xch(child_coin),
                                memos: parse_memos(allocator, *condition, false),
                            });
                        }
                    }
                    // CATs never output anything other than CAT children.
                    ParsedAsset::Cat(_) => {
                        if let Some(cat) = cats
                            .get(child_index)
                            .filter(|cat| cat.coin.coin_id() == child_coin.coin_id())
                        {
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

    Ok(LinkedSpendSummary {
        asset,
        clawback: inner_spend.clawback,
        p2_singleton: p2_singleton_info.clone(),
        children,
    })
}

fn parse_memos(
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
        };
    }

    // If we're parsing a CAT output, we can remove the hint from the memos if applicable.
    if requires_hint && let Ok((_hint, rest)) = <(Bytes32, NodePtr)>::from_clvm(allocator, memos) {
        return ParsedMemos {
            p2_puzzle_hash: p2_create_coin.puzzle_hash,
            clawback: None,
            human_readable_memos: parse_memo_list(allocator, rest),
        };
    }

    // Otherwise, we assume there's no clawback and return the memos as is, if they are bytes.
    ParsedMemos {
        p2_puzzle_hash: p2_create_coin.puzzle_hash,
        clawback: None,
        human_readable_memos: parse_memo_list(allocator, memos),
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
