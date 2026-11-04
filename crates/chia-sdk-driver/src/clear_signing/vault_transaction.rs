use std::collections::{HashMap, HashSet};

use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::cat::CatSolution;
use chia_sdk_types::{Condition, Mod, puzzles::SingletonMember};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::tree_hash;
use clvmr::NodePtr;
use indexmap::{IndexMap, IndexSet};

use crate::{
    AssertedNotarizedPayment, AssertedPayment, ClawbackInfo, ClawbackPath, ClawbackV2,
    ClearSigningAsset, ControlledVaultRecovery, CustodyInfo, DriverError, DropCoin, Facts,
    Issuance, IssuanceKind, Layer, LinkedOffer, P2ConditionsOrSingletonInfo, P2SingletonInfo,
    ParsedAsset, ParsedChild, ParsedSpend, RevealedCoinSpend, Reveals, Spend, SpendContext,
    TransferType, VAULT_CONTROLLER_MESSAGE_COIN_ID, VaultLayers, VaultMessage, VaultOutput,
    build_linked_offer, get_extra_delta_message, mips_puzzle_hash,
    parse_asserted_requested_payments, parse_children, parse_delegated_spend, parse_run_cat_tail,
    parse_spend, parse_vault_controller_envelope, parse_vault_delegated_spend,
    split_asserted_payments, vault_recovery_delegated_puzzle_hash,
};

/// The maximum depth of controlled vaults that are parsed below the vault being signed for, which bounds the work done
/// to parse a transaction.
pub const MAX_CONTROLLED_VAULT_DEPTH: usize = 8;

/// The amount of every vault coin, which clear signing requires each vault to be recreated with.
pub const VAULT_AMOUNT: u64 = 1;

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
    /// The spends (and their children) which were authorized by the vault. This includes clawback push-throughs that
    /// the vault asserts, which don't need its authorization, and whose coins aren't necessarily owned by the vault.
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
    /// If the delegated spend is a [`vault_message_envelope`](crate::vault_message_envelope), these are the
    /// arbitrary messages being signed, in order. In that case, the delegated spend contains nothing else other than
    /// recreating the vault, which is [`vault_child`](Self::vault_child). It should be recreated with the vault's
    /// current custody hash and amount, which [`verify_vault_signed_message`](crate::verify_vault_signed_message)
    /// requires, but that isn't checked here since the current custody hash isn't known.
    pub signed_messages: Vec<Bytes>,
    /// Vaults whose custody includes this vault (as a singleton member), and whose spends this vault authorizes by
    /// sending them a message with their delegated puzzle hash. Each one is parsed as its own transaction, with its own
    /// p2 puzzle hashes, which can in turn authorize the spends of other vaults that it controls.
    ///
    /// A controlled vault's puzzle is checked to be a vault, and its custody hash to be the one that its coin commits
    /// to, but not that its custody is controlled by this vault. Its transaction is what its delegated spend does,
    /// which only happens if the message can only be received through a singleton member of this vault. That depends on
    /// its custody, so a controlled vault should only be shown as the user's own if its launcher id is known to be.
    ///
    /// What each controlled vault does is shown separately, rather than being merged into this transaction, so that
    /// it's clear which vault is spending which assets. The facts of each transaction (such as which spends are
    /// asserted, and its time bounds) are also kept separately, which is conservative, since each vault's facts only
    /// come from its own conditions and the spends it authorizes.
    pub controlled_vaults: Vec<ControlledVaultTransaction>,
}

impl VaultTransaction {
    /// Whether this vault, or a vault that it controls, signs arbitrary messages with a
    /// [`vault_message_envelope`](crate::vault_message_envelope) rather than doing anything else.
    ///
    /// A vault that authorizes another vault to sign messages must use a
    /// [`vault_controller_envelope`](crate::vault_controller_envelope), which sends a message to
    /// [`VAULT_CONTROLLER_MESSAGE_COIN_ID`](crate::VAULT_CONTROLLER_MESSAGE_COIN_ID), so that its spend can never be
    /// valid on chain either. Each controlled vault is also required to be recreated as it was, which can't be checked
    /// for the vault being signed for, since its current custody hash isn't known.
    pub fn signs_messages(&self) -> bool {
        !self.signed_messages.is_empty()
            || self
                .controlled_vaults
                .iter()
                .any(|controlled_vault| controlled_vault.transaction.signs_messages())
    }
}

