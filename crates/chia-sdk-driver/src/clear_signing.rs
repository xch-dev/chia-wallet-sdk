use std::collections::HashSet;

use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{Memos, offer::SettlementPaymentsSolution};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    Condition, MessageFlags, MessageSide, Mod, announcement_id, conditions::CreateCoin,
    puzzles::SingletonMember, run_puzzle, tree_hash_notarized_payment,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{Cat, ClawbackV2, DriverError, MetadataUpdate, Puzzle, Spend, mips_puzzle_hash};

/// Information about a vault that must be provided in order to securely parse a transaction.
#[derive(Debug, Clone, Copy)]
pub struct VaultSpendReveal {
    /// The launcher id of the vault's singleton.
    /// This is used to calculate the p2 puzzle hash.
    pub launcher_id: Bytes32,
    /// The inner puzzle hash of the vault singleton.
    /// This is used to construct the puzzle hash we're signing for.
    pub custody_hash: TreeHash,
    /// The delegated puzzle we're signing and its solution.
    /// Its output is the non-custody related conditions that the vault spend will output.
    pub delegated_spend: Spend,
}

/// The purpose of this is to provide sufficient information to verify what is happening to a vault and its assets
/// as a result of a transaction at a glance. Information that is not verifiable should not be included or displayed.
/// We can still allow transactions which are not fully verifiable, but a conservative summary should be provided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultTransaction {
    /// If a new vault coin is created (i.e. the vault isn't melted), this will be set.
    /// It's the new inner puzzle hash of the vault singleton. If it's different, the custody configuration has changed.
    /// It can be validated against a [`MipsMemo`](crate::MipsMemo) so that you know what specifically is happening.
    pub new_custody_hash: Option<TreeHash>,
    /// Fungible asset payments that are relevant to the vault and can be verified to exist if the signature is used.
    pub payments: Vec<ParsedPayment>,
    /// NFT transfers that are relevant to the vault and can be verified to exist if the signature is used.
    pub nfts: Vec<ParsedNftTransfer>,
    /// Total fees (different between input and output amounts) paid by coin spends authorized by the vault.
    /// If the transaction is signed, the fee is guaranteed to be at least this amount, unless it's not reserved.
    /// The reason to include unreserved fees is to make it clear that the XCH is leaving the vault due to this transaction.
    pub fee_paid: u64,
    /// Total fees (different between input and output amounts) paid by all coin spends in the transaction combined.
    /// Because the full coin spend list cannot be validated off-chain, this is not guaranteed to be accurate.
    pub total_fee: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPayment {
    /// The direction in which the asset is being transferred.
    pub transfer_type: TransferType,
    /// The asset id, if applicable. This may be [`None`] for XCH, or [`Some`] for a CAT or singleton asset.
    pub asset_id: Option<Bytes32>,
    /// The revocation hidden puzzle hash (if the asset is a revocable CAT).
    pub hidden_puzzle_hash: Option<Bytes32>,
    /// The custody p2 puzzle hash that the payment is being sent to (analogous to a decoded XCH or TXCH address).
    pub p2_puzzle_hash: Bytes32,
    /// The coin that will be created as a result of this payment being confirmed on-chain.
    /// This includes the amount being paid to the p2 puzzle hash.
    pub coin: Coin,
    /// If applicable, the clawback information for the payment (including who can claw it back and for how long).
    pub clawback: Option<ClawbackV2>,
    /// The potentially human readable memo list after the hint and/or clawback memo is removed.
    pub memos: Vec<Bytes>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNftTransfer {
    /// The direction in which the NFT is being transferred.
    pub transfer_type: TransferType,
    /// The launcher id of the NFT.
    pub launcher_id: Bytes32,
    /// The custody p2 puzzle hash that the NFT is being sent to (analogous to a decoded XCH or TXCH address).
    pub p2_puzzle_hash: Bytes32,
    /// The latest NFT coin that is confirmed to be created as a result of this transaction.
    /// Unverifiable coin spends will be excluded.
    pub coin: Coin,
    /// If applicable, the clawback information for the NFT (including who can claw it back and for how long).
    pub clawback: Option<ClawbackV2>,
    /// The potentially human readable memo list after the hint and/or clawback memo is removed.
    pub memos: Vec<Bytes>,
    /// URIs which are added to the NFT's metadata as part of coin spends which can be verified to exist.
    pub new_uris: Vec<MetadataUpdate>,
    /// The latest owner hash of the NFT from verified coin spends.
    pub latest_owner: Option<Bytes32>,
    /// Whether the NFT transfer includes unverifiable metadata updates.
    pub includes_unverifiable_updates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    /// These are payments that are output from coin spends which have been authorized by the vault.
    /// Notably, this will not include payments that are being sent to the vault's p2 puzzle hash.
    /// Thus, this excludes change coins and payments that are received from taken offers.
    Sent,
    /// These are payments to the vault's p2 puzzle hash that are output from offer settlement coins.
    /// Change coins and non-offer payments are excluded, since their authenticity cannot be easily verified off-chain.
    /// An offer payment is also excluded if its notarized payment announcement id is not asserted by a coin spend authorized by the vault.
    Received,
    /// When the coin spend originally comes from the vault, and ends up back in the vault, this is an update.
    /// It's only used for singleton transactions.
    Updated,
}

impl VaultTransaction {
    pub fn parse(
        allocator: &mut Allocator,
        vault: &VaultSpendReveal,
        coin_spends: Vec<CoinSpend>,
    ) -> Result<Self, DriverError> {
        let our_p2_puzzle_hash = vault_p2_puzzle_hash(vault.launcher_id);

        let ParsedDelegatedSpend {
            new_custody_hash,
            our_spent_coin_ids,
            puzzle_assertion_ids,
        } = parse_delegated_spend(
            allocator,
            vault.delegated_spend,
            &coin_spends.iter().map(|cs| cs.coin.coin_id()).collect(),
        )?;

        let ParsedConditions {
            puzzle_assertion_ids,
        } = parse_our_conditions(
            allocator,
            coin_spends
                .iter()
                .filter(|cs| our_spent_coin_ids.contains(&cs.coin.coin_id())),
            puzzle_assertion_ids,
        )?;

        let mut payments = Vec::new();
        let mut nfts = Vec::new();
        let mut our_input = 0;
        let mut our_output = 0;
        let mut total_input = 0;
        let mut total_output = 0;

        for coin_spend in coin_spends {
            let is_parent_ours = our_spent_coin_ids.contains(&coin_spend.coin.coin_id());

            total_input += coin_spend.coin.amount;

            if is_parent_ours {
                our_input += coin_spend.coin.amount;
            }

            let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
            let puzzle = Puzzle::parse(allocator, puzzle);
            let solution = coin_spend.solution.to_clvm(allocator)?;

            let output = run_puzzle(allocator, puzzle.ptr(), solution)?;
            let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

            if let Some((cat, p2_puzzle, p2_solution)) =
                Cat::parse(allocator, coin_spend.coin, puzzle, solution)?
            {
                let p2_output = run_puzzle(allocator, p2_puzzle.ptr(), p2_solution)?;

                let mut p2_create_coins = Vec::<Condition>::from_clvm(allocator, p2_output)?
                    .into_iter()
                    .filter_map(Condition::into_create_coin)
                    .collect::<Vec<_>>();

                let children = Cat::parse_children(allocator, coin_spend.coin, puzzle, solution)?
                    .unwrap_or_default();

                let notarized_payments =
                    if cat.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                        Some(
                            SettlementPaymentsSolution::from_clvm(allocator, p2_solution)?
                                .notarized_payments,
                        )
                    } else {
                        None
                    };

                for child in children {
                    let create_coin = p2_create_coins.remove(0);
                    let parsed_memos = parse_memos(allocator, create_coin, true);
                    let is_child_ours = parsed_memos.p2_puzzle_hash == our_p2_puzzle_hash;

                    total_output += child.coin.amount;

                    if is_parent_ours {
                        our_output += child.coin.amount;
                    }

                    // Skip ephemeral coins
                    if our_spent_coin_ids.contains(&child.coin.coin_id()) {
                        continue;
                    }

                    let parsed_payment = ParsedPayment {
                        transfer_type: TransferType::Sent,
                        asset_id: Some(child.info.asset_id),
                        hidden_puzzle_hash: child.info.hidden_puzzle_hash,
                        p2_puzzle_hash: parsed_memos.p2_puzzle_hash,
                        coin: child.coin,
                        clawback: parsed_memos.clawback,
                        memos: parsed_memos.memos,
                    };

                    // Don't add change coins to the payment list, or received payments
                    // that aren't from verifiable offer payments
                    if is_parent_ours && !is_child_ours {
                        payments.push(parsed_payment);
                    } else if !is_parent_ours
                        && is_child_ours
                        && let Some(notarized_payments) = &notarized_payments
                        && let Some(notarized_payment) = notarized_payments.iter().find(|np| {
                            np.payments.iter().any(|p| {
                                p.puzzle_hash == create_coin.puzzle_hash
                                    && p.amount == create_coin.amount
                            })
                        })
                    {
                        let notarized_payment_hash =
                            tree_hash_notarized_payment(allocator, notarized_payment);

                        let settlement_announcement_id =
                            announcement_id(cat.coin.puzzle_hash, notarized_payment_hash);

                        if puzzle_assertion_ids.contains(&settlement_announcement_id) {
                            payments.push(ParsedPayment {
                                transfer_type: TransferType::Received,
                                ..parsed_payment
                            });
                        }
                    }
                }
            } else {
                let create_coins = conditions
                    .into_iter()
                    .filter_map(Condition::into_create_coin)
                    .collect::<Vec<_>>();

                let notarized_payments =
                    if coin_spend.coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                        Some(
                            SettlementPaymentsSolution::from_clvm(allocator, solution)?
                                .notarized_payments,
                        )
                    } else {
                        None
                    };

                for create_coin in create_coins {
                    let child_coin = Coin::new(
                        coin_spend.coin.coin_id(),
                        create_coin.puzzle_hash,
                        create_coin.amount,
                    );

                    let parsed_memos = parse_memos(allocator, create_coin, false);
                    let is_child_ours = parsed_memos.p2_puzzle_hash == our_p2_puzzle_hash;

                    total_output += child_coin.amount;

                    if is_parent_ours {
                        our_output += child_coin.amount;
                    }

                    // Skip ephemeral coins
                    if our_spent_coin_ids.contains(&child_coin.coin_id()) {
                        continue;
                    }

                    let parsed_payment = ParsedPayment {
                        transfer_type: TransferType::Sent,
                        asset_id: None,
                        hidden_puzzle_hash: None,
                        p2_puzzle_hash: parsed_memos.p2_puzzle_hash,
                        coin: child_coin,
                        clawback: parsed_memos.clawback,
                        memos: parsed_memos.memos,
                    };

                    // Don't add change coins to the payment list
                    if is_parent_ours && !is_child_ours {
                        payments.push(parsed_payment);
                    } else if !is_parent_ours
                        && is_child_ours
                        && let Some(notarized_payments) = &notarized_payments
                        && let Some(notarized_payment) = notarized_payments.iter().find(|np| {
                            np.payments.iter().any(|p| {
                                p.puzzle_hash == create_coin.puzzle_hash
                                    && p.amount == create_coin.amount
                            })
                        })
                    {
                        let notarized_payment_hash =
                            tree_hash_notarized_payment(allocator, notarized_payment);

                        let settlement_announcement_id =
                            announcement_id(coin_spend.coin.puzzle_hash, notarized_payment_hash);

                        if puzzle_assertion_ids.contains(&settlement_announcement_id) {
                            payments.push(ParsedPayment {
                                transfer_type: TransferType::Received,
                                ..parsed_payment
                            });
                        }
                    }
                }
            }
        }

        Ok(Self {
            new_custody_hash,
            payments,
            nfts,
            fee_paid: our_input.saturating_sub(our_output),
            total_fee: total_input.saturating_sub(total_output),
        })
    }
}

