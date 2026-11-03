use std::collections::VecDeque;

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::CreateCoin};

use crate::{
    BURN_PUZZLE_HASH, BagAsset, BagFormat, BagPayment, Cat, DriverError, Nft, ParsedAsset,
    ParsedMemos, RevealedCoinSpend, RevealedP2Puzzle, Reveals, SpendContext, parse_memos,
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
    /// The coin is the root of a revealed bag, so its value is paid out to the bag's payments once it's unrolled.
    SecureTheBag(SecureTheBagInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferPreSplitInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub fixed_conditions: Vec<Condition>,
    pub settlement_amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecureTheBagInfo {
    pub format: BagFormat,
    /// The payments in the order of the bag, which add up to the amount of the root coin.
    pub payments: Vec<ParsedBagPayment>,
}

/// A payment that a bag pays out once it's unrolled.
///
/// The payment's puzzle hash is the recipient (the receiver for clawback payments), the same as
/// [`ParsedMemos::p2_puzzle_hash`] for a child coin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBagPayment {
    pub payment: BagPayment,
    /// This is never [`TransferType::SecureTheBag`] or [`TransferType::OfferPreSplit`], since bags aren't nested and
    /// their payments don't carry fixed conditions.
    pub transfer_type: TransferType,
}

pub fn parse_children(
    reveals: &mut Reveals,
    ctx: &mut SpendContext,
    asset: &ParsedAsset,
    spend: RevealedCoinSpend,
    conditions: &[Condition],
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
        if let Condition::CreateCoin(condition) = condition {
            match &asset {
                // All XCH and bulletin children are considered to be XCH.
                // However, ephemeral bulletin children are hydrated later based on the spend.
                ParsedAsset::Xch(_) | ParsedAsset::Bulletin(_) => {
                    let memos = parse_memos(reveals, ctx, *condition, false);
                    let transfer_type = calculate_transfer_type(
                        reveals,
                        &memos,
                        Some(BagAsset::Xch),
                        condition.amount,
                    )?;

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
                            calculate_transfer_type(reveals, &memos, None, condition.amount)?;

                        children.push(ParsedChild {
                            asset: ParsedAsset::Nft(nft),
                            memos,
                            transfer_type,
                        });
                    } else {
                        let memos = parse_memos(reveals, ctx, *condition, false);
                        let transfer_type = calculate_transfer_type(
                            reveals,
                            &memos,
                            Some(BagAsset::Xch),
                            condition.amount,
                        )?;

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
                        CreateCoin::new(cat.info.p2_puzzle_hash, condition.amount, condition.memos),
                        true,
                    );

                    // The bag must be for the same CAT, including the revocation layer, so this is based on the
                    // child's info rather than what the bag was revealed with.
                    let bag_asset = match cat.info.hidden_puzzle_hash {
                        Some(hidden_puzzle_hash) => BagAsset::RevocableCat {
                            asset_id: cat.info.asset_id,
                            hidden_puzzle_hash,
                        },
                        None => BagAsset::Cat {
                            asset_id: cat.info.asset_id,
                        },
                    };

                    let transfer_type = calculate_transfer_type(
                        reveals,
                        &memos,
                        Some(bag_asset),
                        condition.amount,
                    )?;

                    children.push(ParsedChild {
                        asset: ParsedAsset::Cat(cat),
                        memos,
                        transfer_type,
                    });
                }
            }
        }
    }

    if !cats.is_empty() {
        return Err(DriverError::MissingChild);
    }

    Ok(children)
}

/// The bag asset is the asset of the child coin, or [`None`] if it can't be a bag (for example an NFT).
fn calculate_transfer_type(
    reveals: &Reveals,
    memos: &ParsedMemos,
    bag_asset: Option<BagAsset>,
    input_amount: u64,
) -> Result<TransferType, DriverError> {
    // The root hash commits to every payment and to the total amount, so a coin with the root hash and that amount
    // is guaranteed to pay out to the payments once it's unrolled. If the amount is less, the root can't be spent.
    // If it's more, the rest is lost to fees, which isn't something we want to show as a bag.
    if memos.clawback.is_none()
        && let Some(bag_asset) = bag_asset
        && let Some(bag) = reveals.bag(memos.p2_puzzle_hash, bag_asset)
        && bag.total_amount() == input_amount
    {
        let payments = bag
            .payments()
            .iter()
            .map(|payment| {
                let memos = ParsedMemos {
                    p2_puzzle_hash: payment.puzzle_hash,
                    clawback: payment.clawback,
                    human_readable_memos: Vec::new(),
                    fixed_conditions: None,
                };

                Ok(ParsedBagPayment {
                    payment: *payment,
                    transfer_type: calculate_transfer_type(reveals, &memos, None, payment.amount)?,
                })
            })
            .collect::<Result<_, DriverError>>()?;

        return Ok(TransferType::SecureTheBag(SecureTheBagInfo {
            format: bag.format(),
            payments,
        }));
    }

    Ok(if memos.p2_puzzle_hash == BURN_PUZZLE_HASH {
        TransferType::Burned
    } else if memos.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
        TransferType::Offered
    } else if memos.clawback.is_none()
        && let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(reveal)) =
            reveals.p2_puzzle(memos.p2_puzzle_hash.into())
        && let Some(fixed_conditions) = &memos.fixed_conditions
    {
        let reserved_fee: u128 = fixed_conditions
            .iter()
            .filter_map(|condition| match condition {
                Condition::ReserveFee(condition) => Some(u128::from(condition.amount)),
                _ => None,
            })
            .sum();

        let reserved_fee: u64 = reserved_fee.try_into()?;

        TransferType::OfferPreSplit(OfferPreSplitInfo {
            launcher_id: reveal.launcher_id,
            nonce: reveal.nonce,
            fixed_conditions: fixed_conditions.clone(),
            // The fee can be paid by other coins in the bundle, so it may exceed the input amount.
            settlement_amount: input_amount.saturating_sub(reserved_fee),
        })
    } else {
        TransferType::Sent
    })
}
