use anyhow::Result;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{Memos, singleton::SingletonArgs};
use chia_sdk_test::Simulator;
use chia_sdk_types::{Conditions, conditions::SendMessage};
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;
use rstest::rstest;

use crate::{
    Action, ControlledVaultRecovery, DelegatedPuzzleFeederLayer, DriverError, Id,
    IndexWrapperLayer, Layer, ParsedAsset, Puzzle, RevealedControlledVault, Reveals, SingletonInfo,
    SingletonLayer, Spend, SpendContext, TestVault, TransactionData,
    VAULT_CONTROLLER_MESSAGE_COIN_ID, VAULT_FAST_FORWARD_MESSAGE_MODE, VAULT_MESSAGE_MODE,
    VaultOutput, VaultTransaction, VaultVersion, parse_vault_transaction,
    vault_recovery_delegated_puzzle_hash,
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

#[rstest]
fn test_clear_signing_controlled_vault_transfer(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1100, &controller, fast_forward)?;
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

#[rstest]
fn test_clear_signing_controlled_vault_requires_reveal(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &controller, fast_forward)?;

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

    // Without the controlled vault's reveal, its coin spend can't be verified as a spend that the message authorizes. A
    // message to its puzzle hash can't be matched to a coin spend at all.
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let result = parse_from_top(&mut ctx, &result, reveals);

    if fast_forward {
        assert!(matches!(result, Err(DriverError::MissingSpend)));
    } else {
        assert!(matches!(result, Err(DriverError::InvalidLinkedCustody)));
    }

    Ok(())
}

#[rstest]
fn test_clear_signing_chained_controlled_vaults(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let top = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let middle = TestVault::mint_controlled(&mut sim, &mut ctx, 0, &top, fast_forward)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &middle, fast_forward)?;
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

#[rstest]
fn test_clear_signing_wrong_controlled_vault_reveal(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &controller, fast_forward)?;

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

#[rstest]
fn test_clear_signing_controlled_vault_nonces(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 1000, &controller, fast_forward)?;

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

/// A vault that's only used to build delegated spends, without being on chain. Its puzzle is a singleton whose inner
/// puzzle is a top-level MIPS puzzle, so that it can be revealed as a controlled vault.
#[derive(Debug, Clone, Copy)]
struct UnmintedVault {
    launcher_id: Bytes32,
    custody_hash: Bytes32,
    coin: Coin,
    puzzle: NodePtr,
}