fn vault_p2_puzzle_hash(launcher_id: Bytes32) -> Bytes32 {
    mips_puzzle_hash(
        0,
        vec![],
        SingletonMember::new(launcher_id).curry_tree_hash(),
        true,
    )
    .into()
}

#[derive(Debug, Clone)]
struct ParsedDelegatedSpend {
    new_custody_hash: Option<TreeHash>,
    our_spent_coin_ids: HashSet<Bytes32>,
    puzzle_assertion_ids: HashSet<Bytes32>,
}

fn parse_delegated_spend(
    allocator: &mut Allocator,
    delegated_spend: Spend,
    spent_coin_ids: &HashSet<Bytes32>,
) -> Result<ParsedDelegatedSpend, DriverError> {
    let vault_output = run_puzzle(allocator, delegated_spend.puzzle, delegated_spend.solution)?;
    let vault_conditions = Vec::<Condition>::from_clvm(allocator, vault_output)?;

    let mut new_custody_hash = None;
    let mut our_spent_coin_ids = HashSet::new();
    let mut puzzle_assertion_ids = HashSet::new();

    for condition in vault_conditions {
        match condition {
            Condition::CreateCoin(condition) => {
                if condition.amount % 2 == 1 {
                    // The vault singleton is being recreated
                    new_custody_hash = Some(condition.puzzle_hash.into());
                } else {
                    // TODO: The vault is creating a drop coin
                }
            }
            Condition::SendMessage(condition) => {
                // If the receiver isn't a specific coin id, we prevent signing
                let sender = MessageFlags::decode(condition.mode, MessageSide::Sender);
                let receiver = MessageFlags::decode(condition.mode, MessageSide::Receiver);

                if sender != MessageFlags::PUZZLE
                    || receiver != MessageFlags::COIN
                    || condition.data.len() != 1
                {
                    return Err(DriverError::MissingSpend);
                }

                // If we're authorizing a spend, it must be in the revealed coin spends
                // We can't authorize the same spend twice
                let coin_id = Bytes32::from_clvm(allocator, condition.data[0])?;

                if !spent_coin_ids.contains(&coin_id) || !our_spent_coin_ids.insert(coin_id) {
                    return Err(DriverError::MissingSpend);
                }
            }
            Condition::AssertPuzzleAnnouncement(condition) => {
                puzzle_assertion_ids.insert(condition.announcement_id);
            }
            _ => {}
        }
    }

    Ok(ParsedDelegatedSpend {
        new_custody_hash,
        our_spent_coin_ids,
        puzzle_assertion_ids,
    })
}