/// The spend of a vault that's controlled by another vault, and what it does.
#[derive(Debug, Clone)]
pub struct ControlledVaultTransaction {
    /// The controlled vault's coin. If the message is sent to its puzzle hash (for a fast-forwardable singleton member),
    /// only the puzzle hash is committed to, so the spend can happen on another coin of the vault with a different
    /// parent. Its amount is always [`VAULT_AMOUNT`] either way.
    pub coin: Coin,
    pub launcher_id: Bytes32,
    /// The controlled vault's current custody hash, which can be compared with the custody hash of its
    /// [`vault_child`](VaultTransaction::vault_child) to tell whether its custody is changing.
    pub custody_hash: Bytes32,
    /// Set if the spend is authorized through the controlled vault's recovery path, which starts a recovery.
    pub recovery: Option<ControlledVaultRecovery>,
    /// What the controlled vault does.
    pub transaction: VaultTransaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetFlow {
    pub asset: ClearSigningAsset,
    pub input_amount: u128,
    pub output_amount: u128,
    pub issued_amount: u128,
    pub melted_amount: u128,
    pub received_amount: u128,
    pub paid_amount: u128,
    pub unaccounted_amount: u128,
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
    reveals: Reveals,
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    delegated_spend: Spend,
) -> Result<VaultTransaction, DriverError> {
    let base_reveals = reveals.clone();
    let mut visited = HashSet::from([launcher_id]);
    parse_vault(
        reveals,
        &base_reveals,
        ctx,
        launcher_id,
        None,
        delegated_spend,
        &mut visited,
        0,
    )
}

/// Parses the spend of a vault, and the spends of the vaults it controls. Each vault is parsed with its own copy of the
/// reveals as they were given, since parsing records the settlement payments of the spends it visits. The custody hash
/// is only known for a controlled vault, rather than the vault being signed for.
#[allow(clippy::too_many_arguments)]
fn parse_vault(
    mut reveals: Reveals,
    base_reveals: &Reveals,
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    custody_hash: Option<Bytes32>,
    delegated_spend: Spend,
    visited: &mut HashSet<Bytes32>,
    depth: usize,
) -> Result<VaultTransaction, DriverError> {
    if depth > 0 {
        reveals.use_controlled_vault_nonces(launcher_id);
    }

    let mut facts = Facts::default();

    let vault_spend = parse_vault_delegated_spend(&mut facts, ctx, delegated_spend)?;

    // Every vault is assumed to have an amount of 1, which is enforced here so that it stays that way. The vault's
    // amount isn't otherwise known, since the coin of the vault being signed for isn't given, and a controlled vault
    // whose message is sent to its puzzle hash could be spent with any of its coins. If the amount could be more than
    // 1, recreating it with 1 would hide that the rest is spent as a fee.
    if vault_spend
        .child
        .is_some_and(|child| child.amount != VAULT_AMOUNT)
    {
        return Err(DriverError::InvalidVaultAmount);
    }

    let mut parsed_spends = HashMap::new();
    let mut parse_errors = HashMap::new();

    for spend in reveals.coin_spends().copied().collect::<Vec<_>>() {
        // Spends which aren't linked to the vault are ignored, so they shouldn't prevent the
        // transaction from being parsed. The error is only returned if the spend is linked.
        let parsed_spend = match parse_spend(&mut reveals, ctx, &spend) {
            Ok(parsed_spend) => parsed_spend,
            Err(error) => {
                parse_errors.insert(spend.coin.coin_id(), error);
                continue;
            }
        };

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

    // Messages to controlled vaults authorize their delegated spends, rather than the spend of a coin.
    let mut messages_by_coin: IndexMap<Bytes32, Vec<VaultMessage>> = IndexMap::new();
    let mut messages_by_controlled_vault: IndexMap<Bytes32, Vec<VaultMessage>> = IndexMap::new();
    let mut has_controller_marker = false;

    for message in vault_spend.messages {
        // A message to the controller message coin can never be received, so it only makes the spend invalid on chain,
        // which a vault that authorizes another vault to sign messages must do. It's only allowed in a controller
        // envelope, which is checked below.
        if message.spent_coin_id == VAULT_CONTROLLER_MESSAGE_COIN_ID {
            has_controller_marker = true;
            continue;
        }

        let messages = if reveals.controlled_vault(message.spent_coin_id).is_some() {
            &mut messages_by_controlled_vault
        } else {
            &mut messages_by_coin
        };

        messages
            .entry(message.spent_coin_id)
            .or_default()
            .push(message);
    }

    // A message to a puzzle hash is sent to a controlled vault whose singleton member is fast-forwardable. It isn't
    // allowed for any other coin, since any coin with the puzzle hash could receive it, which would let a different
    // coin than the one that's revealed be spent. A vault's puzzle hash commits to its launcher id and custody hash, so
    // the controlled vault's delegated spend is authorized the same way on any of its coins.
    for message in vault_spend.puzzle_messages {
        let controlled_vault = reveals
            .controlled_vault_by_puzzle_hash(message.puzzle_hash)?
            .ok_or(DriverError::MissingSpend)?;

        messages_by_controlled_vault
            .entry(controlled_vault.coin.coin_id())
            .or_default()
            .push(VaultMessage {
                spent_coin_id: controlled_vault.coin.coin_id(),
                data: message.data,
            });
    }

    // The time bounds decide which clawback spends can be trusted, so they're recorded up front from the spends
    // that the vault authorizes and that can't be replaced. Any mismatched message fails the transaction below.
    for coin_id in messages_by_coin.keys() {
        let Some(ParsedSpend {
            clawback: None,
            custody: Some(custody),
            ..
        }) = parsed_spends.get(coin_id)
        else {
            continue;
        };

        if !custody.receives_message() {
            continue;
        }

        for condition in custody.conditions() {
            match condition {
                Condition::AssertSecondsAbsolute(condition) => {
                    facts.update_actual_earliest_time(condition.seconds);
                }
                Condition::AssertBeforeSecondsAbsolute(condition) => {
                    facts.update_actual_expiration_time(condition.seconds);
                }
                _ => {}
            }
        }
    }

    let mut verified_spends = Vec::new();
    let mut issuances: Vec<Issuance> = Vec::new();

    // Indices of verified sender path spends whose conditions haven't been recorded, since the expiration time
    // didn't rule out the other paths yet when they were verified.
    let mut untrusted_senders = Vec::new();

    // Children of verified spends that are yet to be visited.
    let mut stack: IndexSet<Bytes32> = IndexSet::new();

    for (coin_id, messages) in messages_by_coin {
        let spend = *reveals
            .coin_spend(coin_id)
            .ok_or(DriverError::MissingSpend)?;
        let Some(parsed_spend) = parsed_spends.remove(&coin_id) else {
            return Err(parse_errors
                .remove(&coin_id)
                .unwrap_or(DriverError::MissingSpend));
        };

        // The sender path could consume the message instead, unless the clawback has expired.
        if let Some(ClawbackInfo {
            clawback,
            path: ClawbackPath::Receiver,
        }) = parsed_spend.clawback
            && !is_expired(&facts, &clawback)
        {
            return Err(DriverError::UnguaranteedClawbackExpiry);
        }

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

        push_verified_spend(
            &facts,
            &mut verified_spends,
            &mut untrusted_senders,
            &mut stack,
            verified_spend,
        );
    }

    // Coins that were visited before anything asserted their spend, and the number of verified spends when they
    // were last revisited.
    let mut unasserted: IndexSet<Bytes32> = IndexSet::new();
    let mut revisited_count = verified_spends.len();

    // Once a clawback has expired, anyone can push it through to the receiver without a signature, so the vault
    // only asserts that the clawback coin is spent. The coin can then only be spent by its receiver (the receiver
    // path or the push-through path), unless it's clawed back by the sender. The sender path asserts that the
    // clawback hasn't expired, so if the vault asserts that it has, the clawback is going to the receiver.
    //
    // The children of a push-through can assert other spends, including other push-throughs, which is why this
    // repeats until nothing else qualifies.
    loop {
        while let Some(coin_id) = stack.pop() {
            if !facts.is_spend_asserted(coin_id) {
                unasserted.insert(coin_id);
                continue;
            }

            // Spends without custody stay parsed, since they may be verifiable push-throughs.
            if parsed_spends
                .get(&coin_id)
                .is_none_or(|spend| spend.custody.is_none())
            {
                continue;
            }

            let Some(parsed_spend) = parsed_spends.remove(&coin_id) else {
                continue;
            };

            // Without a vault message, the receiver path could be replaced with the sender path.
            if let Some(ClawbackInfo {
                clawback,
                path: ClawbackPath::Receiver,
            }) = parsed_spend.clawback
                && !parsed_spend
                    .custody
                    .as_ref()
                    .is_some_and(CustodyInfo::receives_message)
                && !is_expired(&facts, &clawback)
            {
                continue;
            }

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

            push_verified_spend(
                &facts,
                &mut verified_spends,
                &mut untrusted_senders,
                &mut stack,
                verified_spend,
            );
        }

        // Spends verified later in the traversal can lower the expiration time, which can make a sender path spend
        // trusted after the fact, so its conditions are recorded then.
        let (trusted_senders, still_untrusted): (Vec<usize>, Vec<usize>) = untrusted_senders
            .into_iter()
            .partition(|&index| !is_untrusted_sender(&facts, &verified_spends[index]));
        untrusted_senders = still_untrusted;

        for &index in &trusted_senders {
            let spend = &verified_spends[index];
            record_facts(
                &mut facts,
                spend.custody.conditions(),
                spend.clawback,
                spend.custody.receives_message(),
            );
        }

        // A spend can be asserted after the traversal has already visited its coin, so the coins that weren't
        // asserted are revisited whenever something new is verified or trusted.
        if (verified_spends.len() > revisited_count || !trusted_senders.is_empty())
            && !unasserted.is_empty()
        {
            revisited_count = verified_spends.len();
            stack.extend(unasserted.drain(..));
            continue;
        }

        // The revealed coin spends are iterated in order rather than the parsed spends, which are unordered.
        let push_throughs: Vec<(Bytes32, ClawbackV2)> = reveals
            .coin_spends()
            .filter_map(|spend| {
                let coin_id = spend.coin.coin_id();
                match parsed_spends.get(&coin_id)?.clawback {
                    Some(ClawbackInfo {
                        clawback,
                        path: ClawbackPath::PushThrough,
                    }) if spend.coin.amount == clawback.amount
                        && facts.is_spend_asserted(coin_id)
                        && is_expired(&facts, &clawback) =>
                    {
                        Some((coin_id, clawback))
                    }
                    _ => None,
                }
            })
            .collect();

        if push_throughs.is_empty() {
            break;
        }

        for (coin_id, clawback) in push_throughs {
            let mut parsed_spend = parsed_spends
                .remove(&coin_id)
                .ok_or(DriverError::MissingSpend)?;

            let spend = *reveals
                .coin_spend(coin_id)
                .ok_or(DriverError::MissingSpend)?;

            // The conditions are determined by the clawback rather than by the revealed solution, since the
            // clawback's puzzle hash commits to them.
            let push_through_path = ctx.alloc(&clawback.push_through_path())?;
            let conditions =
                parse_delegated_spend(ctx, Spend::new(push_through_path, NodePtr::NIL))?;
            parsed_spend.custody = Some(CustodyInfo::DelegatedConditions(conditions));

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

            push_verified_spend(
                &facts,
                &mut verified_spends,
                &mut untrusted_senders,
                &mut stack,
                verified_spend,
            );
        }
    }

    // A spend that the transaction asserts is linked even without a vault message, so it can't be
    // ignored if it couldn't be parsed. The revealed coin spends are iterated in order, so that the
    // error is deterministic.
    for spend in reveals.coin_spends() {
        let coin_id = spend.coin.coin_id();

        if facts.is_spend_asserted(coin_id)
            && let Some(error) = parse_errors.remove(&coin_id)
        {
            return Err(error);
        }
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
        &vault_spend.drop_coins,
        &issuances,
        reserved_fee,
    );

    let mut controlled_vaults = Vec::new();

    for (coin_id, messages) in messages_by_controlled_vault {
        let controlled_vault = *reveals
            .controlled_vault(coin_id)
            .ok_or(DriverError::MissingSpend)?;

        // A message authorizes whatever the coin's puzzle does with it, so the controlled vault's coin spend must be
        // revealed, and its puzzle must be a vault. Otherwise, an asset that the vault owns (such as an NFT, which is
        // also a singleton) could be revealed as a controlled vault, which would hide that it's being spent.
        let coin_spend = reveals
            .coin_spend(coin_id)
            .ok_or(DriverError::MissingSpend)?;

        if VaultLayers::parse_puzzle(ctx, coin_spend.puzzle)?.is_none() {
            return Err(DriverError::InvalidControlledVaultPuzzle);
        }

        // The controlled vault's custody only receives one message from this vault for each spend.
        let [message] = messages.as_slice() else {
            return Err(DriverError::DuplicateVaultMessage);
        };

        let controlled_delegated_puzzle_hash =
            tree_hash(ctx, controlled_vault.delegated_spend.puzzle).into();

        // A member of the recovery path is passed the delegated puzzle hash wrapped in the recovery path's delegated
        // puzzle wrappers, which only restrict what the delegated puzzle can do.
        let signed_delegated_puzzle_hash = match controlled_vault.recovery {
            Some(recovery) => vault_recovery_delegated_puzzle_hash(
                controlled_delegated_puzzle_hash,
                recovery.left_side_subtree_hash,
                recovery.recovery_timelock,
                recovery.version,
            ),
            None => controlled_delegated_puzzle_hash,
        };

        if message.data.as_ref() != signed_delegated_puzzle_hash.as_ref() {
            return Err(DriverError::WrongControlledVaultMessage);
        }

        if depth >= MAX_CONTROLLED_VAULT_DEPTH {
            return Err(DriverError::ControlledVaultTooDeep);
        }

        // Each vault is only parsed once, so that the same spends aren't shown more than once, and so that cycles
        // can't be parsed forever.
        if !visited.insert(controlled_vault.launcher_id) {
            return Err(DriverError::DuplicateControlledVault);
        }

        let transaction = parse_vault(
            base_reveals.clone(),
            base_reveals,
            ctx,
            controlled_vault.launcher_id,
            Some(controlled_vault.custody_hash),
            controlled_vault.delegated_spend,
            visited,
            depth + 1,
        )?;

        controlled_vaults.push(ControlledVaultTransaction {
            coin: controlled_vault.coin,
            launcher_id: controlled_vault.launcher_id,
            custody_hash: controlled_vault.custody_hash,
            recovery: controlled_vault.recovery,
            transaction,
        });
    }

    // Like a vault that signs messages, a vault that authorizes another vault to sign messages can't do anything else,
    // so that it can't look like it also authorizes a transaction. It must also send its message to the controller
    // message coin, so that its spend can never be valid on chain, even if the controlled vault's custody can receive
    // the message some other way than with a vault message envelope. This is the same check that
    // `verify_controlled_vault_signed_message` does. A message to the controller message coin is only allowed in a
    // controller envelope.
    if has_controller_marker
        || controlled_vaults
            .iter()
            .any(|controlled_vault| controlled_vault.transaction.signs_messages())
    {
        let conditions = parse_delegated_spend(ctx, delegated_spend)?;

        if parse_vault_controller_envelope(ctx, &conditions)?.is_none() {
            return Err(DriverError::InvalidVaultControllerEnvelope);
        }
    }

    // A vault that signs messages, or authorizes another vault to, must be recreated as it was, which is also what
    // `verify_controlled_vault_signed_message` requires. This can only be checked for a controlled vault, whose custody
    // hash is known.
    let signs_messages = !vault_spend.signed_messages.is_empty()
        || controlled_vaults
            .iter()
            .any(|controlled_vault| controlled_vault.transaction.signs_messages());

    if signs_messages
        && let Some(custody_hash) = custody_hash
        && vault_spend.child != Some(VaultOutput::new(custody_hash, VAULT_AMOUNT))
    {
        return Err(DriverError::VaultMessageEnvelopeChangesVault);
    }

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
        signed_messages: vault_spend.signed_messages,
        controlled_vaults,
    })
}

