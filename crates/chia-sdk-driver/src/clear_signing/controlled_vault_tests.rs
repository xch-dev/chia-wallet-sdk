use anyhow::Result;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::{Memos, singleton::SingletonArgs};
use chia_sdk_test::Simulator;
use chia_sdk_types::{Conditions, conditions::SendMessage};
use clvm_utils::ToTreeHash;
use rstest::rstest;

use crate::{
    Action, ControlledVaultRecovery, DriverError, Id, ParsedAsset, RevealedControlledVault,
    Reveals, Spend, SpendContext, TestVault, TransactionData, VAULT_MESSAGE_MODE, VaultOutput,
    VaultTransaction, VaultVersion, parse_vault_transaction, vault_recovery_delegated_puzzle_hash,
};

/// Reveals each vault below the vault at the top of the chain of controllers, which is the one being signed for.
fn reveal_controlled_vaults(
    ctx: &mut SpendContext,
    vault: &TestVault,
    result: &TransactionData,
) -> Result<Reveals> {
    let mut reveals = Reveals::from_coin_spends(ctx, &result.spend_bundle.coin_spends)?;

    reveals.reveal_controlled_vault(RevealedControlledVault {
        coin: result.vault_spend.coin,
        launcher_id: vault.info.launcher_id,
        custody_hash: vault.info.custody_hash.into(),
        delegated_spend: result.delegated_spend,
        recovery: None,
    })?;

    // Every controller other than the one at the top is also controlled.
    for controller_spend in result
        .controller_spends
        .iter()
        .take(result.controller_spends.len() - 1)
    {
        reveals.reveal_controlled_vault(RevealedControlledVault {
            coin: controller_spend.vault.coin,
            launcher_id: controller_spend.vault.info.launcher_id,
            custody_hash: controller_spend.vault.info.custody_hash.into(),
            delegated_spend: controller_spend.delegated_spend,
            recovery: None,
        })?;
    }

    Ok(reveals)
}

/// Parses the transaction from the point of view of the vault at the top of the chain of controllers.
fn parse_from_top(
    ctx: &mut SpendContext,
    result: &TransactionData,
    reveals: Reveals,
) -> Result<VaultTransaction, DriverError> {
    let top = result.controller_spends.last().unwrap();
    parse_vault_transaction(
        reveals,
        ctx,
        top.vault.info.launcher_id,
        top.delegated_spend,
    )
}

#[test]
fn test_clear_signing_controlled_vault_transfer() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1100, &controller)?;
    let bob_puzzle_hash: Bytes32 = "bob".tree_hash().into();

    let result = vault.spend(
        &mut sim,
        &mut ctx,
        &[
            Action::send(Id::Xch, bob_puzzle_hash, 1000, Memos::None),
            Action::fee(100),
        ],
    )?;
    assert_eq!(result.controller_spends.len(), 1);

    let reveals = reveal_controlled_vaults(&mut ctx, &vault, &result)?;
    let tx = parse_from_top(&mut ctx, &result, reveals)?;

    // The controller only recreates itself, and authorizes the controlled vault's spend.
    assert_eq!(
        tx.vault_child,
        Some(VaultOutput::new(controller.info.custody_hash.into(), 1))
    );
    assert!(tx.spends.is_empty());
    assert!(tx.asset_flows.is_empty());
    assert_eq!(tx.reserved_fee, 0);
    assert!(!tx.signs_messages());
    assert_eq!(tx.controlled_vaults.len(), 1);

    let controlled_vault = &tx.controlled_vaults[0];
    assert_eq!(controlled_vault.launcher_id, vault.info.launcher_id);
    assert_eq!(
        controlled_vault.custody_hash,
        vault.info.custody_hash.into()
    );
    assert_eq!(controlled_vault.coin, result.vault_spend.coin);
    assert_eq!(controlled_vault.recovery, None);

    // The controlled vault's spends are parsed with its own p2 puzzle hash.
    let controlled_tx = &controlled_vault.transaction;
    assert_eq!(
        controlled_tx.vault_child,
        Some(VaultOutput::new(vault.info.custody_hash.into(), 1))
    );
    assert_eq!(controlled_tx.p2_puzzle_hashes, vec![vault.p2_puzzle_hash]);
    assert_eq!(controlled_tx.reserved_fee, 100);
    assert_eq!(controlled_tx.spends.len(), 1);
    assert!(matches!(controlled_tx.spends[0].asset, ParsedAsset::Xch(_)));

    let children = &controlled_tx.spends[0].children;
    assert!(
        children
            .iter()
            .any(|child| child.memos.p2_puzzle_hash == bob_puzzle_hash
                && child.asset.coin().amount == 1000)
    );

    Ok(())
}

