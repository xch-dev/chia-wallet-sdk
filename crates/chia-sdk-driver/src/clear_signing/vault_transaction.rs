use std::collections::HashMap;

use chia_protocol::{Bytes32, CoinSpend};
use chia_sdk_types::{Mod, puzzles::SingletonMember};
use clvm_traits::{ToClvm, clvm_quote};
use clvm_utils::tree_hash;
use clvmr::Allocator;
use indexmap::IndexSet;

use crate::{
    AssertedRequestedPayment, ClawbackInfo, ClawbackV2, CustodyInfo, DriverError, DropCoin, Facts,
    P2SingletonInfo, ParsedAsset, ParsedChild, Reveals, Spend, VaultOutput, mips_puzzle_hash,
    parse_asserted_requested_payments, parse_children, parse_spend, parse_vault_delegated_spend,
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

    let mut verified_spends = Vec::new();

    for message in vault_spend.messages {
        let Some(parsed_spend) = parsed_spends.remove(&message.spent_coin_id) else {
            return Err(DriverError::MissingSpend);
        };

        let Some(spend) = reveals.coin_spend(message.spent_coin_id) else {
            return Err(DriverError::MissingSpend);
        };

        let Some(custody) = parsed_spend.custody else {
            return Err(DriverError::InvalidLinkedCustody);
        };

        let CustodyInfo::P2Singleton(P2SingletonInfo { conditions, .. }) = &custody else {
            return Err(DriverError::InvalidLinkedCustody);
        };

        let delegated_puzzle = clvm_quote!(conditions).to_clvm(allocator)?;
        let delegated_puzzle_hash = tree_hash(allocator, delegated_puzzle);

        if delegated_puzzle_hash != message.delegated_puzzle_hash.into() {
            return Err(DriverError::WrongConditions);
        }

        if let Some(time) = parsed_spend.required_expiration_time {
            facts.update_required_expiration_time(time);
        }

        let children = parse_children(
            &mut facts,
            allocator,
            &parsed_spend.asset,
            spend,
            conditions,
            parsed_spend.required_expiration_time.is_some(),
        )?;

        verified_spends.push(VerifiedSpend {
            asset: parsed_spend.asset,
            clawback: parsed_spend.clawback,
            custody,
            children,
        });
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
            return Err(DriverError::MissingSpend);
        };

        let Some(spend) = reveals.coin_spend(coin_id) else {
            return Err(DriverError::MissingSpend);
        };

        let Some(custody) = parsed_spend.custody else {
            return Err(DriverError::InvalidLinkedCustody);
        };

        let CustodyInfo::DelegatedConditions(conditions) = &custody else {
            return Err(DriverError::InvalidLinkedCustody);
        };

        if let Some(time) = parsed_spend.required_expiration_time {
            facts.update_required_expiration_time(time);
        }

        let children = parse_children(
            &mut facts,
            allocator,
            &parsed_spend.asset,
            spend,
            conditions,
            parsed_spend.required_expiration_time.is_some(),
        )?;

        verified_spends.push(VerifiedSpend {
            asset: parsed_spend.asset,
            clawback: parsed_spend.clawback,
            custody,
            children,
        });
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
        received_payments,
        fee_paid,
        reserved_fee,
        launcher_id,
        p2_puzzle_hashes,
        delegated_puzzle_hash,
    })
}

fn find_launcher_id(spends: &[VerifiedSpend]) -> Result<Option<Bytes32>, DriverError> {
    let mut launcher_id = None;

    for spend in spends {
        let CustodyInfo::P2Singleton(P2SingletonInfo {
            launcher_id: spend_launcher_id,
            ..
        }) = &spend.custody
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
