use chia_protocol::{Bytes32, CoinSpend};
use chia_sdk_types::{Mod, puzzles::SingletonMember};
use clvm_utils::tree_hash;
use clvmr::Allocator;

use crate::{
    AssertedRequestedPayment, ClawbackV2, DriverError, DropCoin, Facts, LinkedSpendSummary, Spend,
    VaultOutput, VaultSpendSummary, mips_puzzle_hash, parse_asserted_requested_payments,
    parse_vault_delegated_spend,
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
    pub spends: Vec<LinkedSpendSummary>,
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

impl VaultTransaction {
    pub fn parse(
        allocator: &mut Allocator,
        delegated_spend: Spend,
        coin_spends: Vec<CoinSpend>,
        spent_clawbacks: Vec<ClawbackV2>,
    ) -> Result<Self, DriverError> {
        let mut facts = Facts::default();

        facts.reveal_vault_nonce(0);

        for coin_spend in coin_spends {
            facts.reveal_coin_spend(allocator, &coin_spend)?;
        }

        for clawback in spent_clawbacks {
            facts.reveal_clawback(clawback);
        }

        let summary = parse_vault_delegated_spend(&mut facts, allocator, delegated_spend)?;
        let delegated_puzzle_hash = tree_hash(allocator, delegated_spend.puzzle).into();

        Self::from_vault_spend_summary(&facts, allocator, summary, delegated_puzzle_hash)
    }

    pub fn from_vault_spend_summary(
        facts: &Facts,
        allocator: &Allocator,
        summary: VaultSpendSummary,
        delegated_puzzle_hash: Bytes32,
    ) -> Result<Self, DriverError> {
        let reserved_fee = facts.reserved_fees().try_into()?;

        let mut input_amount = 0;
        let mut output_amount = 0;

        for spend in &summary.linked_spends {
            input_amount += u128::from(spend.asset.coin().amount);

            for child in &spend.children {
                output_amount += u128::from(child.asset.coin().amount);
            }
        }

        let fee_paid = (input_amount - output_amount).try_into()?;
        let received_payments = parse_asserted_requested_payments(facts, allocator)?;
        let launcher_id = find_launcher_id(&summary.linked_spends)?;
        let p2_puzzle_hashes = if let Some(launcher_id) = launcher_id {
            calculate_p2_puzzle_hashes(facts, launcher_id)
        } else {
            Vec::new()
        };

        Ok(Self {
            vault_child: summary.child,
            drop_coins: summary.drop_coins,
            spends: summary.linked_spends,
            received_payments,
            fee_paid,
            reserved_fee,
            launcher_id,
            p2_puzzle_hashes,
            delegated_puzzle_hash,
        })
    }
}

fn find_launcher_id(linked_spends: &[LinkedSpendSummary]) -> Result<Option<Bytes32>, DriverError> {
    let mut launcher_id = None;

    for spend in linked_spends {
        let Some(launcher_id) = launcher_id else {
            launcher_id = Some(spend.p2_singleton.launcher_id);
            continue;
        };

        if launcher_id != spend.p2_singleton.launcher_id {
            return Err(DriverError::ConflictingVaultLauncherIds);
        }
    }

    Ok(launcher_id)
}

fn calculate_p2_puzzle_hashes(facts: &Facts, launcher_id: Bytes32) -> Vec<Bytes32> {
    let mut p2_puzzle_hashes = Vec::new();

    for nonce in facts.vault_nonces() {
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