#[test]
fn test_clear_signing_controlled_vault_requires_reveal() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &controller)?;

    let result = vault.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Xch,
            "bob".tree_hash().into(),
            1000,
            Memos::None,
        )],
    )?;

    // Without the controlled vault's reveal, its coin spend can't be verified as a spend that the message authorizes.
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    assert!(matches!(
        parse_from_top(&mut ctx, &result, reveals),
        Err(DriverError::InvalidLinkedCustody)
    ));

    Ok(())
}

#[test]
fn test_clear_signing_chained_controlled_vaults() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let top = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let middle = TestVault::mint_controlled(&mut sim, &mut ctx, 0, &top)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &middle)?;
    let bob_puzzle_hash: Bytes32 = "bob".tree_hash().into();

    let result = vault.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, bob_puzzle_hash, 1000, Memos::None)],
    )?;
    assert_eq!(result.controller_spends.len(), 2);

    let reveals = reveal_controlled_vaults(&mut ctx, &vault, &result)?;
    let tx = parse_from_top(&mut ctx, &result, reveals)?;

    assert!(tx.spends.is_empty());
    assert_eq!(tx.controlled_vaults.len(), 1);

    let middle_tx = &tx.controlled_vaults[0];
    assert_eq!(middle_tx.launcher_id, middle.info.launcher_id);
    assert!(middle_tx.transaction.spends.is_empty());
    assert_eq!(middle_tx.transaction.controlled_vaults.len(), 1);

    let vault_tx = &middle_tx.transaction.controlled_vaults[0];
    assert_eq!(vault_tx.launcher_id, vault.info.launcher_id);
    assert_eq!(vault_tx.transaction.spends.len(), 1);
    assert!(vault_tx.transaction.controlled_vaults.is_empty());

    Ok(())
}

#[test]
fn test_clear_signing_wrong_controlled_vault_reveal() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &controller)?;

    let result = vault.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Xch,
            "bob".tree_hash().into(),
            1000,
            Memos::None,
        )],
    )?;

    // The custody hash must match the controlled vault's coin.
    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    assert!(matches!(
        reveals.reveal_controlled_vault(RevealedControlledVault {
            coin: result.vault_spend.coin,
            launcher_id: vault.info.launcher_id,
            custody_hash: Bytes32::default(),
            delegated_spend: result.delegated_spend,
            recovery: None,
        }),
        Err(DriverError::WrongControlledVaultPuzzleHash)
    ));

    // The delegated spend must be the one that the message authorizes.
    let other_delegated_spend = ctx.delegated_spend(Conditions::new().create_coin(
        vault.info.custody_hash.into(),
        1,
        Memos::None,
    ))?;
    reveals.reveal_controlled_vault(RevealedControlledVault {
        coin: result.vault_spend.coin,
        launcher_id: vault.info.launcher_id,
        custody_hash: vault.info.custody_hash.into(),
        delegated_spend: other_delegated_spend,
        recovery: None,
    })?;
    assert!(matches!(
        parse_from_top(&mut ctx, &result, reveals),
        Err(DriverError::WrongControlledVaultMessage)
    ));

    Ok(())
}

#[test]
fn test_clear_signing_controlled_vault_nonces() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &controller)?;

    let result = vault.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Xch,
            "bob".tree_hash().into(),
            1000,
            Memos::None,
        )],
    )?;

    let mut reveals = reveal_controlled_vaults(&mut ctx, &vault, &result)?;
    reveals.reveal_controlled_vault_nonce(vault.info.launcher_id, 1);
    let tx = parse_from_top(&mut ctx, &result, reveals)?;

    // The nonce only applies to the controlled vault.
    assert_eq!(tx.p2_puzzle_hashes.len(), 1);
    assert_eq!(
        tx.controlled_vaults[0].transaction.p2_puzzle_hashes.len(),
        2
    );

    Ok(())
}

/// A vault that's only used to build delegated spends, without being on chain.
#[derive(Debug, Clone, Copy)]
struct UnmintedVault {
    launcher_id: Bytes32,
    custody_hash: Bytes32,
    coin: Coin,
}