#[derive(Debug, Clone)]
struct ParsedConditions {
    puzzle_assertion_ids: HashSet<Bytes32>,
}

fn parse_our_conditions<'a>(
    allocator: &mut Allocator,
    coin_spends: impl Iterator<Item = &'a CoinSpend>,
    mut puzzle_assertion_ids: HashSet<Bytes32>,
) -> Result<ParsedConditions, DriverError> {
    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let solution = coin_spend.solution.to_clvm(allocator)?;
        let output = run_puzzle(allocator, puzzle, solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        for condition in conditions {
            if let Condition::AssertPuzzleAnnouncement(condition) = condition {
                puzzle_assertion_ids.insert(condition.announcement_id);
            }
        }
    }

    Ok(ParsedConditions {
        puzzle_assertion_ids,
    })
}

#[derive(Debug, Clone)]
struct ParsedMemos {
    p2_puzzle_hash: Bytes32,
    clawback: Option<ClawbackV2>,
    memos: Vec<Bytes>,
}

fn parse_memos(
    allocator: &Allocator,
    p2_create_coin: CreateCoin<NodePtr>,
    is_cat: bool,
) -> ParsedMemos {
    // If there is no memo list, there's nothing to parse and we can assume there's no clawback
    let Memos::Some(memos) = p2_create_coin.memos else {
        return ParsedMemos {
            p2_puzzle_hash: p2_create_coin.puzzle_hash,
            clawback: None,
            memos: Vec::new(),
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
            is_cat,
            p2_create_coin.puzzle_hash,
        )
    {
        return ParsedMemos {
            p2_puzzle_hash: clawback.receiver_puzzle_hash,
            clawback: Some(clawback),
            memos: Vec::<Bytes>::from_clvm(allocator, rest).unwrap_or_default(),
        };
    }

    // If we're parsing a CAT output, we can remove the hint from the memos if applicable.
    if is_cat && let Ok((_hint, rest)) = <(Bytes32, NodePtr)>::from_clvm(allocator, memos) {
        return ParsedMemos {
            p2_puzzle_hash: p2_create_coin.puzzle_hash,
            clawback: None,
            memos: Vec::<Bytes>::from_clvm(allocator, rest).unwrap_or_default(),
        };
    }

    // Otherwise, we assume there's no clawback and return the memos as is, if they are bytes.
    ParsedMemos {
        p2_puzzle_hash: p2_create_coin.puzzle_hash,
        clawback: None,
        memos: Vec::<Bytes>::from_clvm(allocator, memos).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use chia_sdk_test::Simulator;

    use crate::{Action, Id, SpendContext, TestVault};

    #[test]
    fn test_clear_signing() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(Id::Xch, bob.puzzle_hash(), 800, Memos::None),
                Action::fee(200),
            ],
        )?;

        let reveal = VaultSpendReveal {
            launcher_id: alice.launcher_id(),
            custody_hash: alice.custody_hash(),
            delegated_spend: result.delegated_spend,
        };

        let tx = VaultTransaction::parse(&mut ctx, &reveal, result.coin_spends)?;
        assert_eq!(tx.new_custody_hash, Some(alice.custody_hash()));
        assert_eq!(tx.payments.len(), 1);
        assert_eq!(tx.fee_paid, 200);
        assert_eq!(tx.total_fee, 200);

        let payment = &tx.payments[0];
        assert_eq!(payment.transfer_type, TransferType::Sent);
        assert_eq!(payment.p2_puzzle_hash, bob.puzzle_hash());
        assert_eq!(payment.coin.amount, 800);

        Ok(())
    }
}
