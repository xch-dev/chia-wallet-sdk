use std::collections::{HashMap, HashSet};

use chia_protocol::Bytes32;
use chia_puzzle_types::cat::CatSolution;
use chia_sdk_types::{Condition, Mod, puzzles::SingletonMember};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::tree_hash;
use clvmr::NodePtr;
use indexmap::{IndexMap, IndexSet};

use crate::{
    AssertedNotarizedPayment, AssertedPayment, ClawbackInfo, ClearSigningAsset, CustodyInfo,
    DriverError, DropCoin, Facts, Issuance, IssuanceKind, LinkedOffer, P2ConditionsOrSingletonInfo,
    P2SingletonInfo, ParsedAsset, ParsedChild, ParsedSpend, RevealedCoinSpend, Reveals, Spend,
    SpendContext, TransferType, VaultMessage, VaultOutput, build_linked_offer,
    get_extra_delta_message, mips_puzzle_hash, parse_asserted_requested_payments, parse_children,
    parse_run_cat_tail, parse_spend, parse_vault_delegated_spend, split_asserted_payments,
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
    /// Settlement payments which were both revealed and asserted by the transaction.
    pub asserted_payments: Vec<AssertedNotarizedPayment>,
    /// Individual asserted payment outputs whose puzzle hash matches one of the vault's known p2 puzzle hashes.
    pub received_payments: Vec<AssertedPayment>,
    /// Individual asserted payment outputs whose puzzle hash doesn't match one of the vault's known p2 puzzle hashes.
    /// These are not necessarily paid by the vault, but the transaction requires them to happen.
    pub external_payments: Vec<AssertedPayment>,
    /// Per-asset value flow for verified spends and asserted payments.
    pub asset_flows: Vec<AssetFlow>,
    /// If this transaction creates one or more offer pre-split coins, this rolls them up into a
    /// description of the future offer. Per-leg details (the individual pre-split amounts) live
    /// in the children's transfer type field.
    ///
    /// [`None`] means the transaction does not link any offer pre-split coins.
    pub linked_offer: Option<LinkedOffer>,
    /// The amount of fees reserved by coin spends authorized by the vault.
    pub reserved_fee: u64,
    /// The known p2 puzzle hashes of the vault, based on revealed nonces (the first address is included by default).
    pub p2_puzzle_hashes: Vec<Bytes32>,
    /// The delegated puzzle hash that is being signed for.
    pub delegated_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetFlow {
    pub asset: ClearSigningAsset,
    pub input_amount: u64,
    pub output_amount: u64,
    pub issued_amount: u64,
    pub melted_amount: u64,
    pub received_amount: u64,
    pub paid_amount: u64,
    pub unaccounted_amount: u64,
}

#[derive(Debug, Clone)]
pub struct VerifiedSpend {
    pub asset: ParsedAsset,
    pub clawback: Option<ClawbackInfo>,
    pub custody: CustodyInfo,
    pub children: Vec<ParsedChild>,
    pub revoked: bool,
}

pub fn parse_vault_transaction(
    mut reveals: Reveals,
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    delegated_spend: Spend,
) -> Result<VaultTransaction, DriverError> {
    let mut facts = Facts::default();

    let vault_spend = parse_vault_delegated_spend(&mut facts, ctx, delegated_spend)?;

    let mut parsed_spends = HashMap::new();

    for spend in reveals.coin_spends().copied().collect::<Vec<_>>() {
        let parsed_spend = parse_spend(&mut reveals, ctx, &spend)?;

        if let Some(
            CustodyInfo::P2Singleton(P2SingletonInfo {
                launcher_id: spend_launcher_id,
                ..
            })
            | CustodyInfo::P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo {
                launcher_id: spend_launcher_id,
                ..
            }),
        ) = &parsed_spend.custody
            && spend_launcher_id != &launcher_id
        {
            continue;
        }

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
        let spend = *reveals
            .coin_spend(coin_id)
            .ok_or(DriverError::MissingSpend)?;
        let parsed_spend = parsed_spends
            .remove(&coin_id)
            .ok_or(DriverError::MissingSpend)?;

        let Some(verified_spend) = verify_spend(
            &mut reveals,
            &mut facts,
            ctx,
            spend,
            parsed_spend,
            &messages,
            &mut issuances,
        )?
        else {
            return Err(DriverError::InvalidLinkedCustody);
        };

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
        if !facts.is_spend_asserted(coin_id) {
            continue;
        }

        let Some(parsed_spend) = parsed_spends.remove(&coin_id) else {
            continue;
        };

        let spend = *reveals
            .coin_spend(coin_id)
            .ok_or(DriverError::MissingSpend)?;

        let Some(verified_spend) = verify_spend(
            &mut reveals,
            &mut facts,
            ctx,
            spend,
            parsed_spend,
            &[],
            &mut issuances,
        )?
        else {
            continue;
        };

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

    let delegated_puzzle_hash = tree_hash(ctx, delegated_spend.puzzle).into();

    let p2_puzzle_hashes = calculate_p2_puzzle_hashes(&reveals, launcher_id);
    let p2_puzzle_hash_set = p2_puzzle_hashes.iter().copied().collect();
    let reserved_fee = facts.reserved_fees().try_into()?;
    let asserted_payments = parse_asserted_requested_payments(&reveals, &facts, ctx)?;
    let split_payments = split_asserted_payments(
        &asserted_payments,
        &p2_puzzle_hash_set,
        reveals.asset_info(),
    );
    let linked_offer = build_linked_offer(
        &reveals,
        ctx,
        &verified_spends,
        launcher_id,
        &p2_puzzle_hash_set,
    )?;

    // Hydrate ephemerally spent bulletin children.
    let mut bulletins = HashMap::new();

    for spend in &verified_spends {
        if let ParsedAsset::Bulletin(bulletin) = &spend.asset {
            bulletins.insert(bulletin.coin.coin_id(), bulletin.clone());
        }
    }

    for spend in &mut verified_spends {
        for child in &mut spend.children {
            if matches!(child.asset, ParsedAsset::Xch(_))
                && let Some(bulletin) = bulletins.remove(&child.asset.coin().coin_id())
            {
                child.asset = ParsedAsset::Bulletin(bulletin);
            }
        }
    }

    let asset_flows = build_asset_flows(
        &verified_spends,
        &split_payments.received_payments,
        &split_payments.external_payments,
        &issuances,
        reserved_fee,
    );

    Ok(VaultTransaction {
        vault_child: vault_spend.child,
        drop_coins: vault_spend.drop_coins,
        spends: verified_spends,
        issuances,
        asserted_payments,
        received_payments: split_payments.received_payments,
        external_payments: split_payments.external_payments,
        asset_flows,
        linked_offer,
        reserved_fee,
        p2_puzzle_hashes,
        delegated_puzzle_hash,
    })
}

fn verify_spend(
    reveals: &mut Reveals,
    facts: &mut Facts,
    allocator: &mut SpendContext,
    spend: RevealedCoinSpend,
    parsed_spend: ParsedSpend,
    messages: &[VaultMessage],
    issuances: &mut Vec<Issuance>,
) -> Result<Option<VerifiedSpend>, DriverError> {
    let Some(custody) = parsed_spend.custody else {
        return Ok(None);
    };

    let conditions: &[Condition] = match &custody {
        CustodyInfo::P2Singleton(P2SingletonInfo { conditions, .. })
        | CustodyInfo::P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo {
            conditions, ..
        })
        | CustodyInfo::DelegatedConditions(conditions) => conditions,
    };

    if matches!(&custody, CustodyInfo::P2ConditionsOrSingleton(_))
        && !conditions.contains(&Condition::assert_my_coin_id(spend.coin.coin_id()))
    {
        return Err(DriverError::MissingP2ConditionsOrSingletonAssertion);
    }

    if messages.is_empty() && custody.receives_message() {
        return Err(DriverError::MissingVaultMessage);
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
            coin_id: spend.coin.coin_id(),
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
        } else if let Some(issuance) = &issuance
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
        reveals,
        facts,
        allocator,
        &parsed_spend.asset,
        spend,
        conditions,
        parsed_spend.required_expiration_time.is_some(),
    )?;

    if let Some(issuance) = issuance {
        issuances.push(issuance);
    }

    Ok(Some(VerifiedSpend {
        asset: parsed_spend.asset,
        clawback: parsed_spend.clawback,
        custody,
        children,
        revoked: parsed_spend.revoked,
    }))
}