impl UnmintedVault {
    fn new(seed: u8) -> Self {
        let launcher_id = Bytes32::new([seed; 32]);
        let custody_hash = Bytes32::new([seed.wrapping_add(100); 32]);
        let puzzle_hash = SingletonArgs::curry_tree_hash(launcher_id, custody_hash.into()).into();

        Self {
            launcher_id,
            custody_hash,
            coin: Coin::new(Bytes32::new([seed.wrapping_add(200); 32]), puzzle_hash, 1),
        }
    }

    fn recreate(self) -> Conditions {
        Conditions::new().create_coin(self.custody_hash, 1, Memos::None)
    }

    fn reveal(
        self,
        delegated_spend: Spend,
        recovery: Option<ControlledVaultRecovery>,
    ) -> RevealedControlledVault {
        RevealedControlledVault {
            coin: self.coin,
            launcher_id: self.launcher_id,
            custody_hash: self.custody_hash,
            delegated_spend,
            recovery,
        }
    }
}

/// A message from a vault to the vault it controls.
fn controller_message(
    ctx: &mut SpendContext,
    controlled: UnmintedVault,
    delegated_puzzle_hash: Bytes32,
) -> Result<SendMessage<clvmr::NodePtr>> {
    let coin_id = ctx.alloc(&controlled.coin.coin_id())?;
    Ok(SendMessage::new(
        VAULT_MESSAGE_MODE,
        delegated_puzzle_hash.to_vec().into(),
        vec![coin_id],
    ))
}

#[rstest]
fn test_clear_signing_controlled_vault_recovery(
    #[values(VaultVersion::Legacy, VaultVersion::ForceSingletonRecreation)] version: VaultVersion,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(1);
    let vault = UnmintedVault::new(2);

    let recovered_custody_hash = Bytes32::new([3; 32]);
    let delegated_spend =
        ctx.delegated_spend(Conditions::new().create_coin(recovered_custody_hash, 1, Memos::None))?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    let recovery = ControlledVaultRecovery::new(Bytes32::new([4; 32]), 1000, version);
    let signed_delegated_puzzle_hash = vault_recovery_delegated_puzzle_hash(
        delegated_puzzle_hash,
        recovery.left_side_subtree_hash,
        recovery.recovery_timelock,
        recovery.version,
    );

    let message = controller_message(&mut ctx, vault, signed_delegated_puzzle_hash)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message))?;

    // The controller signs the delegated puzzle hash wrapped in the recovery path's wrappers.
    let mut reveals = Reveals::default();
    reveals.reveal_controlled_vault(vault.reveal(delegated_spend, Some(recovery)))?;
    let tx = parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend)?;

    let controlled = &tx.controlled_vaults[0];
    assert_eq!(controlled.recovery, Some(recovery));
    assert_eq!(
        controlled.transaction.vault_child,
        Some(VaultOutput::new(recovered_custody_hash, 1))
    );

    // Without the recovery path, the message doesn't match the delegated puzzle hash.
    let mut reveals = Reveals::default();
    reveals.reveal_controlled_vault(vault.reveal(delegated_spend, None))?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::WrongControlledVaultMessage)
    ));

    // And a recovery path with different details doesn't match either.
    let mut reveals = Reveals::default();
    reveals.reveal_controlled_vault(vault.reveal(
        delegated_spend,
        Some(ControlledVaultRecovery::new(
            recovery.left_side_subtree_hash,
            recovery.recovery_timelock + 1,
            version,
        )),
    ))?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::WrongControlledVaultMessage)
    ));

    Ok(())
}

#[test]
fn test_clear_signing_duplicate_controlled_vault_message() -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(1);
    let vault = UnmintedVault::new(2);

    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    let first = controller_message(&mut ctx, vault, delegated_puzzle_hash)?;
    let second = controller_message(&mut ctx, vault, delegated_puzzle_hash)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(first).with(second))?;

    let mut reveals = Reveals::default();
    reveals.reveal_controlled_vault(vault.reveal(delegated_spend, None))?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::DuplicateVaultMessage)
    ));

    Ok(())
}

#[test]
fn test_clear_signing_controlled_vault_cycle() -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(1);
    let vault = UnmintedVault::new(2);

    // The controlled vault claims to control the vault being signed for.
    let controller_delegated_spend = ctx.delegated_spend(controller.recreate())?;
    let controller_delegated_puzzle_hash: Bytes32 =
        ctx.tree_hash(controller_delegated_spend.puzzle).into();
    let back = controller_message(&mut ctx, controller, controller_delegated_puzzle_hash)?;
    let delegated_spend = ctx.delegated_spend(vault.recreate().with(back))?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message))?;

    let mut reveals = Reveals::default();
    reveals.reveal_controlled_vault(vault.reveal(delegated_spend, None))?;
    reveals.reveal_controlled_vault(controller.reveal(controller_delegated_spend, None))?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::DuplicateControlledVault)
    ));

    Ok(())
}