impl UnmintedVault {
    fn new(ctx: &mut SpendContext, seed: u8) -> Result<Self> {
        let launcher_id = Bytes32::new([seed; 32]);

        // The member is never run, so it only has to give each vault a different custody hash.
        let member = ctx.alloc(&Bytes32::new([seed.wrapping_add(100); 32]))?;
        let custody = IndexWrapperLayer::new(
            0,
            DelegatedPuzzleFeederLayer::new(Puzzle::parse(ctx, member)),
        )
        .construct_puzzle(ctx)?;
        let custody_hash: Bytes32 = ctx.tree_hash(custody).into();

        let puzzle =
            SingletonLayer::new(launcher_id, Puzzle::parse(ctx, custody)).construct_puzzle(ctx)?;
        let puzzle_hash = SingletonArgs::curry_tree_hash(launcher_id, custody_hash.into()).into();

        Ok(Self {
            launcher_id,
            custody_hash,
            coin: Coin::new(Bytes32::new([seed.wrapping_add(200); 32]), puzzle_hash, 1),
            puzzle,
        })
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

    /// Reveals the vault's coin spend, and the vault as a controlled vault.
    fn reveal_to(
        self,
        ctx: &mut SpendContext,
        reveals: &mut Reveals,
        delegated_spend: Spend,
        recovery: Option<ControlledVaultRecovery>,
    ) -> Result<()> {
        let puzzle_reveal = ctx.serialize(&self.puzzle)?;
        let solution = ctx.serialize(&NodePtr::NIL)?;
        reveals.reveal_coin_spend(ctx, &CoinSpend::new(self.coin, puzzle_reveal, solution))?;
        reveals.reveal_controlled_vault(self.reveal(delegated_spend, recovery))?;
        Ok(())
    }
}

/// A message from a vault to the vault it controls, which is sent to its puzzle hash if its singleton member is
/// fast-forwardable.
fn controller_message(
    ctx: &mut SpendContext,
    controlled: UnmintedVault,
    delegated_puzzle_hash: Bytes32,
    fast_forward: bool,
) -> Result<SendMessage<clvmr::NodePtr>> {
    let (mode, receiver) = if fast_forward {
        (VAULT_FAST_FORWARD_MESSAGE_MODE, controlled.coin.puzzle_hash)
    } else {
        (VAULT_MESSAGE_MODE, controlled.coin.coin_id())
    };
    let receiver = ctx.alloc(&receiver)?;
    Ok(SendMessage::new(
        mode,
        delegated_puzzle_hash.to_vec().into(),
        vec![receiver],
    ))
}

/// The message that a vault which authorizes another vault to sign messages also sends to the controller message coin,
/// so that its spend can never be valid on chain.
fn controller_marker(
    ctx: &mut SpendContext,
    delegated_puzzle_hash: Bytes32,
) -> Result<SendMessage<clvmr::NodePtr>> {
    let coin_id = ctx.alloc(&VAULT_CONTROLLER_MESSAGE_COIN_ID)?;
    Ok(SendMessage::new(
        VAULT_MESSAGE_MODE,
        delegated_puzzle_hash.to_vec().into(),
        vec![coin_id],
    ))
}

#[rstest]
fn test_clear_signing_controlled_vault_recovery(
    #[values(VaultVersion::Legacy, VaultVersion::ForceSingletonRecreation)] version: VaultVersion,
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

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

    let message = controller_message(&mut ctx, vault, signed_delegated_puzzle_hash, fast_forward)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message))?;

    // The controller signs the delegated puzzle hash wrapped in the recovery path's wrappers.
    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, Some(recovery))?;
    let tx = parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend)?;

    let controlled = &tx.controlled_vaults[0];
    assert_eq!(controlled.recovery, Some(recovery));
    assert_eq!(
        controlled.transaction.vault_child,
        Some(VaultOutput::new(recovered_custody_hash, 1))
    );

    // Without the recovery path, the message doesn't match the delegated puzzle hash.
    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::WrongControlledVaultMessage)
    ));

    // And a recovery path with different details doesn't match either.
    let mut reveals = Reveals::default();
    vault.reveal_to(
        &mut ctx,
        &mut reveals,
        delegated_spend,
        Some(ControlledVaultRecovery::new(
            recovery.left_side_subtree_hash,
            recovery.recovery_timelock + 1,
            version,
        )),
    )?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::WrongControlledVaultMessage)
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_duplicate_controlled_vault_message(
    #[values(false, true)] first_fast_forward: bool,
    #[values(false, true)] second_fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    let first = controller_message(&mut ctx, vault, delegated_puzzle_hash, first_fast_forward)?;
    let second = controller_message(&mut ctx, vault, delegated_puzzle_hash, second_fast_forward)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(first).with(second))?;

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::DuplicateVaultMessage)
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_controlled_vault_cycle(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

    // The controlled vault claims to control the vault being signed for.
    let controller_delegated_spend = ctx.delegated_spend(controller.recreate())?;
    let controller_delegated_puzzle_hash: Bytes32 =
        ctx.tree_hash(controller_delegated_spend.puzzle).into();
    let back = controller_message(
        &mut ctx,
        controller,
        controller_delegated_puzzle_hash,
        fast_forward,
    )?;
    let delegated_spend = ctx.delegated_spend(vault.recreate().with(back))?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash, fast_forward)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message))?;

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    controller.reveal_to(&mut ctx, &mut reveals, controller_delegated_spend, None)?;
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

    for (i, pair) in vaults.windows(2).enumerate().rev() {
        let [controller, controlled_vault] = pair else {
            unreachable!();
        };

        controlled_vault.reveal_to(ctx, reveals, delegated_spend, None)?;

        // Every other vault in the chain has a fast-forwardable singleton member.
        let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();
        let message =
            controller_message(ctx, *controlled_vault, delegated_puzzle_hash, i % 2 == 1)?;
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
        .map(|i| UnmintedVault::new(&mut ctx, u8::try_from(i).unwrap() + 1))
        .collect::<Result<_>>()?;

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

#[rstest]
fn test_clear_signing_controlled_vault_signs_messages(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let top = UnmintedVault::new(&mut ctx, 1)?;
    let middle = UnmintedVault::new(&mut ctx, 2)?;
    let vault = UnmintedVault::new(&mut ctx, 3)?;
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

    let middle_message = controller_message(&mut ctx, vault, envelope_hash, fast_forward)?;
    let middle_marker = controller_marker(&mut ctx, envelope_hash)?;
    let middle_spend =
        ctx.delegated_spend(middle.recreate().with(middle_message).with(middle_marker))?;
    let middle_hash: Bytes32 = ctx.tree_hash(middle_spend.puzzle).into();

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, envelope, None)?;
    middle.reveal_to(&mut ctx, &mut reveals, middle_spend, None)?;

    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let top_marker = controller_marker(&mut ctx, middle_hash)?;
    let top_spend = ctx.delegated_spend(top.recreate().with(top_message).with(top_marker))?;

    let tx = parse_vault_transaction(reveals.clone(), &mut ctx, top.launcher_id, top_spend)?;
    assert!(tx.signs_messages());
    assert!(tx.signed_messages.is_empty());
    let middle_tx = &tx.controlled_vaults[0].transaction;
    assert!(middle_tx.signs_messages());
    assert_eq!(
        middle_tx.controlled_vaults[0].transaction.signed_messages,
        messages
    );

    // A vault that authorizes a vault to sign messages must make its spend invalid on chain, since it could otherwise
    // be valid if the vault's custody can receive the message some other way.
    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let unmarked_spend = ctx.delegated_spend(top.recreate().with(top_message))?;
    assert!(matches!(
        parse_vault_transaction(reveals.clone(), &mut ctx, top.launcher_id, unmarked_spend),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    // The marker must have the same message.
    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let wrong_marker = controller_marker(&mut ctx, envelope_hash)?;
    let wrong_spend = ctx.delegated_spend(top.recreate().with(top_message).with(wrong_marker))?;
    assert!(matches!(
        parse_vault_transaction(reveals.clone(), &mut ctx, top.launcher_id, wrong_spend),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    // And it can't do anything else.
    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let top_marker = controller_marker(&mut ctx, middle_hash)?;
    let mixed_spend = ctx.delegated_spend(
        top.recreate()
            .with(top_message)
            .with(top_marker)
            .create_coin(Bytes32::default(), 0, Memos::None),
    )?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, top.launcher_id, mixed_spend),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_fast_forwarded_controlled_vault(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash, fast_forward)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message))?;

    // The controlled vault's spend is fast-forwarded onto a newer coin, which has the same puzzle hash.
    let newer = UnmintedVault {
        coin: Coin::new(
            vault.coin.coin_id(),
            vault.coin.puzzle_hash,
            vault.coin.amount,
        ),
        ..vault
    };

    let mut reveals = Reveals::default();
    newer.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    let result =
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend);

    // A message to the coin id can only be received by the original coin, which isn't revealed.
    if fast_forward {
        assert_eq!(result?.controlled_vaults[0].coin, newer.coin);
    } else {
        assert!(matches!(result, Err(DriverError::MissingSpend)));
    }

    Ok(())
}