#[derive(Debug, Clone)]
struct AssetFlowTotals {
    asset: ClearSigningAsset,
    input_amount: u64,
    output_amount: u64,
    issued_amount: u64,
    melted_amount: u64,
    received_amount: u64,
    paid_amount: u64,
}

fn build_asset_flows(
    spends: &[VerifiedSpend],
    received_payments: &[AssertedPayment],
    external_payments: &[AssertedPayment],
    issuances: &[Issuance],
    reserved_fee: u64,
) -> Vec<AssetFlow> {
    let child_coin_ids: HashSet<Bytes32> = spends
        .iter()
        .flat_map(|spend| spend.children.iter())
        .map(|child| child.asset.coin().coin_id())
        .collect();
    let spend_by_coin_id: HashMap<Bytes32, &VerifiedSpend> = spends
        .iter()
        .map(|spend| (spend.asset.coin().coin_id(), spend))
        .collect();
    let xch_child_coin_ids: HashSet<Bytes32> = spends
        .iter()
        .filter(|spend| matches!(spend.asset, ParsedAsset::Xch(_) | ParsedAsset::Bulletin(_)))
        .flat_map(|spend| spend.children.iter())
        .map(|child| child.asset.coin().coin_id())
        .collect();

    let mut flows = IndexMap::<Option<Bytes32>, AssetFlowTotals>::new();

    for spend in spends {
        if !child_coin_ids.contains(&spend.asset.coin().coin_id()) {
            asset_flow_mut(&mut flows, asset_from_parsed(&spend.asset)).input_amount +=
                spend.asset.coin().amount;
        }

        for child in &spend.children {
            if !spend_by_coin_id.contains_key(&child.asset.coin().coin_id())
                && child.transfer_type != TransferType::Offered
            {
                asset_flow_mut(&mut flows, asset_from_parsed(&child.asset)).output_amount +=
                    child.asset.coin().amount;
            }
        }
    }

    for asserted_payment in received_payments {
        asset_flow_mut(&mut flows, asserted_payment.asset).received_amount +=
            asserted_payment.payment.amount;
    }

    for asserted_payment in external_payments {
        asset_flow_mut(&mut flows, asserted_payment.asset).paid_amount +=
            asserted_payment.payment.amount;
    }

    for issuance in issuances {
        let Some(spend) = spend_by_coin_id.get(&issuance.coin_id) else {
            continue;
        };

        let ParsedAsset::Cat(cat) = &spend.asset else {
            continue;
        };

        if cat.info.asset_id != issuance.asset_id {
            continue;
        }

        let cat_asset = asset_from_parsed(&spend.asset);

        if xch_child_coin_ids.contains(&issuance.coin_id) {
            let amount = spend.asset.coin().amount;
            asset_flow_mut(&mut flows, cat_asset).issued_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount += amount;
        }

        if issuance.extra_delta > 0 {
            let amount = u64::try_from(issuance.extra_delta).unwrap();
            asset_flow_mut(&mut flows, cat_asset).issued_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount += amount;
        } else if issuance.extra_delta < 0 {
            let amount = u64::try_from(-issuance.extra_delta).unwrap();
            asset_flow_mut(&mut flows, cat_asset).melted_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).issued_amount += amount;
        }
    }

    flows
        .into_values()
        .filter_map(|flow| {
            let unaccounted_amount = flow
                .input_amount
                .saturating_add(flow.issued_amount)
                .saturating_sub(flow.output_amount)
                .saturating_sub(flow.melted_amount)
                .saturating_sub(flow.paid_amount)
                .saturating_sub(if matches!(flow.asset, ClearSigningAsset::Xch) {
                    reserved_fee
                } else {
                    0
                });

            let include = flow.input_amount > 0
                || flow.output_amount > 0
                || flow.received_amount > 0
                || flow.paid_amount > 0
                || flow.issued_amount > 0
                || flow.melted_amount > 0;

            include.then_some((flow, unaccounted_amount))
        })
        .map(|(flow, unaccounted_amount)| AssetFlow {
            asset: flow.asset,
            input_amount: flow.input_amount,
            output_amount: flow.output_amount,
            issued_amount: flow.issued_amount,
            melted_amount: flow.melted_amount,
            received_amount: flow.received_amount,
            paid_amount: flow.paid_amount,
            unaccounted_amount,
        })
        .collect()
}