/// Builds a chain of vaults, each of which controls the next, and returns the delegated spend of the first.
fn build_chain(
    ctx: &mut SpendContext,
    vaults: &[UnmintedVault],
    reveals: &mut Reveals,
) -> Result<Spend> {
    let last = vaults.last().unwrap();
    let mut delegated_spend = ctx.delegated_spend(last.recreate())?;

    for pair in vaults.windows(2).rev() {
        let [controller, controlled_vault] = pair else {
            unreachable!();
        };

        reveals.reveal_controlled_vault(controlled_vault.reveal(delegated_spend, None))?;

        let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();
        let message = controller_message(ctx, *controlled_vault, delegated_puzzle_hash)?;
        delegated_spend = ctx.delegated_spend(controller.recreate().with(message))?;
    }

    Ok(delegated_spend)
}

#[rstest]
#[case(crate::MAX_CONTROLLED_VAULT_DEPTH, true)]
#[case(crate::MAX_CONTROLLED_VAULT_DEPTH + 1, false)]
fn test_clear_signing_controlled_vault_depth(
    #[case] depth: usize,
    #[case] is_valid: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let vaults: Vec<UnmintedVault> = (0..=depth)
        .map(|i| UnmintedVault::new(u8::try_from(i).unwrap() + 1))
        .collect();

    let mut reveals = Reveals::default();
    let delegated_spend = build_chain(&mut ctx, &vaults, &mut reveals)?;

    let result = parse_vault_transaction(reveals, &mut ctx, vaults[0].launcher_id, delegated_spend);

    if is_valid {
        let mut tx = result?;

        for vault in &vaults[1..] {
            assert_eq!(tx.controlled_vaults.len(), 1);
            let controlled = tx.controlled_vaults.remove(0);
            assert_eq!(controlled.launcher_id, vault.launcher_id);
            tx = controlled.transaction;
        }

        assert!(tx.controlled_vaults.is_empty());
    } else {
        assert!(matches!(result, Err(DriverError::ControlledVaultTooDeep)));
    }

    Ok(())
}

#[test]
fn test_clear_signing_controlled_vault_signs_messages() -> Result<()> {
    let mut ctx = SpendContext::new();

    let top = UnmintedVault::new(1);
    let middle = UnmintedVault::new(2);
    let vault = UnmintedVault::new(3);
    let messages: Vec<Bytes> = vec![b"hello".to_vec().into(), b"world".to_vec().into()];

    // The controlled vault signs messages, which the vaults above it authorize with controller envelopes.
    let mut envelope = vault.recreate();
    let message_coin_id = ctx.alloc(&crate::VAULT_MESSAGE_COIN_ID)?;
    for message in &messages {
        envelope = envelope.with(SendMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![message_coin_id],
        ));
    }
    let envelope = ctx.delegated_spend(envelope)?;
    let envelope_hash: Bytes32 = ctx.tree_hash(envelope.puzzle).into();

    let middle_message = controller_message(&mut ctx, vault, envelope_hash)?;
    let middle_spend = ctx.delegated_spend(middle.recreate().with(middle_message))?;
    let middle_hash: Bytes32 = ctx.tree_hash(middle_spend.puzzle).into();

    let mut reveals = Reveals::default();
    reveals.reveal_controlled_vault(vault.reveal(envelope, None))?;
    reveals.reveal_controlled_vault(middle.reveal(middle_spend, None))?;

    let top_message = controller_message(&mut ctx, middle, middle_hash)?;
    let top_spend = ctx.delegated_spend(top.recreate().with(top_message))?;

    let tx = parse_vault_transaction(reveals.clone(), &mut ctx, top.launcher_id, top_spend)?;
    assert!(tx.signs_messages());
    assert!(tx.signed_messages.is_empty());
    let middle_tx = &tx.controlled_vaults[0].transaction;
    assert!(middle_tx.signs_messages());
    assert_eq!(
        middle_tx.controlled_vaults[0].transaction.signed_messages,
        messages
    );

    // A vault that authorizes a vault to sign messages can't do anything else.
    let top_message = controller_message(&mut ctx, middle, middle_hash)?;
    let mixed_spend = ctx.delegated_spend(top.recreate().with(top_message).create_coin(
        Bytes32::default(),
        0,
        Memos::None,
    ))?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, top.launcher_id, mixed_spend),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    Ok(())
}
