use std::collections::VecDeque;

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::CreateCoin};

use crate::{
    BURN_PUZZLE_HASH, BagAsset, BagFormat, BagPayment, Cat, DriverError, Nft, ParsedAsset,
    ParsedMemos, RevealedCoinSpend, RevealedP2Puzzle, Reveals, SpendContext, parse_memos,
};

/// A coin created by a verified spend.
#[derive(Debug, Clone)]
pub struct ParsedChild {
    /// The coin and the asset it holds. CAT and NFT children inherit the parent's asset, and every other child
    /// (including even amount children of an NFT) is XCH. A bulletin coin that is created and spent in the same
    /// transaction by a verified spend is reported as [`ParsedAsset::Bulletin`].
    pub asset: ParsedAsset,
    /// The destination and memos of the coin.
    pub memos: ParsedMemos,
    /// What kind of transfer the coin represents.
    pub transfer_type: TransferType,
}

/// What kind of transfer a [`ParsedChild`] represents, based on its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferType {
    /// Sent to [`ParsedMemos::p2_puzzle_hash`]. This could be the vault itself (for example, change), so compare
    /// it to [`VaultTransaction::p2_puzzle_hashes`](crate::VaultTransaction::p2_puzzle_hashes).
    Sent,
    /// Sent to [`BURN_PUZZLE_HASH`], so it can never be spent.
    Burned,
    /// Sent to the settlement payments puzzle, which anyone can spend. This is how value is offered, and it's only
    /// safe if the payments in return are asserted.
    Offered,
    /// Sent to a revealed [`P2ConditionsOrSingleton`](crate::P2ConditionsOrSingleton) with fixed conditions that
    /// were revealed in the memos, such as a coin set aside for an offer.
    OfferPreSplit(OfferPreSplitInfo),
    /// The coin is the root of a revealed bag, so its value is paid out to the bag's payments once it's unrolled.
    ///
    /// Since the payments are delayed until someone unrolls the bag, the root is treated as a single output that
    /// leaves the vault, even if some of the payments go back to it.
    SecureTheBag(SecureTheBagInfo),
}

/// A coin set aside for a future offer, which can be spent by anyone with its fixed conditions, or by the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferPreSplitInfo {
    /// The launcher id of the vault that can spend the coin.
    pub launcher_id: Bytes32,
    /// The nonce of the p2 puzzle.
    pub nonce: usize,
    /// The conditions that anyone can spend the coin with, such as the offer's payment assertions and fee.
    pub fixed_conditions: Vec<Condition>,
    /// The amount of the coin minus the fee reserved by the fixed conditions (floored at zero), which is what's
    /// left for the offer.
    pub settlement_amount: u64,
}

/// The payments of a revealed Secure the Bag, whose root coin is created by a verified spend.
///
/// The root coin's puzzle hash commits to every payment and its amount is the bag's total, so once the root exists,
/// anyone can unroll it into exactly these payments. Unrolling it takes further spends, which aren't part of this
/// transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecureTheBagInfo {
    /// The format of the bag, which determines how its tree of nodes is built.
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
    /// The recipient, amount, and clawback of the payment. A payment with a clawback can still be clawed back by its
    /// sender until the clawback expires.
    pub payment: BagPayment,
    /// This is either [`TransferType::Burned`] or [`TransferType::Sent`]. Bags aren't nested, their payments don't
    /// carry fixed conditions, and an offer can't settle a payment before the bag is unrolled, so a payment to the
    /// settlement payments puzzle can be spent by anyone and is [`TransferType::Sent`].
    pub transfer_type: TransferType,
}

/// Parses the coins created by the `CREATE_COIN` conditions of a verified spend, whose asset is `asset`.
///
/// CAT children are parsed from the spend, and each must have the same revocation layer as the parent, or
/// [`DriverError::RevocableChild`] is returned. A missing CAT or NFT child returns [`DriverError::MissingChild`]. Each
/// child's [`TransferType`] is calculated from its memos and the [`Reveals`] (such as revealed bags and
/// [`P2ConditionsOrSingleton`](crate::P2ConditionsOrSingleton) puzzles). The spend's other conditions aren't recorded
/// here.
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
