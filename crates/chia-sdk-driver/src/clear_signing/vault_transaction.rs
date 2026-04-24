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
    /// The launcher id of the vault, based on the spends authorized by the delegated spend.
    pub launcher_id: Bytes32,
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
        let p2_puzzle_hashes = calculate_p2_puzzle_hashes(facts, launcher_id);

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

fn find_launcher_id(linked_spends: &[LinkedSpendSummary]) -> Result<Bytes32, DriverError> {
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

    let Some(launcher_id) = launcher_id else {
        return Err(DriverError::MissingLinkedSpends);
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use chia_protocol::Bytes32;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use clvm_utils::ToTreeHash;
    use rstest::rstest;

    use crate::{Action, Id, ParsedAsset, ParsedChild, SpendContext, TestVault};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AssetKind {
        Xch,
        Cat,
        RevocableCat,
    }

    struct IssuedAsset {
        id: Id,
        asset_id: Option<Bytes32>,
        hidden_puzzle_hash: Option<Bytes32>,
    }

    fn issue_asset(
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        alice: &TestVault,
        asset_kind: AssetKind,
    ) -> Result<IssuedAsset> {
        let hidden_puzzle_hash = if matches!(asset_kind, AssetKind::RevocableCat) {
            Some(Bytes32::default())
        } else {
            None
        };

        let (id, asset_id) = if let AssetKind::Cat | AssetKind::RevocableCat = asset_kind {
            let result = alice.spend(
                sim,
                ctx,
                &[Action::single_issue_cat(hidden_puzzle_hash, 1000)],
            )?;

            let asset_id = result.outputs.cats[0][0].info.asset_id;
            let id = Id::Existing(asset_id);
            (id, Some(asset_id))
        } else {
            (Id::Xch, None)
        };

        Ok(IssuedAsset {
            id,
            asset_id,
            hidden_puzzle_hash,
        })
    }

    fn check_child_asset(
        child: &ParsedChild,
        asset_id: Option<Bytes32>,
        hidden_puzzle_hash: Option<Bytes32>,
    ) {
        if let Some(asset_id) = asset_id {
            let ParsedAsset::Cat(cat) = child.asset else {
                panic!("Expected CAT child");
            };
            assert_eq!(cat.info.asset_id, asset_id);
            assert_eq!(cat.info.hidden_puzzle_hash, hidden_puzzle_hash);
        } else {
            assert!(matches!(child.asset, ParsedAsset::Xch(_)));
        }
    }

    #[rstest]
    fn test_clear_signing_vault_child() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

        let result = alice.spend(&mut sim, &mut ctx, &[])?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

        assert_eq!(
            tx.vault_child,
            Some(VaultOutput::new(alice.custody_hash().into(), 1))
        );

        Ok(())
    }

    #[rstest]
    fn test_clear_signing_transfer(
        #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
        #[values(0, 100)] fee: u64,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000 + fee)?;
        let bob_puzzle_hash = "bob".tree_hash().into();

        let IssuedAsset {
            id,
            asset_id,
            hidden_puzzle_hash,
        } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(id, bob_puzzle_hash, 1000, Memos::None),
                Action::fee(fee),
            ],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

        assert_eq!(tx.fee_paid, fee);
        assert_eq!(tx.reserved_fee, fee);
        assert_eq!(
            tx.spends.len(),
            if matches!(asset_kind, AssetKind::Xch) || fee == 0 {
                1
            } else {
                2
            }
        );

        let spend = &tx.spends.last().unwrap();
        assert_eq!(spend.children.len(), 1);

        let child = &spend.children[0];

        check_child_asset(child, asset_id, hidden_puzzle_hash);
        assert_eq!(child.memos.p2_puzzle_hash, bob_puzzle_hash);
        assert_eq!(child.asset.coin().amount, 1000);

        Ok(())
    }
}
