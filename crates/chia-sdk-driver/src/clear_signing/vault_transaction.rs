use std::collections::{HashMap, HashSet};

use chia_protocol::{Bytes32, CoinSpend};
use chia_puzzle_types::cat::CatSolution;
use chia_sdk_types::{Condition, Mod, puzzles::SingletonMember};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};
use indexmap::{IndexMap, IndexSet};

use crate::{
    AssertedRequestedPayment, ClawbackInfo, ClawbackV2, CustodyInfo, DriverError, DropCoin, Facts,
    Issuance, IssuanceKind, LinkedOffer, OfferPreSplitInfo, P2ConditionsOrSingletonInfo,
    P2ConditionsOrSingletonRevealInput, P2PuzzleType, P2SingletonInfo, ParsedAsset, ParsedChild,
    ParsedSpend, Reveals, Spend, VaultMessage, VaultOutput, get_extra_delta_message,
    mips_puzzle_hash, parse_asserted_requested_payments, parse_children, parse_run_cat_tail,
    parse_spend, parse_vault_delegated_spend,
};

/// The purpose of this is to provide sufficient information to verify what is happening to a vault and its assets
/// as a result of a transaction at a glance. Information that is not verifiable should not be included or displayed.
/// We can still allow transactions which are not fully verifiable, but a conservative summary should be provided.
#[derive(Debug, Clone)]
pub struct VaultTransaction {
    /// If a new vault coin is created (i.e. the vault isn't melted), this will be set.
    /// It's the new inner puzzle hash and amount of the vault singleton. If the puzzle hash is different, the custody
    /// configuration has changed. If the amount is different, XCH is being added or removed from its value. The hash
    /// can be validated against a [`MipsMemo`](crate::MipsMemo) so that you know what specifically is happening.
    pub vault_child: Option<VaultOutput>,
    /// Coins which were created as outputs of the vault singleton spend itself, for example to mint NFTs.
    pub drop_coins: Vec<DropCoin>,
    /// The spends (and their children) which were authorized by the vault.
    pub spends: Vec<VerifiedSpend>,
    /// CAT supply changes (mints or melts) authorized by spends in this transaction. Each issuance
    /// records the coin id of the spend that emitted the `RunCatTail` condition, so callers can
    /// match it back to the corresponding `VerifiedSpend` by coin id if they want to.
    pub issuances: Vec<Issuance>,
    /// If this transaction creates one or more offer pre-split coins, this rolls them up into a
    /// description of the future offer. Per-leg details (the individual pre-split children) live
    /// on the children themselves via [`P2PuzzleType::OfferPreSplit`].
    ///
    /// `None` means the transaction does not link any offer pre-split coins.
    pub linked_offer: Option<LinkedOffer>,
    /// Requested payments which were both revealed and asserted by the vault spend. These are assets which are going
    /// to be received when and if the transaction is confirmed on-chain.
    pub received_payments: Vec<AssertedRequestedPayment>,
    /// Total fees (different between input and output amounts) paid by coin spends authorized by the vault.
    /// If the transaction is signed, the fee is guaranteed to be at least this amount, unless it's not reserved.
    /// The reason to include unreserved fees is to make it clear that the XCH is leaving the vault due to this transaction.
    pub fee_paid: u64,
    /// The amount of fees reserved by coin spends authorized by the vault.
    /// If this is greater than or equal to the fee paid, you can be sure that the XCH spent for fees will not be
    /// maliciously redirected for some other purpose by the submitter of the transaction after signing.
    pub reserved_fee: u64,
    /// The launcher id of the vault, based on the spends authorized by the delegated spend. If there were no other spends,
    /// this is unknowable, unless the signer has this information stored somewhere locally.
    pub launcher_id: Option<Bytes32>,
    /// The known p2 puzzle hashes of the vault, based on revealed nonces (the first address is included by default).
    pub p2_puzzle_hashes: Vec<Bytes32>,
    /// The delegated puzzle hash that is being signed for.
    pub delegated_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone)]
pub struct VerifiedSpend {
    pub asset: ParsedAsset,
    pub clawback: Option<ClawbackInfo>,
    pub custody: CustodyInfo,
    pub children: Vec<ParsedChild>,
}