#[test]
fn test_clear_signing_ambiguous_fast_forward_controlled_vault() -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash, true)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message))?;

    // Either coin could receive the message, so it's unclear which one is spent.
    let newer = UnmintedVault {
        coin: Coin::new(
            vault.coin.coin_id(),
            vault.coin.puzzle_hash,
            vault.coin.amount,
        ),
        ..vault
    };

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    newer.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::DuplicateControlledVault)
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_puzzle_message_to_coin(#[values(false, true)] revealed: bool) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Xch,
            "bob".tree_hash().into(),
            1000,
            Memos::None,
        )],
    )?;

    // Any coin with the puzzle hash could receive the message, so it can only be sent to a controlled vault, even if
    // the coin spend that would receive it is revealed.
    let p2_puzzle_hash = ctx.alloc(&alice.p2_puzzle_hash)?;
    let delegated_spend = ctx.delegated_spend(
        Conditions::new()
            .create_coin(alice.info.custody_hash.into(), 1, Memos::None)
            .with(SendMessage::new(
                VAULT_FAST_FORWARD_MESSAGE_MODE,
                Bytes::default(),
                vec![p2_puzzle_hash],
            )),
    )?;

    let reveals = if revealed {
        Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?
    } else {
        Reveals::default()
    };
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, alice.info.launcher_id, delegated_spend),
        Err(DriverError::MissingSpend)
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_controlled_vault_amount(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

    // The controlled vault's coin must have an amount of 1.
    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let mut reveals = Reveals::default();
    assert!(matches!(
        reveals.reveal_controlled_vault(RevealedControlledVault {
            coin: Coin::new(vault.coin.parent_coin_info, vault.coin.puzzle_hash, 3),
            ..vault.reveal(delegated_spend, None)
        }),
        Err(DriverError::InvalidVaultAmount)
    ));

    // And it must be recreated with an amount of 1, since a larger coin could be spent instead if the message is sent
    // to its puzzle hash, and the rest would be spent as a fee.
    let delegated_spend =
        ctx.delegated_spend(Conditions::new().create_coin(vault.custody_hash, 3, Memos::None))?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();
    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash, fast_forward)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message))?;

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::InvalidVaultAmount)
    ));

    // The same applies to the vault being signed for.
    let controller_spend = ctx.delegated_spend(Conditions::new().create_coin(
        controller.custody_hash,
        3,
        Memos::None,
    ))?;
    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            controller.launcher_id,
            controller_spend
        ),
        Err(DriverError::InvalidVaultAmount)
    ));

    Ok(())
}

