use std::collections::HashSet;

use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{
    Memos,
    nft::NftMetadata,
    offer::{NotarizedPayment, SettlementPaymentsSolution},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    Condition, MessageFlags, MessageSide, Mod, announcement_id, conditions::CreateCoin,
    puzzles::SingletonMember, run_puzzle, tree_hash_notarized_payment,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{
    Cat, ClawbackV2, DriverError, MetadataUpdate, Nft, Puzzle, Spend, UriKind, mips_puzzle_hash,
};

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
    /// Coins which were created as outputs of the vault singleton spend itself, for example to mint NFTs.
    pub drop_coins: Vec<DropCoin>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropCoin {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

impl VaultTransaction {
    pub fn parse(
        allocator: &mut Allocator,
        vault: &VaultSpendReveal,
        coin_spends: Vec<CoinSpend>,
    ) -> Result<Self, DriverError> {
        let our_p2_puzzle_hash = vault_p2_puzzle_hash(vault.launcher_id);

        let all_spent_coin_ids = coin_spends.iter().map(|cs| cs.coin.coin_id()).collect();

        let ParsedDelegatedSpend {
            new_custody_hash,
            our_spent_coin_ids,
            puzzle_assertion_ids,
            drop_coins,
        } = parse_delegated_spend(allocator, vault.delegated_spend, &all_spent_coin_ids)?;

        let ParsedConditions {
            puzzle_assertion_ids,
            all_created_coin_ids,
        } = parse_our_conditions(
            allocator,
            &coin_spends,
            &our_spent_coin_ids,
            puzzle_assertion_ids,
        )?;

        let coin_spends = reorder_coin_spends(coin_spends);

        let mut payments = Vec::new();
        let mut nfts = Vec::new();
        let mut our_input = 0;
        let mut our_output = 0;
        let mut total_input = 0;
        let mut total_output = 0;

        for coin_spend in coin_spends {
            let coin_id = coin_spend.coin.coin_id();
            let is_parent_ours = our_spent_coin_ids.contains(&coin_id);
            let is_parent_ephemeral = all_created_coin_ids.contains(&coin_id);

            total_input += coin_spend.coin.amount;

            if is_parent_ours && !is_parent_ephemeral {
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
                        SettlementPaymentsSolution::from_clvm(allocator, p2_solution)?
                            .notarized_payments
                    } else {
                        Vec::new()
                    };

                for child in children {
                    let child_coin_id = child.coin.coin_id();
                    let create_coin = p2_create_coins.remove(0);
                    let parsed_memos = parse_memos(allocator, create_coin, true);
                    let is_child_ours = parsed_memos.p2_puzzle_hash == our_p2_puzzle_hash;
                    let is_child_ephemeral = all_spent_coin_ids.contains(&child_coin_id);

                    total_output += child.coin.amount;

                    if is_parent_ours && !is_child_ephemeral {
                        our_output += child.coin.amount;
                    }

                    // Skip ephemeral coins
                    if our_spent_coin_ids.contains(&child_coin_id) {
                        continue;
                    }

                    if let Some(transfer_type) = calculate_transfer_type(
                        allocator,
                        TransferTypeContext {
                            puzzle_assertion_ids: &puzzle_assertion_ids,
                            notarized_payments: &notarized_payments,
                            create_coin: &create_coin,
                            full_puzzle_hash: cat.coin.puzzle_hash,
                            is_parent_ours,
                            is_child_ours,
                            is_fungible: true,
                        },
                    ) {
                        payments.push(ParsedPayment {
                            transfer_type,
                            asset_id: Some(child.info.asset_id),
                            hidden_puzzle_hash: child.info.hidden_puzzle_hash,
                            p2_puzzle_hash: parsed_memos.p2_puzzle_hash,
                            coin: child.coin,
                            clawback: parsed_memos.clawback,
                            memos: parsed_memos.memos,
                        });
                    }
                }

                continue;
            }

            let mut exclude_odd_coins = false;

            if let Some((nft, p2_puzzle, p2_solution)) =
                Nft::parse(allocator, coin_spend.coin, puzzle, solution)?
            {
                exclude_odd_coins = true;

                let p2_output = run_puzzle(allocator, p2_puzzle.ptr(), p2_solution)?;

                let mut p2_create_coins = Vec::<Condition>::from_clvm(allocator, p2_output)?
                    .into_iter()
                    .filter_map(Condition::into_create_coin)
                    .filter(|cc| cc.amount % 2 == 1)
                    .collect::<Vec<_>>();

                let child = Nft::parse_child(allocator, coin_spend.coin, puzzle, solution)?
                    .ok_or(DriverError::MissingChild)?;

                let notarized_payments =
                    if nft.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                        SettlementPaymentsSolution::from_clvm(allocator, p2_solution)?
                            .notarized_payments
                    } else {
                        Vec::new()
                    };

                let child_coin_id = child.coin.coin_id();
                let is_child_ephemeral = all_spent_coin_ids.contains(&child_coin_id);
                let create_coin = p2_create_coins.remove(0);
                let parsed_memos = parse_memos(allocator, create_coin, true);
                let is_child_ours = parsed_memos.p2_puzzle_hash == our_p2_puzzle_hash;

                total_output += child.coin.amount;

                if is_parent_ours && !is_child_ephemeral {
                    our_output += child.coin.amount;
                }

                // Skip ephemeral coins
                if our_spent_coin_ids.contains(&child.coin.coin_id()) {
                    continue;
                }

                if let Some(transfer_type) = calculate_transfer_type(
                    allocator,
                    TransferTypeContext {
                        puzzle_assertion_ids: &puzzle_assertion_ids,
                        notarized_payments: &notarized_payments,
                        create_coin: &create_coin,
                        full_puzzle_hash: nft.coin.puzzle_hash,
                        is_parent_ours,
                        is_child_ours,
                        is_fungible: false,
                    },
                ) {
                    let mut includes_unverifiable_updates = false;

                    let new_uris = if let Ok(old_metadata) =
                        NftMetadata::from_clvm(allocator, nft.info.metadata.ptr())
                        && let Ok(new_metadata) =
                            NftMetadata::from_clvm(allocator, child.info.metadata.ptr())
                    {
                        let mut new_uris = Vec::new();

                        for uri in new_metadata.data_uris {
                            if !old_metadata.data_uris.contains(&uri) {
                                new_uris.push(MetadataUpdate {
                                    kind: UriKind::Data,
                                    uri,
                                });
                            }
                        }

                        for uri in new_metadata.metadata_uris {
                            if !old_metadata.metadata_uris.contains(&uri) {
                                new_uris.push(MetadataUpdate {
                                    kind: UriKind::Metadata,
                                    uri,
                                });
                            }
                        }

                        for uri in new_metadata.license_uris {
                            if !old_metadata.license_uris.contains(&uri) {
                                new_uris.push(MetadataUpdate {
                                    kind: UriKind::License,
                                    uri,
                                });
                            }
                        }

                        new_uris
                    } else {
                        includes_unverifiable_updates |= nft.info.metadata != child.info.metadata;

                        vec![]
                    };

                    nfts.push(ParsedNftTransfer {
                        transfer_type,
                        launcher_id: child.info.launcher_id,
                        p2_puzzle_hash: parsed_memos.p2_puzzle_hash,
                        coin: child.coin,
                        clawback: parsed_memos.clawback,
                        memos: parsed_memos.memos,
                        new_uris,
                        latest_owner: child.info.current_owner,
                        includes_unverifiable_updates,
                    });
                }
            }

            let create_coins = conditions
                .into_iter()
                .filter_map(Condition::into_create_coin)
                .collect::<Vec<_>>();

            let notarized_payments =
                if coin_spend.coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                    SettlementPaymentsSolution::from_clvm(allocator, solution)?.notarized_payments
                } else {
                    Vec::new()
                };

            for create_coin in create_coins {
                let child_coin = Coin::new(
                    coin_spend.coin.coin_id(),
                    create_coin.puzzle_hash,
                    create_coin.amount,
                );

                let child_coin_id = child_coin.coin_id();
                let is_child_ephemeral = all_spent_coin_ids.contains(&child_coin_id);

                // We've already emitted payments for singleton outputs, so we can skip odd coins
                if exclude_odd_coins && child_coin.amount % 2 == 1 {
                    continue;
                }

                let parsed_memos = parse_memos(allocator, create_coin, false);
                let is_child_ours = parsed_memos.p2_puzzle_hash == our_p2_puzzle_hash;

                total_output += child_coin.amount;

                if is_parent_ours && !is_child_ephemeral {
                    our_output += child_coin.amount;
                }

                // Skip ephemeral coins
                if our_spent_coin_ids.contains(&child_coin_id) {
                    continue;
                }

                if let Some(transfer_type) = calculate_transfer_type(
                    allocator,
                    TransferTypeContext {
                        puzzle_assertion_ids: &puzzle_assertion_ids,
                        notarized_payments: &notarized_payments,
                        create_coin: &create_coin,
                        full_puzzle_hash: coin_spend.coin.puzzle_hash,
                        is_parent_ours,
                        is_child_ours,
                        is_fungible: true,
                    },
                ) {
                    payments.push(ParsedPayment {
                        transfer_type,
                        asset_id: None,
                        hidden_puzzle_hash: None,
                        p2_puzzle_hash: parsed_memos.p2_puzzle_hash,
                        coin: child_coin,
                        clawback: parsed_memos.clawback,
                        memos: parsed_memos.memos,
                    });
                }
            }
        }

        Ok(Self {
            new_custody_hash,
            payments,
            nfts,
            drop_coins,
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
    drop_coins: Vec<DropCoin>,
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
    let mut drop_coins = Vec::new();

    for condition in vault_conditions {
        match condition {
            Condition::CreateCoin(condition) => {
                if condition.amount % 2 == 1 {
                    // The vault singleton is being recreated
                    new_custody_hash = Some(condition.puzzle_hash.into());
                } else {
                    drop_coins.push(DropCoin {
                        puzzle_hash: condition.puzzle_hash,
                        amount: condition.amount,
                    });
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
        drop_coins,
    })
}

#[derive(Debug, Clone)]
struct ParsedConditions {
    puzzle_assertion_ids: HashSet<Bytes32>,
    all_created_coin_ids: HashSet<Bytes32>,
}

fn parse_our_conditions(
    allocator: &mut Allocator,
    coin_spends: &[CoinSpend],
    our_coin_ids: &HashSet<Bytes32>,
    mut puzzle_assertion_ids: HashSet<Bytes32>,
) -> Result<ParsedConditions, DriverError> {
    let mut all_created_coin_ids = HashSet::new();

    for coin_spend in coin_spends {
        let coin_id = coin_spend.coin.coin_id();
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let solution = coin_spend.solution.to_clvm(allocator)?;
        let output = run_puzzle(allocator, puzzle, solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        for condition in conditions {
            match condition {
                Condition::AssertPuzzleAnnouncement(condition) => {
                    if our_coin_ids.contains(&coin_id) {
                        puzzle_assertion_ids.insert(condition.announcement_id);
                    }
                }
                Condition::CreateCoin(condition) => {
                    all_created_coin_ids.insert(
                        Coin::new(coin_id, condition.puzzle_hash, condition.amount).coin_id(),
                    );
                }
                _ => {}
            }
        }
    }

    Ok(ParsedConditions {
        puzzle_assertion_ids,
        all_created_coin_ids,
    })
}

/// The idea here is to order coin spends by the order in which they were created.
/// This simplifies keeping track of the lineage and latest updates of singletons such as NFTs.
/// Coins which weren't created in this transaction are first, followed by coins which they created, and so on.
fn reorder_coin_spends(mut coin_spends: Vec<CoinSpend>) -> Vec<CoinSpend> {
    let mut reordered_coin_spends = Vec::new();
    let mut remaining_spent_coin_ids: HashSet<Bytes32> =
        coin_spends.iter().map(|cs| cs.coin.coin_id()).collect();

    while !coin_spends.is_empty() {
        coin_spends.retain(|cs| {
            if remaining_spent_coin_ids.contains(&cs.coin.parent_coin_info) {
                true
            } else {
                remaining_spent_coin_ids.remove(&cs.coin.coin_id());
                reordered_coin_spends.push(cs.clone());
                false
            }
        });
    }

    reordered_coin_spends
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
    hintable: bool,
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
            hintable,
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
    if hintable && let Ok((_hint, rest)) = <(Bytes32, NodePtr)>::from_clvm(allocator, memos) {
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

#[derive(Debug, Clone, Copy)]
struct TransferTypeContext<'a> {
    puzzle_assertion_ids: &'a HashSet<Bytes32>,
    notarized_payments: &'a Vec<NotarizedPayment>,
    create_coin: &'a CreateCoin<NodePtr>,
    full_puzzle_hash: Bytes32,
    is_parent_ours: bool,
    is_child_ours: bool,
    is_fungible: bool,
}

fn calculate_transfer_type(
    allocator: &Allocator,
    context: TransferTypeContext<'_>,
) -> Option<TransferType> {
    let TransferTypeContext {
        puzzle_assertion_ids,
        notarized_payments,
        create_coin,
        full_puzzle_hash,
        is_parent_ours,
        is_child_ours,
        is_fungible,
    } = context;

    if is_parent_ours && !is_child_ours {
        // We know that the coin spend is authorized by the delegated spend, and we don't own the child coin
        // Therefore, it's a valid sent payment
        Some(TransferType::Sent)
    } else if !is_parent_ours
        && is_child_ours
        && let Some(notarized_payment) = notarized_payments.iter().find(|np| {
            np.payments
                .iter()
                .any(|p| p.puzzle_hash == create_coin.puzzle_hash && p.amount == create_coin.amount)
        })
    {
        let notarized_payment_hash = tree_hash_notarized_payment(allocator, notarized_payment);
        let settlement_announcement_id = announcement_id(full_puzzle_hash, notarized_payment_hash);

        // Since the parent spend isn't verifiable, we need to know that we've asserted the payment
        // Otherwise, it may as well not exist since we could be being lied to by the coin spend provider
        if puzzle_assertion_ids.contains(&settlement_announcement_id) {
            Some(TransferType::Received)
        } else {
            None
        }
    } else if is_parent_ours && is_child_ours && !is_fungible {
        // For non-fungible assets that we sent to ourself, we can assume they are updated
        Some(TransferType::Updated)
    } else {
        // For fungible assets, change coins aren't relevant to the transaction summary
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use chia_puzzles::SINGLETON_LAUNCHER_HASH;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use rstest::rstest;

    use crate::{Action, Id, SpendContext, Spends, TestVault};

    #[rstest]
    fn test_clear_signing_sent(
        #[values(false, true)] is_cat: bool,
        #[values(0, 100)] fee: u64,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000 + fee)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

        let (id, asset_id) = if is_cat {
            let result =
                alice.spend(&mut sim, &mut ctx, &[Action::single_issue_cat(None, 1000)])?;

            let asset_id = result.outputs.cats[0][0].info.asset_id;
            let id = Id::Existing(asset_id);
            (id, Some(asset_id))
        } else {
            (Id::Xch, None)
        };

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(id, bob.puzzle_hash(), 1000, Memos::None),
                Action::fee(fee),
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
        assert_eq!(tx.fee_paid, fee);
        assert_eq!(tx.total_fee, fee);

        let payment = &tx.payments[0];
        assert_eq!(payment.transfer_type, TransferType::Sent);
        assert_eq!(payment.asset_id, asset_id);
        assert_eq!(payment.p2_puzzle_hash, bob.puzzle_hash());
        assert_eq!(payment.coin.amount, 1000);

        Ok(())
    }

    #[rstest]
    fn test_clear_signing_received(
        #[values(false, true)] is_cat: bool,
        #[values(true, false)] disable_settlement_assertions: bool,
        #[values(0, 100)] alice_fee: u64,
        #[values(0, 100)] bob_fee: u64,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000 + alice_fee)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, bob_fee)?;

        let (id, asset_id) = if is_cat {
            let result =
                alice.spend(&mut sim, &mut ctx, &[Action::single_issue_cat(None, 1000)])?;

            let asset_id = result.outputs.cats[0][0].info.asset_id;
            let id = Id::Existing(asset_id);
            (id, Some(asset_id))
        } else {
            (Id::Xch, None)
        };

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(id, SETTLEMENT_PAYMENT_HASH.into(), 1000, Memos::None),
                Action::fee(alice_fee),
            ],
        )?;

        let reveal = VaultSpendReveal {
            launcher_id: bob.launcher_id(),
            custody_hash: bob.custody_hash(),
            delegated_spend: result.delegated_spend,
        };

        let tx = VaultTransaction::parse(&mut ctx, &reveal, result.coin_spends)?;

        assert_eq!(tx.payments.len(), 1);
        assert_eq!(tx.fee_paid, alice_fee);
        assert_eq!(tx.total_fee, alice_fee);

        let payment = &tx.payments[0];
        assert_eq!(payment.transfer_type, TransferType::Sent);
        assert_eq!(payment.asset_id, asset_id);
        assert_eq!(payment.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());
        assert_eq!(payment.coin.amount, 1000);

        let mut spends = Spends::new(bob.puzzle_hash());
        if id == Id::Xch {
            spends.add(result.outputs.xch[0]);
        } else {
            spends.add(result.outputs.cats[&id][0]);
        }
        spends.conditions.disable_settlement_assertions = disable_settlement_assertions;

        let result = bob.custom_spend(
            &mut sim,
            &mut ctx,
            &[Action::fee(bob_fee)],
            spends,
            Conditions::new(),
        )?;

        let reveal = VaultSpendReveal {
            launcher_id: bob.launcher_id(),
            custody_hash: bob.custody_hash(),
            delegated_spend: result.delegated_spend,
        };

        let tx = VaultTransaction::parse(&mut ctx, &reveal, result.coin_spends)?;

        if disable_settlement_assertions {
            assert_eq!(tx.payments.len(), 0);
            assert_eq!(tx.fee_paid, bob_fee);
            assert_eq!(tx.total_fee, bob_fee);
        } else {
            assert_eq!(tx.payments.len(), 1);
            assert_eq!(tx.fee_paid, bob_fee);
            assert_eq!(tx.total_fee, bob_fee);

            let payment = &tx.payments[0];
            assert_eq!(payment.transfer_type, TransferType::Received);
            assert_eq!(payment.asset_id, asset_id);
            assert_eq!(payment.p2_puzzle_hash, bob.puzzle_hash());
            assert_eq!(payment.coin.amount, 1000);
        }

        Ok(())
    }

    #[rstest]
    fn test_clear_signing_nft_lifecycle() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

        let result = alice.spend(&mut sim, &mut ctx, &[Action::mint_empty_nft()])?;

        let reveal = VaultSpendReveal {
            launcher_id: alice.launcher_id(),
            custody_hash: alice.custody_hash(),
            delegated_spend: result.delegated_spend,
        };

        let tx = VaultTransaction::parse(&mut ctx, &reveal, result.coin_spends)?;

        assert_eq!(tx.payments.len(), 1);
        assert_eq!(tx.nfts.len(), 1);
        assert_eq!(tx.fee_paid, 0);
        assert_eq!(tx.total_fee, 0);

        // Even though this is for an NFT mint, the launcher is tracked as a sent payment
        let payment = &tx.payments[0];
        assert_eq!(payment.transfer_type, TransferType::Sent);
        assert_eq!(payment.p2_puzzle_hash, SINGLETON_LAUNCHER_HASH.into());
        assert_eq!(payment.coin.amount, 0);

        // The NFT should be included
        let nft = &tx.nfts[0];
        assert_eq!(nft.transfer_type, TransferType::Updated);
        assert_eq!(nft.p2_puzzle_hash, alice.puzzle_hash());
        assert!(!nft.includes_unverifiable_updates);

        // Transfer the NFT to Bob
        let nft_id = Id::Existing(nft.launcher_id);
        let bob_hint = ctx.hint(bob.puzzle_hash())?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[Action::send(nft_id, bob.puzzle_hash(), 1, bob_hint)],
        )?;

        let reveal = VaultSpendReveal {
            launcher_id: alice.launcher_id(),
            custody_hash: alice.custody_hash(),
            delegated_spend: result.delegated_spend,
        };

        let tx = VaultTransaction::parse(&mut ctx, &reveal, result.coin_spends)?;

        assert_eq!(tx.payments.len(), 0);
        assert_eq!(tx.nfts.len(), 1);
        assert_eq!(tx.fee_paid, 0);
        assert_eq!(tx.total_fee, 0);

        let nft = &tx.nfts[0];
        assert_eq!(nft.transfer_type, TransferType::Sent);
        assert_eq!(nft.p2_puzzle_hash, bob.puzzle_hash());
        assert!(!nft.includes_unverifiable_updates);

        Ok(())
    }
}