pub fn parse_vault_transaction(
    allocator: &mut Allocator,
    delegated_spend: Spend,
    coin_spends: Vec<CoinSpend>,
    spent_clawbacks: Vec<ClawbackV2>,
    p2_conditions_or_singletons: Vec<P2ConditionsOrSingletonRevealInput>,
) -> Result<VaultTransaction, DriverError> {
    let mut facts = Facts::default();

    let reveals = Reveals::from_spends(
        allocator,
        coin_spends,
        spent_clawbacks,
        p2_conditions_or_singletons,
    )?;
    let vault_spend = parse_vault_delegated_spend(&mut facts, allocator, delegated_spend)?;

    let mut parsed_spends = HashMap::new();

    for spend in reveals.coin_spends() {
        let parsed_spend = parse_spend(&reveals, allocator, spend)?;
        parsed_spends.insert(spend.coin.coin_id(), parsed_spend);
    }

    let mut messages_by_coin: IndexMap<Bytes32, Vec<VaultMessage>> = IndexMap::new();
    for message in vault_spend.messages {
        messages_by_coin
            .entry(message.spent_coin_id)
            .or_default()
            .push(message);
    }

    let mut verified_spends = Vec::new();
    let mut issuances: Vec<Issuance> = Vec::new();

    for (coin_id, messages) in messages_by_coin {
        let verified_spend = verify_spend(
            &reveals,
            &mut facts,
            allocator,
            &mut parsed_spends,
            coin_id,
            &messages,
            &mut issuances,
        )?;

        verified_spends.push(verified_spend);
    }

    let mut stack: IndexSet<Bytes32> = verified_spends
        .iter()
        .flat_map(|spend| {
            spend
                .children
                .iter()
                .map(|child| child.asset.coin().coin_id())
        })
        .collect();

    while let Some(coin_id) = stack.pop() {
        if !facts.is_spend_asserted(coin_id) || !parsed_spends.contains_key(&coin_id) {
            continue;
        }

        let verified_spend = verify_spend(
            &reveals,
            &mut facts,
            allocator,
            &mut parsed_spends,
            coin_id,
            &[],
            &mut issuances,
        )?;

        for child in &verified_spend.children {
            stack.insert(child.asset.coin().coin_id());
        }

        verified_spends.push(verified_spend);
    }

    // If the transaction expires after the required expiration time of the spend,
    // we can't guarantee that the transaction will expire when the spend expires,
    // which is a security vulnerability.
    if facts.required_expiration_time().is_some_and(|required| {
        facts
            .actual_expiration_time()
            .is_none_or(|expiration| expiration > required)
    }) {
        return Err(DriverError::UnguaranteedClawBack);
    }

    let delegated_puzzle_hash = tree_hash(allocator, delegated_spend.puzzle).into();

    let reserved_fee = facts.reserved_fees().try_into()?;

    let mut input_amount = 0;
    let mut output_amount = 0;

    for spend in &verified_spends {
        input_amount += u128::from(spend.asset.coin().amount);

        for child in &spend.children {
            output_amount += u128::from(child.asset.coin().amount);
        }
    }

    let fee_paid = (input_amount - output_amount).try_into()?;
    let received_payments = parse_asserted_requested_payments(&reveals, &facts, allocator)?;
    let launcher_id = find_launcher_id(&verified_spends)?;
    let p2_puzzle_hashes = if let Some(launcher_id) = launcher_id {
        calculate_p2_puzzle_hashes(&reveals, launcher_id)
    } else {
        Vec::new()
    };

    let linked_offer = build_linked_offer(&verified_spends, launcher_id, &reveals, allocator)?;

    Ok(VaultTransaction {
        vault_child: vault_spend.child,
        drop_coins: vault_spend.drop_coins,
        spends: verified_spends,
        issuances,
        linked_offer,
        received_payments,
        fee_paid,
        reserved_fee,
        launcher_id,
        p2_puzzle_hashes,
        delegated_puzzle_hash,
    })
}