#[test]
fn test_clear_signing_nft_revealed_as_controlled_vault() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let result = alice.spend(&mut sim, &mut ctx, &[Action::mint_empty_nft()])?;
    let nft = *result.outputs.nfts.values().next().unwrap();

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Existing(nft.info.launcher_id),
            "bob".tree_hash().into(),
            1,
            Memos::None,
        )],
    )?;

    // The NFT is also a singleton, so it could be revealed as a controlled vault, which would hide that the vault is
    // sending it. Its puzzle isn't a vault, so it's rejected.
    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_controlled_vault(RevealedControlledVault {
        coin: nft.coin,
        launcher_id: nft.info.launcher_id,
        custody_hash: nft.info.inner_puzzle_hash().into(),
        delegated_spend: result.delegated_spend,
        recovery: None,
    })?;
    assert!(matches!(
        parse_vault_transaction(
            reveals,
            &mut ctx,
            alice.info.launcher_id,
            result.delegated_spend
        ),
        Err(DriverError::InvalidControlledVaultPuzzle)
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_controller_of_message_signer_changes_custody(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let top = UnmintedVault::new(&mut ctx, 1)?;
    let middle = UnmintedVault::new(&mut ctx, 2)?;
    let vault = UnmintedVault::new(&mut ctx, 3)?;

    let mut envelope = vault.recreate();
    let message_coin_id = ctx.alloc(&crate::VAULT_MESSAGE_COIN_ID)?;
    envelope = envelope.with(SendMessage::new(
        VAULT_MESSAGE_MODE,
        b"hello".to_vec().into(),
        vec![message_coin_id],
    ));
    let envelope = ctx.delegated_spend(envelope)?;
    let envelope_hash: Bytes32 = ctx.tree_hash(envelope.puzzle).into();

    // The middle vault authorizes the vault to sign messages, but changes its own custody. Its custody hash is known,
    // since it's controlled by the top vault, so this is rejected.
    let middle_message = controller_message(&mut ctx, vault, envelope_hash, fast_forward)?;
    let middle_marker = controller_marker(&mut ctx, envelope_hash)?;
    let middle_spend = ctx.delegated_spend(
        Conditions::new()
            .create_coin(Bytes32::new([9; 32]), 1, Memos::None)
            .with(middle_message)
            .with(middle_marker),
    )?;
    let middle_hash: Bytes32 = ctx.tree_hash(middle_spend.puzzle).into();

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, envelope, None)?;
    middle.reveal_to(&mut ctx, &mut reveals, middle_spend, None)?;

    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let top_marker = controller_marker(&mut ctx, middle_hash)?;
    let top_spend = ctx.delegated_spend(top.recreate().with(top_message).with(top_marker))?;

    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, top.launcher_id, top_spend),
        Err(DriverError::VaultMessageEnvelopeChangesVault)
    ));

    Ok(())
}
