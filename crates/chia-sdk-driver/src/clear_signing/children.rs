use std::collections::VecDeque;

use chia_consensus::opcodes::{CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, SEND_MESSAGE};
use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::CreateCoin};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};
use num_bigint::BigInt;

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
    ///
    /// Since the payments are delayed until someone unrolls the bag, the root is treated as a single output that
    /// leaves the vault, even if some of the payments go back to it.
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
    /// This is either [`TransferType::Burned`] or [`TransferType::Sent`]. Bags aren't nested, their payments don't
    /// carry fixed conditions, and an offer can't settle a payment before the bag is unrolled.
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
    // The p2 puzzle hash of a clawback is the receiver's, but the coin can also be clawed back by
    // the sender, so it can't be treated as anything other than a regular send.
    if memos.clawback.is_some() {
        return Ok(TransferType::Sent);
    }

    // The root hash commits to every payment and to the total amount, so anyone can unroll a coin with the root hash
    // and that amount to the payments. As with any revocable CAT, the issuer can still revoke the root or its nodes
    // before then. If the amount is less, the root can't be spent. If it's more, the rest is lost to fees, which isn't
    // something we want to show as a bag.
    if let Some(bag_asset) = bag_asset
        && let Some(bag) = reveals.bag(memos.p2_puzzle_hash, bag_asset)
        && bag.total_amount() == input_amount
    {
        // An offer in this transaction can't settle a payment that's only created once the bag is unrolled, so a
        // payment to the settlement puzzle is anyone can spend, rather than offered.
        let payments = bag
            .payments()
            .iter()
            .map(|payment| ParsedBagPayment {
                payment: *payment,
                transfer_type: if payment.puzzle_hash == BURN_PUZZLE_HASH {
                    TransferType::Burned
                } else {
                    TransferType::Sent
                },
            })
            .collect();

        return Ok(TransferType::SecureTheBag(SecureTheBagInfo {
            format: bag.format(),
            payments,
        }));
    }

    Ok(if memos.p2_puzzle_hash == BURN_PUZZLE_HASH {
        TransferType::Burned
    } else if memos.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
        TransferType::Offered
    } else if let Some(RevealedP2Puzzle::P2ConditionsOrSingleton(reveal)) =
        reveals.p2_puzzle(memos.p2_puzzle_hash.into())
        && let Some(fixed_conditions) = &memos.fixed_conditions
    {
        let (settlement_amount, reserved_fee) = calculate_settlement_amount(fixed_conditions)?;

        // Any XCH that isn't paid to the settlement or reserved as a fee would be an implicit fee,
        // which wouldn't be reflected in the offer. The fee may be paid by other coins, so it can exceed
        // the remaining amount.
        if bag_asset == Some(BagAsset::Xch)
            && u128::from(settlement_amount) + reserved_fee < u128::from(input_amount)
        {
            return Err(DriverError::InvalidOfferPreSplitConditions);
        }

        TransferType::OfferPreSplit(OfferPreSplitInfo {
            launcher_id: reveal.launcher_id,
            nonce: reveal.nonce,
            fixed_conditions: fixed_conditions.clone(),
            settlement_amount,
        })
    } else {
        TransferType::Sent
    })
}

/// The fixed conditions of an offer pre-split coin can be output by anyone, so we only allow
/// creating settlement coins, reserving fees, and assertions which restrict when they can be output.
///
/// Returns the settlement amount and the reserved fee.
fn calculate_settlement_amount(fixed_conditions: &[Condition]) -> Result<(u64, u128), DriverError> {
    let mut settlement_amount = 0_u128;
    let mut reserved_fee = 0_u128;

    for condition in fixed_conditions {
        match condition {
            Condition::CreateCoin(condition)
                if condition.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() =>
            {
                settlement_amount += u128::from(condition.amount);
            }
            Condition::ReserveFee(condition) => {
                reserved_fee += u128::from(condition.amount);
            }
            Condition::AssertPuzzleAnnouncement(_)
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

    Ok((settlement_amount.try_into()?, reserved_fee))
}

/// Returns the announcements and messages created by a spend, which aren't otherwise reflected in
/// the summary. These can authorize other spends which assert them, so they must be surfaced.
pub fn parse_extra_conditions(
    allocator: &Allocator,
    conditions: &[Condition],
) -> Result<Vec<Condition>, DriverError> {
    let mut extra_conditions = Vec::new();

    for condition in conditions {
        match condition {
            Condition::CreateCoinAnnouncement(_)
            | Condition::CreatePuzzleAnnouncement(_)
            | Condition::SendMessage(_) => {
                extra_conditions.push(condition.clone());
            }
            Condition::Other(raw_condition) => {
                let (opcode, _) = <(BigInt, NodePtr)>::from_clvm(allocator, *raw_condition)?;

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
            _ => {}
        }
    }

    Ok(extra_conditions)
}
