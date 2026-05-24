use std::collections::{HashMap, VecDeque};

use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::offer::{NotarizedPayment, Payment, SettlementPaymentsSolution};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, puzzles::SettlementPayment};

use crate::{
    BURN_PUZZLE_HASH, Cat, CatInfo, DriverError, Facts, Nft, ParsedAsset, ParsedMemos, Puzzle,
    RevealedCoinSpend, RevealedP2Puzzle, Reveals, SpendContext, calculate_nft_royalty, parse_memos,
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
                            let Some(nft) =
                                Nft::parse_child(ctx, spend.coin, spend.puzzle, spend.solution)?
                            else {
                                return Err(DriverError::MissingChild);
                            };

                            let memos = parse_memos(reveals, ctx, *condition, true);
                            let transfer_type = calculate_transfer_type(reveals, &memos);

                            children.push(ParsedChild {
                                asset: ParsedAsset::Nft(nft),
                                memos,
                                transfer_type,
                            });
                        } else {
                            let memos = parse_memos(reveals, ctx, *condition, false);
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
                        let cat = cats.pop_front().ok_or(DriverError::MissingChild)?;

                        // This prevents an attack where someone tricks you into spending a CAT, sending it
                        // back to you, and wrapping it in a revocation layer that they control.
                        if cat.info.hidden_puzzle_hash != parent.info.hidden_puzzle_hash {
                            return Err(DriverError::RevocableChild);
                        }

                        let memos = parse_memos(reveals, ctx, *condition, true);
                        let transfer_type = calculate_transfer_type(reveals, &memos);

                        children.push(ParsedChild {
                            asset: ParsedAsset::Cat(cat),
                            memos,
                            transfer_type,
                        });
                    }
                }
            }
            Condition::TransferNft(condition)
                if let ParsedAsset::Nft(parent_nft) = asset
                    && parent_nft.info.royalty_basis_points > 0 =>
            {
                let cats: HashMap<Bytes32, CatInfo> = reveals
                    .asset_info()
                    .cats()
                    .filter_map(|&asset_id| {
                        let hidden_puzzle_hash =
                            reveals.asset_info().cat(asset_id)?.hidden_puzzle_hash;
                        let cat_info = CatInfo::new(
                            asset_id,
                            hidden_puzzle_hash,
                            SETTLEMENT_PAYMENT_HASH.into(),
                        );
                        Some((cat_info.puzzle_hash().into(), cat_info))
                    })
                    .collect();

                let hint = ctx.hint(parent_nft.info.royalty_puzzle_hash)?;

                for trade_price in &condition.trade_prices {
                    let royalty_amount = calculate_nft_royalty(
                        trade_price.amount,
                        parent_nft.info.royalty_basis_points,
                    );

                    let notarized_payment = NotarizedPayment::new(
                        parent_nft.info.launcher_id,
                        vec![Payment::new(
                            parent_nft.info.royalty_puzzle_hash,
                            royalty_amount,
                            hint,
                        )],
                    );

                    let inner_settlement_solution =
                        ctx.alloc(&SettlementPaymentsSolution::new(vec![notarized_payment]))?;

                    if trade_price.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                        let outer_puzzle = ctx.alloc_mod::<SettlementPayment>()?;
                        let outer_puzzle = Puzzle::parse(ctx, outer_puzzle);
                        reveals.reveal_settlement_payment(
                            ctx,
                            outer_puzzle,
                            inner_settlement_solution,
                        )?;
                    } else if let Some(cat_info) = cats.get(&trade_price.puzzle_hash) {
                        let p2_puzzle = ctx.alloc_mod::<SettlementPayment>()?;
                        let outer_puzzle = cat_info.construct_puzzle(ctx, p2_puzzle)?;
                        let outer_puzzle = Puzzle::parse(ctx, outer_puzzle);
                        reveals.reveal_settlement_payment(
                            ctx,
                            outer_puzzle,
                            inner_settlement_solution,
                        )?;
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
        && let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(reveal)) =
            reveals.p2_puzzle(memos.p2_puzzle_hash.into())
        && let Some(fixed_conditions) = &memos.fixed_conditions
    {
        let mut settlement_amount = 0;

        for condition in fixed_conditions {
            if let Some(condition) = condition.as_create_coin()
                && condition.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
            {
                settlement_amount += condition.amount;
            }
        }

        TransferType::OfferPreSplit(OfferPreSplitInfo {
            launcher_id: reveal.launcher_id,
            nonce: reveal.nonce,
            fixed_conditions: fixed_conditions.clone(),
            settlement_amount,
        })
    } else {
        TransferType::Sent
    }
}