/// Whether the transaction can only be valid once the clawback has expired, which rules out its sender path.
fn is_expired(facts: &Facts, clawback: &ClawbackV2) -> bool {
    facts
        .actual_earliest_time()
        .is_some_and(|earliest| earliest >= clawback.seconds)
}

/// Adds a verified spend, queues its children to be visited, and keeps track of it if it's a sender path spend whose
/// conditions couldn't be recorded yet.
fn push_verified_spend(
    facts: &Facts,
    verified_spends: &mut Vec<VerifiedSpend>,
    untrusted_senders: &mut Vec<usize>,
    stack: &mut IndexSet<Bytes32>,
    verified_spend: VerifiedSpend,
) {
    for child in &verified_spend.children {
        stack.insert(child.asset.coin().coin_id());
    }

    if is_untrusted_sender(facts, &verified_spend) {
        untrusted_senders.push(verified_spends.len());
    }

    verified_spends.push(verified_spend);
}

/// Whether the transaction expires before the clawback, which rules out the paths a sender path spend could be
/// replaced with.
fn is_sender_trusted(facts: &Facts, clawback: &ClawbackV2) -> bool {
    facts
        .actual_expiration_time()
        .is_some_and(|expiration| expiration <= clawback.seconds)
}

/// Whether a verified spend takes the sender path, but its conditions couldn't be recorded yet.
fn is_untrusted_sender(facts: &Facts, spend: &VerifiedSpend) -> bool {
    matches!(
        spend.clawback,
        Some(ClawbackInfo {
            clawback,
            path: ClawbackPath::Sender,
        }) if !is_sender_trusted(facts, &clawback)
    )
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

    let conditions = custody.conditions();

    // The coin id is committed to by the conditions, and therefore the message. Since the coin id
    // depends on the fixed conditions, they can't be made to receive the same message.
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

    let mut issuance = if let Some(run_cat_tail) = run_cat_tail {
        let cat_solution = CatSolution::<NodePtr>::from_clvm(allocator, spend.solution)?;

        Some(Issuance {
            coin_id: spend.coin.coin_id(),
            asset_id: run_cat_tail.asset_id,
            extra_delta: cat_solution.extra_delta,
            kind: run_cat_tail.kind,
            verified: false,
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
        } else if let Some(issuance) = &mut issuance
            && matches!(issuance.kind, IssuanceKind::EverythingWithSingleton { .. })
            && message.data == get_extra_delta_message(issuance.extra_delta)
        {
            if tail_matched {
                return Err(DriverError::DuplicateVaultMessage);
            }

            tail_matched = true;
            issuance.verified = true;
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

    record_facts(
        facts,
        conditions,
        parsed_spend.clawback,
        custody.receives_message(),
    );

    let children = parse_children(reveals, allocator, &parsed_spend.asset, spend, conditions)?;

    // Issuances are either:
    // 1. Invalid, in which case the transaction will atomically fail because this spend is verified
    // 2. Valid, in which case the information is correct
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

/// Records what a verified spend's conditions guarantee about the transaction.
///
/// A vault message is received by a coin rather than a puzzle, so another path of a clawback can consume the message
/// without outputting the conditions it commits to. A clawback spend can therefore only be trusted once the time
/// bounds rule out the paths it could be replaced with. The time bounds can't come from clawback spends themselves.
fn record_facts(
    facts: &mut Facts,
    conditions: &[Condition],
    clawback: Option<ClawbackInfo>,
    is_message_bound: bool,
) {
    let is_trusted = clawback.is_none_or(|info| match info.path {
        ClawbackPath::Sender => is_sender_trusted(facts, &info.clawback),
        ClawbackPath::Receiver => is_message_bound && is_expired(facts, &info.clawback),
        ClawbackPath::PushThrough => false,
    });

    if !is_trusted {
        return;
    }

    for condition in conditions {
        match condition {
            Condition::AssertPuzzleAnnouncement(condition) => {
                facts.assert_puzzle_announcement(condition.announcement_id);
            }
            Condition::AssertConcurrentSpend(condition) => {
                facts.assert_spend(condition.coin_id);
            }
            Condition::AssertBeforeSecondsAbsolute(condition) if clawback.is_none() => {
                facts.update_actual_expiration_time(condition.seconds);
            }
            Condition::ReserveFee(condition) => {
                facts.add_reserved_fees(condition.amount);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct AssetFlowTotals {
    asset: ClearSigningAsset,
    input_amount: u128,
    output_amount: u128,
    issued_amount: u128,
    melted_amount: u128,
    received_amount: u128,
    paid_amount: u128,
}

fn build_asset_flows(
    spends: &[VerifiedSpend],
    received_payments: &[AssertedPayment],
    external_payments: &[AssertedPayment],
    drop_coins: &[DropCoin],
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
                u128::from(spend.asset.coin().amount);
        }

        for child in &spend.children {
            if !spend_by_coin_id.contains_key(&child.asset.coin().coin_id())
                && child.transfer_type != TransferType::Offered
            {
                asset_flow_mut(&mut flows, asset_from_parsed(&child.asset)).output_amount +=
                    u128::from(child.asset.coin().amount);
            }
        }
    }

    for asserted_payment in received_payments {
        asset_flow_mut(&mut flows, asserted_payment.asset).received_amount +=
            u128::from(asserted_payment.payment.amount);
    }

    for asserted_payment in external_payments {
        asset_flow_mut(&mut flows, asserted_payment.asset).paid_amount +=
            u128::from(asserted_payment.payment.amount);
    }

    for drop_coin in drop_coins {
        asset_flow_mut(&mut flows, ClearSigningAsset::Xch).output_amount +=
            u128::from(drop_coin.amount);
    }

    for spend in spends {
        if matches!(spend.asset, ParsedAsset::Nft(_))
            && xch_child_coin_ids.contains(&spend.asset.coin().parent_coin_info)
        {
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount +=
                u128::from(spend.asset.coin().amount);
        }
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
            let amount = u128::from(spend.asset.coin().amount);
            asset_flow_mut(&mut flows, cat_asset).issued_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount += amount;
        }

        // The extra delta comes from the solution, so it can't be relied on unless it was verified.
        if !issuance.verified {
            continue;
        }

        // The extra delta comes from an untrusted solution, so it can be `i64::MIN`.
        let amount = u128::from(issuance.extra_delta.unsigned_abs());

        if issuance.extra_delta > 0 {
            asset_flow_mut(&mut flows, cat_asset).issued_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount += amount;
        } else if issuance.extra_delta < 0 {
            asset_flow_mut(&mut flows, cat_asset).melted_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).issued_amount += amount;
        }
    }

    flows
        .into_values()
        .filter_map(|flow| {
            // Payments and fees may be covered by other parties, so this is floored at zero.
            let unaccounted_amount = (flow.input_amount + flow.issued_amount).saturating_sub(
                flow.output_amount
                    + flow.melted_amount
                    + flow.paid_amount
                    + if matches!(flow.asset, ClearSigningAsset::Xch) {
                        u128::from(reserved_fee)
                    } else {
                        0
                    },
            );

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
