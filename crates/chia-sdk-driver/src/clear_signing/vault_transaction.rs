use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_protocol::{Bytes, Bytes32, CoinSpend};
use chia_sdk_types::{
    Mod,
    puzzles::{
        AddDelegatedPuzzleWrapper, Force1of2RestrictedVariable, PreventConditionOpcode,
        PreventMultipleCreateCoinsMod, Timelock,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{
    AssertedRequestedPayment, ClawbackV2, DriverError, DropCoin, Facts, LinkedSpendSummary, Spend,
    VaultOutput, VaultSpendSummary, parse_asserted_requested_payments, parse_vault_delegated_spend,
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
    pub spends: Vec<LinkedSpendSummary>,
    pub received_payments: Vec<AssertedRequestedPayment>,
    /// Total fees (different between input and output amounts) paid by coin spends authorized by the vault.
    /// If the transaction is signed, the fee is guaranteed to be at least this amount, unless it's not reserved.
    /// The reason to include unreserved fees is to make it clear that the XCH is leaving the vault due to this transaction.
    pub fee_paid: u64,
    /// The amount of fees reserved by coin spends authorized by the vault.
    /// If this is greater than or equal to the fee paid, you can be sure that the XCH spent for fees will not be
    /// maliciously redirected for some other purpose by the submitter of the transaction after signing.
    pub reserved_fee: u64,
}

impl VaultTransaction {
    pub fn parse(
        allocator: &mut Allocator,
        delegated_spend: Spend,
        coin_spends: Vec<CoinSpend>,
        spent_clawbacks: Vec<ClawbackV2>,
    ) -> Result<Self, DriverError> {
        let mut facts = Facts::default();

        for coin_spend in coin_spends {
            facts.reveal_coin_spend(allocator, &coin_spend)?;
        }

        for clawback in spent_clawbacks {
            facts.reveal_clawback(clawback);
        }

        let summary = parse_vault_delegated_spend(&mut facts, allocator, delegated_spend)?;

        Self::from_vault_spend_summary(&facts, allocator, summary)
    }

    pub fn from_vault_spend_summary(
        facts: &Facts,
        allocator: &Allocator,
        summary: VaultSpendSummary,
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

        Ok(Self {
            vault_child: summary.child,
            drop_coins: summary.drop_coins,
            spends: summary.linked_spends,
            received_payments,
            fee_paid,
            reserved_fee,
        })
    }
}

pub fn calculate_vault_puzzle_message(
    delegated_puzzle_hash: Bytes32,
    vault_puzzle_hash: Bytes32,
) -> Bytes {
    [
        delegated_puzzle_hash.to_bytes(),
        vault_puzzle_hash.to_bytes(),
    ]
    .concat()
    .into()
}

pub fn calculate_vault_coin_message(
    delegated_puzzle_hash: Bytes32,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Bytes {
    [
        delegated_puzzle_hash.to_bytes(),
        vault_coin_id.to_bytes(),
        genesis_challenge.to_bytes(),
    ]
    .concat()
    .into()
}

pub fn calculate_vault_start_recovery_message(
    delegated_puzzle_hash: Bytes32,
    left_side_subtree_hash: Bytes32,
    recovery_timelock: u64,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Bytes {
    let mut delegated_puzzle_hash: TreeHash = delegated_puzzle_hash.into();

    let restrictions = vec![
        Force1of2RestrictedVariable::new(
            left_side_subtree_hash,
            0,
            vec![Timelock::new(recovery_timelock).curry_tree_hash()]
                .tree_hash()
                .into(),
            ().tree_hash().into(),
        )
        .curry_tree_hash(),
        PreventConditionOpcode::new(CREATE_COIN_ANNOUNCEMENT).curry_tree_hash(),
        PreventConditionOpcode::new(CREATE_PUZZLE_ANNOUNCEMENT).curry_tree_hash(),
        PreventConditionOpcode::new(SEND_MESSAGE).curry_tree_hash(),
        PreventConditionOpcode::new(RECEIVE_MESSAGE).curry_tree_hash(),
        PreventMultipleCreateCoinsMod::mod_hash(),
    ];

    for restriction in restrictions.into_iter().rev() {
        delegated_puzzle_hash =
            AddDelegatedPuzzleWrapper::new(restriction, delegated_puzzle_hash).curry_tree_hash();
    }

    [
        delegated_puzzle_hash.to_bytes(),
        vault_coin_id.to_bytes(),
        genesis_challenge.to_bytes(),
    ]
    .concat()
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use chia_bls::verify;
    use chia_puzzle_types::Memos;
    use chia_puzzles::{SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH};
    use chia_sdk_test::Simulator;
    use chia_sdk_types::{Conditions, TESTNET11_CONSTANTS};
    use rstest::rstest;

    use crate::{
        Action, FeeAction, Id, ParsedAsset, RequestedAsset, SpendContext, Spends, TestVault,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AssetKind {
        Xch,
        Cat,
        RevocableCat,
    }

    struct Asset {
        id: Id,
        asset_id: Option<Bytes32>,
        hidden_puzzle_hash: Option<Bytes32>,
    }

    fn issue_asset(
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        alice: &TestVault,
        asset_kind: AssetKind,
    ) -> Result<Asset> {
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

        Ok(Asset {
            id,
            asset_id,
            hidden_puzzle_hash,
        })
    }

    #[rstest]
    fn test_clear_signing_sent(
        #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
        #[values(0, 100)] fee: u64,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000 + fee)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

        let Asset {
            id,
            asset_id,
            hidden_puzzle_hash,
        } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(id, bob.puzzle_hash(), 1000, Memos::None),
                Action::fee(fee),
            ],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

        assert_eq!(
            tx.vault_child,
            Some(VaultOutput::new(alice.custody_hash().into(), 1))
        );

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

        if let Some(asset_id) = asset_id {
            let ParsedAsset::Cat(cat) = child.asset else {
                panic!("Expected CAT child");
            };
            assert_eq!(cat.info.asset_id, asset_id);
            assert_eq!(cat.info.hidden_puzzle_hash, hidden_puzzle_hash);
        } else {
            assert!(matches!(child.asset, ParsedAsset::Xch(_)));
        }

        assert_eq!(child.memos.p2_puzzle_hash, bob.puzzle_hash());
        assert_eq!(child.asset.coin().amount, 1000);

        Ok(())
    }

    #[rstest]
    fn test_clear_signing_received(
        #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
        #[values(true, false)] disable_settlement_assertions: bool,
        #[values(0, 100)] alice_fee: u64,
        #[values(0, 100)] bob_fee: u64,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000 + alice_fee)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, bob_fee)?;

        let Asset {
            id,
            asset_id,
            hidden_puzzle_hash,
        } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(id, SETTLEMENT_PAYMENT_HASH.into(), 1000, Memos::None),
                Action::fee(alice_fee),
            ],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

        assert_eq!(tx.fee_paid, alice_fee);
        assert_eq!(tx.reserved_fee, alice_fee);
        assert_eq!(tx.spends.len(), 1);

        let spend = &tx.spends[0];
        assert_eq!(spend.children.len(), 1);

        let child = &spend.children[0];
        assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());
        assert_eq!(child.asset.coin().amount, 1000);

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

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

        if disable_settlement_assertions {
            assert_eq!(tx.received_payments.len(), 0);
            assert_eq!(tx.fee_paid, bob_fee);
            assert_eq!(tx.reserved_fee, bob_fee);
        } else {
            assert_eq!(tx.received_payments.len(), 1);
            assert_eq!(tx.fee_paid, bob_fee);
            assert_eq!(tx.reserved_fee, bob_fee);

            let payment = &tx.received_payments[0];
            if let Some(asset_id) = asset_id {
                assert_eq!(
                    payment.asset,
                    RequestedAsset::Cat {
                        asset_id,
                        hidden_puzzle_hash
                    }
                );
            } else {
                assert_eq!(payment.asset, RequestedAsset::Xch);
            }
            assert_eq!(
                payment.notarized_payment.payments[0].puzzle_hash,
                bob.puzzle_hash()
            );
            assert_eq!(payment.notarized_payment.payments[0].amount, 1000);
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

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

        assert_eq!(tx.payments.len(), 1);
        assert_eq!(tx.nfts.len(), 1);
        assert_eq!(tx.fee_paid, 0);
        assert_eq!(tx.reserved_fee, 0);

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
        assert_eq!(nft.old_state.parsed_metadata, Some(NftMetadata::default()));
        assert_eq!(
            nft.old_state.metadata_updater_puzzle_hash,
            Bytes32::default()
        );
        assert_eq!(nft.old_state.owner, None);
        assert_eq!(nft.new_state.parsed_metadata, Some(NftMetadata::default()));
        assert_eq!(
            nft.new_state.metadata_updater_puzzle_hash,
            Bytes32::default()
        );
        assert_eq!(nft.new_state.owner, None);
        assert_eq!(nft.royalty_puzzle_hash, Bytes32::default());
        assert_eq!(nft.royalty_basis_points, 0);

        // Transfer the NFT to Bob
        let nft_id = Id::Existing(nft.launcher_id);
        let bob_hint = ctx.hint(bob.puzzle_hash())?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[Action::send(nft_id, bob.puzzle_hash(), 1, bob_hint)],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

        assert_eq!(tx.payments.len(), 0);
        assert_eq!(tx.nfts.len(), 1);
        assert_eq!(tx.fee_paid, 0);
        assert_eq!(tx.reserved_fee, 0);

        let nft = &tx.nfts[0];
        assert_eq!(nft.transfer_type, TransferType::Sent);
        assert_eq!(nft.p2_puzzle_hash, bob.puzzle_hash());
        assert!(!nft.includes_unverifiable_updates);

        Ok(())
    }

    #[rstest]
    fn test_clear_signing_split(
        #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
        #[values(0, 100)] fee: u64,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000 + fee)?;

        let Asset {
            id,
            asset_id,
            hidden_puzzle_hash,
        } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(id, alice.puzzle_hash(), 250, Memos::None),
                Action::send(id, alice.puzzle_hash(), 250, Memos::None),
                Action::send(id, alice.puzzle_hash(), 250, Memos::None),
                Action::send(id, alice.puzzle_hash(), 250, Memos::None),
                Action::fee(fee),
            ],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;
        assert_eq!(tx.new_custody_hash, Some(alice.custody_hash()));
        assert_eq!(tx.payments.len(), 4);
        assert_eq!(tx.fee_paid, fee);
        assert_eq!(tx.reserved_fee, fee);

        for payment in &tx.payments {
            assert_eq!(payment.transfer_type, TransferType::Updated);
            assert_eq!(payment.asset_id, asset_id);
            assert_eq!(payment.p2_puzzle_hash, alice.puzzle_hash());
            assert_eq!(payment.coin.amount, 250);
        }

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[Action::send(id, alice.puzzle_hash(), 1000, Memos::None)],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;
        assert_eq!(tx.new_custody_hash, Some(alice.custody_hash()));
        assert_eq!(tx.payments.len(), 1);
        assert_eq!(tx.fee_paid, 0);
        assert_eq!(tx.reserved_fee, 0);

        let payment = &tx.payments[0];
        assert_eq!(payment.transfer_type, TransferType::Updated);
        assert_eq!(payment.asset_id, asset_id);
        assert_eq!(payment.p2_puzzle_hash, alice.puzzle_hash());
        assert_eq!(payment.coin.amount, 1000);

        Ok(())
    }

    #[rstest]
    fn test_clear_signing_unreserved_fee() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1100)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[
                Action::send(Id::Xch, bob.puzzle_hash(), 1000, Memos::None),
                Action::Fee(FeeAction {
                    amount: 100,
                    reserved: false,
                }),
            ],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;
        assert_eq!(tx.new_custody_hash, Some(alice.custody_hash()));
        assert_eq!(tx.payments.len(), 1);
        assert_eq!(tx.fee_paid, 100);
        assert_eq!(tx.reserved_fee, 0);

        let payment = &tx.payments[0];
        assert_eq!(payment.transfer_type, TransferType::Sent);
        assert_eq!(payment.asset_id, None);
        assert_eq!(payment.p2_puzzle_hash, bob.puzzle_hash());
        assert_eq!(payment.coin.amount, 1000);

        Ok(())
    }

    #[rstest]
    fn test_clear_signing_coin_message() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
        let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

        let vault = alice.fetch_vault(&sim)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[Action::send(Id::Xch, bob.puzzle_hash(), 1000, Memos::None)],
        )?;

        let tx =
            VaultTransaction::parse(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;
        assert_eq!(tx.new_custody_hash, Some(alice.custody_hash()));
        assert_eq!(tx.payments.len(), 1);

        let coin_message = calculate_vault_coin_message(
            tx.delegated_puzzle_hash,
            vault.coin.coin_id(),
            TESTNET11_CONSTANTS.genesis_challenge,
        );

        assert!(verify(&result.signature, &alice.public_key, coin_message));

        Ok(())
    }
}