fn verify_spend(
    reveals: &Reveals,
    facts: &mut Facts,
    allocator: &mut Allocator,
    parsed_spends: &mut HashMap<Bytes32, ParsedSpend>,
    coin_id: Bytes32,
    messages: &[VaultMessage],
    issuances: &mut Vec<Issuance>,
) -> Result<VerifiedSpend, DriverError> {
    let Some(parsed_spend) = parsed_spends.remove(&coin_id) else {
        return Err(DriverError::MissingSpend);
    };

    let Some(spend) = reveals.coin_spend(coin_id) else {
        return Err(DriverError::MissingSpend);
    };

    let Some(custody) = parsed_spend.custody else {
        return Err(DriverError::InvalidLinkedCustody);
    };

    let conditions: &[Condition] = match &custody {
        CustodyInfo::P2Singleton(P2SingletonInfo { conditions, .. })
        | CustodyInfo::P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo {
            conditions, ..
        })
        | CustodyInfo::DelegatedConditions(conditions) => conditions,
    };

    if messages.is_empty() && custody.receives_message() {
        return Err(DriverError::InvalidLinkedCustody);
    }

    let conditions_hash = if custody.receives_message() {
        let delegated_puzzle = clvm_quote!(conditions).to_clvm(allocator)?;
        Some(tree_hash(allocator, delegated_puzzle))
    } else {
        None
    };

    let run_cat_tail = if matches!(parsed_spend.asset, ParsedAsset::Cat(_)) {
        parse_run_cat_tail(allocator, conditions)?
    } else {
        None
    };

    let issuance = if let Some(run_cat_tail) = run_cat_tail {
        let cat_solution = CatSolution::<NodePtr>::from_clvm(allocator, spend.solution)?;

        Some(Issuance {
            coin_id,
            asset_id: run_cat_tail.asset_id,
            extra_delta: cat_solution.extra_delta,
            kind: run_cat_tail.kind,
        })
    } else {
        None
    };

    let mut tail_matched = false;
    let mut custody_matched = false;

    for message in messages {
        if let Some(hash) = conditions_hash
            && message.data.as_ref() == hash.as_ref()
        {
            if custody_matched {
                return Err(DriverError::DuplicateVaultMessage);
            }

            custody_matched = true;
        } else if let Some(issuance) = issuance
            && matches!(issuance.kind, IssuanceKind::EverythingWithSingleton { .. })
            && message.data == get_extra_delta_message(issuance.extra_delta)
        {
            if tail_matched {
                return Err(DriverError::DuplicateVaultMessage);
            }

            tail_matched = true;
        } else {
            return Err(DriverError::UnmatchedVaultMessage);
        }
    }

    if custody.receives_message() && !custody_matched {
        return Err(DriverError::WrongConditions);
    }

    if let Some(time) = parsed_spend.required_expiration_time {
        facts.update_required_expiration_time(time);
    }

    let children = parse_children(
        facts,
        allocator,
        reveals,
        &parsed_spend.asset,
        spend,
        conditions,
        parsed_spend.required_expiration_time.is_some(),
    )?;

    if let Some(issuance) = issuance {
        issuances.push(issuance);
    }

    Ok(VerifiedSpend {
        asset: parsed_spend.asset,
        clawback: parsed_spend.clawback,
        custody,
        children,
    })
}

fn find_launcher_id(spends: &[VerifiedSpend]) -> Result<Option<Bytes32>, DriverError> {
    let mut launcher_id = None;

    for spend in spends {
        let (CustodyInfo::P2Singleton(P2SingletonInfo {
            launcher_id: spend_launcher_id,
            ..
        })
        | CustodyInfo::P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo {
            launcher_id: spend_launcher_id,
            ..
        })) = &spend.custody
        else {
            continue;
        };

        let Some(launcher_id) = launcher_id else {
            launcher_id = Some(*spend_launcher_id);
            continue;
        };

        if launcher_id != *spend_launcher_id {
            return Err(DriverError::ConflictingVaultLauncherIds);
        }
    }

    Ok(launcher_id)
}

fn calculate_p2_puzzle_hashes(reveals: &Reveals, launcher_id: Bytes32) -> Vec<Bytes32> {
    let mut p2_puzzle_hashes = Vec::new();

    for nonce in reveals.vault_nonces() {
        p2_puzzle_hashes.push(
            mips_puzzle_hash(
                nonce,
                vec![],
                SingletonMember::new(launcher_id).curry_tree_hash(),
                true,
            )
            .into(),
        );
    }

    p2_puzzle_hashes
}