fn asset_flow_mut(
    flows: &mut IndexMap<Option<Bytes32>, AssetFlowTotals>,
    asset: ClearSigningAsset,
) -> &mut AssetFlowTotals {
    flows
        .entry(asset_flow_key(asset))
        .or_insert_with(|| AssetFlowTotals {
            asset,
            input_amount: 0,
            output_amount: 0,
            issued_amount: 0,
            melted_amount: 0,
            received_amount: 0,
            paid_amount: 0,
        })
}

fn asset_flow_key(asset: ClearSigningAsset) -> Option<Bytes32> {
    match asset {
        ClearSigningAsset::Xch => None,
        ClearSigningAsset::Cat { asset_id, .. }
        | ClearSigningAsset::Nft {
            launcher_id: asset_id,
            ..
        } => Some(asset_id),
    }
}

fn asset_from_parsed(asset: &ParsedAsset) -> ClearSigningAsset {
    match asset {
        ParsedAsset::Xch(_) | ParsedAsset::Bulletin(_) => ClearSigningAsset::Xch,
        ParsedAsset::Cat(cat) => ClearSigningAsset::Cat {
            asset_id: cat.info.asset_id,
            hidden_puzzle_hash: cat.info.hidden_puzzle_hash,
        },
        ParsedAsset::Nft(nft) => ClearSigningAsset::Nft {
            launcher_id: nft.info.launcher_id,
            metadata: nft.info.metadata,
            metadata_updater_puzzle_hash: nft.info.metadata_updater_puzzle_hash,
            royalty_puzzle_hash: nft.info.royalty_puzzle_hash,
            royalty_basis_points: nft.info.royalty_basis_points,
        },
    }
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

pub fn iter_final_children(spends: &[VerifiedSpend]) -> impl Iterator<Item = &ParsedChild> {
    let spent_coin_ids: HashSet<Bytes32> = spends
        .iter()
        .map(|spend| spend.asset.coin().coin_id())
        .collect();

    spends
        .iter()
        .flat_map(|spend| spend.children.iter())
        .filter(move |child| !spent_coin_ids.contains(&child.asset.coin().coin_id()))
}
