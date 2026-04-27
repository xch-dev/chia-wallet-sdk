use std::collections::HashMap;

use chia_protocol::{Bytes32, CoinSpend};
use chia_sdk_types::{Condition, Mod, puzzles::SingletonMember};
use clvm_traits::{ToClvm, clvm_quote};
use clvm_utils::tree_hash;
use clvmr::Allocator;
use indexmap::{IndexMap, IndexSet};

use crate::{
    AssertedRequestedPayment, ClawbackInfo, ClawbackV2, CustodyInfo, DriverError, DropCoin, Facts,
    Issuance, IssuanceKind, P2ConditionsOrSingletonInfo, P2SingletonInfo, ParsedAsset, ParsedChild,
    ParsedSpend, Reveals, Spend, VaultMessage, VaultOutput, mips_puzzle_hash,
    parse_asserted_requested_payments, parse_cat_extra_delta, parse_children, parse_run_cat_tails,
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
) -> Result<VaultTransaction, DriverError> {
    let mut facts = Facts::default();

    let reveals = Reveals::from_spends(allocator, coin_spends, spent_clawbacks)?;
    let vault_spend = parse_vault_delegated_spend(&mut facts, allocator, delegated_spend)?;

    let mut parsed_spends = HashMap::new();

    for spend in reveals.coin_spends() {
        let parsed_spend = parse_spend(&reveals, allocator, spend)?;
        parsed_spends.insert(spend.coin.coin_id(), parsed_spend);
    }

    // Group messages by their target coin id, preserving the order of first occurrence so that the
    // resulting verified spends still appear in the order the user authorized them. This lets one
    // coin receive multiple messages — for example a CAT eve coin receiving both a custody message
    // for its inner p2 puzzle and a TAIL message for its `EverythingWithSingleton` issuance.
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
        if !facts.is_spend_asserted(coin_id) {
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

    Ok(VaultTransaction {
        vault_child: vault_spend.child,
        drop_coins: vault_spend.drop_coins,
        spends: verified_spends,
        issuances,
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

    // Pull the trusted inner conditions out of the custody. This requires the custody to be one
    // of the recognized types: P2 singleton, P2 conditions or singleton, or top-level delegated
    // conditions. Any other custody type means we can't see the inner conditions, so we can't
    // verify what the spend will produce (or whether it issues a CAT), and the spend must be
    // rejected. This is also what guarantees that any `RunCatTail` we find here can't be
    // substituted with something else (e.g. a `ReceiveMessage`) without invalidating the user's
    // signature.
    let conditions: &[Condition] = match &custody {
        CustodyInfo::P2Singleton(P2SingletonInfo { conditions, .. })
        | CustodyInfo::P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo {
            conditions, ..
        })
        | CustodyInfo::DelegatedConditions(conditions) => conditions,
    };

    // Custody type rules:
    //   * Messaged spends must use one of the singleton-message-receiving custody types.
    //   * Chained spends (no message) must use top-level delegated conditions.
    if messages.is_empty() {
        if !matches!(custody, CustodyInfo::DelegatedConditions(_)) {
            return Err(DriverError::InvalidLinkedCustody);
        }
    } else if !matches!(
        custody,
        CustodyInfo::P2Singleton(_) | CustodyInfo::P2ConditionsOrSingleton(_)
    ) {
        return Err(DriverError::InvalidLinkedCustody);
    }

    // The hash of the conditions, used to match custody-auth messages.
    let conditions_hash = if matches!(
        custody,
        CustodyInfo::P2Singleton(_) | CustodyInfo::P2ConditionsOrSingleton(_)
    ) {
        let delegated_puzzle = clvm_quote!(conditions).to_clvm(allocator)?;
        Some(tree_hash(allocator, delegated_puzzle))
    } else {
        None
    };

    // Find every TAIL invocation in the trusted inner conditions. Only meaningful for CAT spends —
    // the CAT layer is what runs the TAIL, so for non-CAT assets a `RunCatTail` condition has no
    // effect and we ignore it.
    let tail_invocations = if matches!(parsed_spend.asset, ParsedAsset::Cat(_)) {
        parse_run_cat_tails(allocator, conditions)?
    } else {
        Vec::new()
    };

    // Match each vault message to either a custody-auth slot or a TAIL-auth slot, one-to-one. A
    // message that doesn't match anything is a fatal error: silently authorizing a message we
    // don't understand is exactly the security hole the clear signer exists to prevent.
    //
    // We don't need to verify that an `EverythingWithSingleton` TAIL's curried `singleton_struct_hash`
    // matches this vault — the vault is the sender of the `SendMessage`, so consensus will only
    // pair it with a `RECEIVE_MESSAGE` that names the vault's full puzzle hash as sender. If the
    // TAIL is for a different singleton, the transaction is invalid and won't be confirmed.
    let mut tail_matched = vec![false; tail_invocations.len()];
    let mut custody_matched = false;

    for message in messages {
        if let Some(hash) = conditions_hash
            && message.message.len() == 32
            && message.message.as_ref() == hash.as_ref()
        {
            if custody_matched {
                return Err(DriverError::DuplicateVaultMessage);
            }
            custody_matched = true;
            continue;
        }

        let mut matched = false;
        for (index, invocation) in tail_invocations.iter().enumerate() {
            if tail_matched[index] {
                continue;
            }

            // Only `EverythingWithSingleton` TAILs accept vault messages.
            if !matches!(invocation.kind, IssuanceKind::Singleton { .. }) {
                continue;
            }

            tail_matched[index] = true;
            matched = true;
            break;
        }

        if !matched {
            return Err(DriverError::UnmatchedVaultMessage);
        }
    }

    // Messaged spends require a custody-auth message: the inner p2 puzzle's RECEIVE_MESSAGE
    // wouldn't be satisfied otherwise, and the spend wouldn't actually run.
    if !messages.is_empty() && !custody_matched {
        return Err(DriverError::WrongConditions);
    }

    if let Some(time) = parsed_spend.required_expiration_time {
        facts.update_required_expiration_time(time);
    }

    let children = parse_children(
        facts,
        allocator,
        &parsed_spend.asset,
        spend,
        conditions,
        parsed_spend.required_expiration_time.is_some(),
    )?;

    // Record an Issuance for every TAIL invocation in this spend. Because the surrounding
    // conditions are pinned by custody, the issuance is guaranteed to happen as described — the
    // submitter cannot rewrite the `RunCatTail` into a `ReceiveMessage` (or anything else) without
    // invalidating the signature.
    if let ParsedAsset::Cat(cat) = &parsed_spend.asset
        && !tail_invocations.is_empty()
    {
        // `extra_delta` is taken from the CAT layer's outer solution, not derived from this coin's
        // conditions in isolation: in a multi-coin ring, the TAIL is run by exactly one coin and
        // sees the *ring-wide* delta, which is committed in that coin's solution.
        let delta = parse_cat_extra_delta(allocator, spend.solution)?;

        for invocation in &tail_invocations {
            issuances.push(Issuance {
                coin_id,
                asset_id: invocation.asset_id,
                hidden_puzzle_hash: cat.info.hidden_puzzle_hash,
                delta,
                kind: invocation.kind,
            });
        }
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