/// Aggregate every offer pre-split child across all verified spends into a single [`LinkedOffer`],
/// rejecting transactions whose pre-split coins disagree about the future offer.
fn build_linked_offer(
    spends: &[VerifiedSpend],
    launcher_id: Option<Bytes32>,
    reveals: &Reveals,
    allocator: &Allocator,
) -> Result<Option<LinkedOffer>, DriverError> {
    // Collect each pre-split leg with its parent input amount and asset kind. We carry the amount
    // and "is XCH" flag separately so the per-leg fee check below doesn't have to re-walk children.
    let mut legs: Vec<(u64, bool, &OfferPreSplitInfo)> = Vec::new();
    for spend in spends {
        for child in &spend.children {
            if let P2PuzzleType::OfferPreSplit(info) = &child.p2_puzzle_type {
                let is_xch = matches!(child.asset, ParsedAsset::Xch(_));
                legs.push((child.asset.coin().amount, is_xch, info));
            }
        }
    }

    if legs.is_empty() {
        return Ok(None);
    }

    // Every leg's launcher id must agree with the main launcher id discovered from the verified
    // spends. A pre-split coin pointing at a different vault isn't ours to cancel and shouldn't
    // be surfaced as our offer.
    let main_launcher = launcher_id.ok_or(DriverError::LinkedOfferLauncherMismatch)?;
    for (_, _, info) in &legs {
        if info.launcher_id != main_launcher {
            return Err(DriverError::LinkedOfferLauncherMismatch);
        }
    }

    // Each XCH leg's fixed conditions must balance: the sum of its `CreateCoin` amounts plus its
    // `ReserveFee` amounts must equal the parent's input amount. The reserve-fee total is the
    // amount we'll surface to the user, so it has to be accurate. CAT legs are skipped because
    // `ReserveFee` is an XCH-only concept.
    let mut reserved_fee: u64 = 0;
    for (input_amount, is_xch, info) in &legs {
        if !*is_xch {
            continue;
        }

        let mut output_total: u64 = 0;
        let mut leg_reserve_fee: u64 = 0;
        for condition in &info.fixed_conditions {
            match condition {
                Condition::CreateCoin(create_coin) => {
                    output_total = output_total
                        .checked_add(create_coin.amount)
                        .ok_or(DriverError::LinkedOfferFeeMismatch)?;
                }
                Condition::ReserveFee(rf) => {
                    leg_reserve_fee = leg_reserve_fee
                        .checked_add(rf.amount)
                        .ok_or(DriverError::LinkedOfferFeeMismatch)?;
                }
                _ => {}
            }
        }

        let expected_fee = input_amount
            .checked_sub(output_total)
            .ok_or(DriverError::LinkedOfferFeeMismatch)?;

        if leg_reserve_fee != expected_fee {
            return Err(DriverError::LinkedOfferFeeMismatch);
        }

        reserved_fee = reserved_fee
            .checked_add(leg_reserve_fee)
            .ok_or(DriverError::LinkedOfferFeeMismatch)?;
    }

    // All legs must assert the same set of puzzle announcements — that's how we know they really
    // describe a single offer. We compare by content (HashSet equality) so ordering and within-leg
    // duplicates don't trigger spurious mismatches.
    let baseline = announcement_set(legs[0].2);
    for (_, _, info) in legs.iter().skip(1) {
        if announcement_set(info) != baseline {
            return Err(DriverError::LinkedOfferAnnouncementMismatch);
        }
    }

    // Match the offer's announcement set against the requested payments revealed in the
    // transaction. We feed `parse_asserted_requested_payments` an offer-local `Facts` so that
    // the transaction's main `received_payments` aren't conflated with the offer's.
    let mut offer_facts = Facts::default();
    for announcement_id in baseline {
        offer_facts.assert_puzzle_announcement(announcement_id);
    }

    let requested_payments = parse_asserted_requested_payments(reveals, &offer_facts, allocator)?;

    Ok(Some(LinkedOffer {
        reserved_fee,
        requested_payments,
    }))
}

fn announcement_set(info: &OfferPreSplitInfo) -> HashSet<Bytes32> {
    info.fixed_conditions
        .iter()
        .filter_map(|condition| match condition {
            Condition::AssertPuzzleAnnouncement(c) => Some(c.announcement_id),
            _ => None,
        })
        .collect()
}
