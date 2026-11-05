use anyhow::Result;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{Memos, singleton::SingletonArgs};
use chia_sdk_test::Simulator;
use chia_sdk_types::{Conditions, Mod, conditions::SendMessage, puzzles::SingletonMember};
use clvm_traits::{ToClvm, clvm_quote};
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;
use rstest::rstest;

use crate::{
    Action, ControlledVaultRecovery, DelegatedPuzzleFeederLayer, Deltas, DriverError, Id,
    IndexWrapperLayer, Layer, ParsedAsset, Puzzle, RevealedControlledVault, Reveals, SingletonInfo,
    SingletonLayer, Spend, SpendContext, Spends, TestVault, TransactionData,
    VAULT_CONTROLLER_MESSAGE_COIN_ID, VAULT_FAST_FORWARD_MESSAGE_MODE, VAULT_MESSAGE_MODE,
    VaultOutput, VaultTransaction, VaultVersion, mips_puzzle_hash, parse_vault_transaction,
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
        Some(VaultOutput::new(vault.info.custody_hash.into(), 1))
    );
    assert!(!tx.signs_messages());

    // The controller only recreates itself, and authorizes the controlled vault's spend.
    let [authorization] = tx.authorization_path.as_slice() else {
        panic!("expected one authorization");
    };
    assert_eq!(
        authorization.controller_child,
        VaultOutput::new(controller.info.custody_hash.into(), 1)
    );
    assert_eq!(authorization.launcher_id, vault.info.launcher_id);
    assert_eq!(authorization.custody_hash, vault.info.custody_hash.into());
    assert_eq!(authorization.coin, result.vault_spend.coin);
    assert_eq!(authorization.recovery, None);

    // The transaction is the controlled vault's, parsed with its own p2 puzzle hash. The delegated puzzle hash is still
    // the one that's signed by the controller.
    assert_eq!(tx.launcher_id, vault.info.launcher_id);
    assert_eq!(tx.p2_puzzle_hashes, vec![vault.p2_puzzle_hash]);
    assert_eq!(
        tx.delegated_puzzle_hash,
        ctx.tree_hash(result.controller_spends[0].delegated_spend.puzzle)
            .into()
    );
    assert_eq!(tx.reserved_fee, 100);
    assert_eq!(tx.spends.len(), 1);
    assert!(matches!(tx.spends[0].asset, ParsedAsset::Xch(_)));

    let children = &tx.spends[0].children;
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

    let launcher_ids: Vec<Bytes32> = tx
        .authorization_path
        .iter()
        .map(|authorization| authorization.launcher_id)
        .collect();
    assert_eq!(
        launcher_ids,
        vec![middle.info.launcher_id, vault.info.launcher_id]
    );
    assert_eq!(
        tx.authorization_path[0].controller_child,
        VaultOutput::new(top.info.custody_hash.into(), 1)
    );
    assert_eq!(
        tx.authorization_path[1].controller_child,
        VaultOutput::new(middle.info.custody_hash.into(), 1)
    );
    assert_eq!(tx.spends.len(), 1);

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
    reveals.reveal_vault_nonce(1);
    let tx = parse_from_top(&mut ctx, &result, reveals)?;

    // The nonces are the controlled vault's, since it's the vault whose transaction it is.
    assert_eq!(
        tx.p2_puzzle_hashes,
        vec![
            vault.p2_puzzle_hash,
            mips_puzzle_hash(
                1,
                vec![],
                SingletonMember::new(vault.info.launcher_id).curry_tree_hash(),
                true
            )
            .into()
        ]
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

    assert_eq!(tx.authorization_path[0].recovery, Some(recovery));
    assert_eq!(
        tx.vault_child,
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

    // A controller can only send one message, so the controlled vault's custody can't receive it twice.
    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::InvalidVaultControllerSpend)
    ));

    Ok(())
}

/// Something other than the controller's recreation and its message to the controlled vault.
#[derive(Debug, Clone, Copy)]
enum ControllerExtra {
    /// The controller isn't recreated.
    Melt,
    /// A fee.
    Fee,
    /// Another controlled vault.
    SecondControlledVault,
}

#[rstest]
fn test_clear_signing_controller_does_more_than_authorize(
    #[values(
        ControllerExtra::Melt,
        ControllerExtra::Fee,
        ControllerExtra::SecondControlledVault
    )]
    extra: ControllerExtra,
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;
    let other_vault = UnmintedVault::new(&mut ctx, 3)?;

    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();
    let other_delegated_spend = ctx.delegated_spend(other_vault.recreate())?;
    let other_delegated_puzzle_hash: Bytes32 = ctx.tree_hash(other_delegated_spend.puzzle).into();

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;

    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash, fast_forward)?;
    let mut conditions = Conditions::new().with(message);

    match extra {
        ControllerExtra::Melt => {}
        ControllerExtra::Fee => {
            conditions = conditions.extend(controller.recreate()).reserve_fee(100);
        }
        ControllerExtra::SecondControlledVault => {
            other_vault.reveal_to(&mut ctx, &mut reveals, other_delegated_spend, None)?;
            let other_message = controller_message(
                &mut ctx,
                other_vault,
                other_delegated_puzzle_hash,
                fast_forward,
            )?;
            conditions = conditions.extend(controller.recreate()).with(other_message);
        }
    }

    let controller_spend = ctx.delegated_spend(conditions)?;

    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend),
        Err(DriverError::InvalidVaultControllerSpend)
    ));

    Ok(())
}

/// A controller that spends its own coins while authorizing a controlled vault is valid on chain, but clear signing
/// only allows a controller to authorize the controlled vault.
#[rstest]
fn test_clear_signing_controller_spends_its_own_coins(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let controller = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let vault = TestVault::mint_controlled(&mut sim, &mut ctx, 0, &controller, fast_forward)?;

    // The controlled vault only recreates itself. Its controller's spend is built below instead.
    let vault_coin = vault.fetch_vault(&sim)?;
    let delegated_spend = ctx.delegated_spend(Conditions::new().create_coin(
        vault.info.custody_hash.into(),
        1,
        Memos::None,
    ))?;
    let (vault_spend, _) = vault.spend_vault(&sim, &mut ctx, &vault_coin, delegated_spend)?;

    let (mode, receiver) = if fast_forward {
        (
            VAULT_FAST_FORWARD_MESSAGE_MODE,
            vault_spend.coin.puzzle_hash,
        )
    } else {
        (VAULT_MESSAGE_MODE, vault_spend.coin.coin_id())
    };
    let receiver = ctx.alloc(&receiver)?;
    let authorization = SendMessage::new(
        mode,
        ctx.tree_hash(delegated_spend.puzzle).to_vec().into(),
        vec![receiver],
    );

    // The controller pays Bob from its own p2 coin in the same spend that authorizes the controlled vault.
    let actions = [Action::send(
        Id::Xch,
        "bob".tree_hash().into(),
        1000,
        Memos::None,
    )];
    let mut spends = Spends::new(controller.p2_puzzle_hash);
    controller.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let result = controller.partial_custom_spend(
        &mut sim,
        &mut ctx,
        &actions,
        spends,
        Conditions::new().with(authorization),
    )?;

    let mut coin_spends = result.spend_bundle.coin_spends.clone();
    coin_spends.push(vault_spend.clone());
    sim.new_transaction(SpendBundle::new(coin_spends.clone(), result.signature))?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &coin_spends)?;
    reveals.reveal_controlled_vault(RevealedControlledVault {
        coin: vault_spend.coin,
        launcher_id: vault.info.launcher_id,
        custody_hash: vault.info.custody_hash.into(),
        delegated_spend,
        recovery: None,
    })?;

    assert!(matches!(
        parse_vault_transaction(
            reveals,
            &mut ctx,
            controller.info.launcher_id,
            result.delegated_spend
        ),
        Err(DriverError::InvalidVaultControllerSpend)
    ));

    Ok(())
}

#[test]
fn test_clear_signing_vault_without_controlled_vaults() -> Result<()> {
    let mut ctx = SpendContext::new();

    let vault = UnmintedVault::new(&mut ctx, 1)?;
    let delegated_spend = ctx.delegated_spend(vault.recreate())?;

    // A vault that doesn't authorize a controlled vault is the vault whose transaction it is.
    let tx = parse_vault_transaction(
        Reveals::default(),
        &mut ctx,
        vault.launcher_id,
        delegated_spend,
    )?;
    assert_eq!(tx.launcher_id, vault.launcher_id);
    assert!(tx.authorization_path.is_empty());
    assert_eq!(
        tx.delegated_puzzle_hash,
        ctx.tree_hash(delegated_spend.puzzle).into()
    );

    Ok(())
}

#[rstest]
fn test_clear_signing_top_controller_changes_custody(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    // The custody hash of the vault being signed for isn't known, so it can change its custody while authorizing the
    // controlled vault. It's shown in the authorization path, like the vault child of an ordinary vault spend.
    let new_custody_hash = Bytes32::new([9; 32]);
    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash, fast_forward)?;
    let controller_spend = ctx.delegated_spend(
        Conditions::new()
            .create_coin(new_custody_hash, 1, Memos::None)
            .with(message),
    )?;

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    let tx = parse_vault_transaction(reveals, &mut ctx, controller.launcher_id, controller_spend)?;

    assert_eq!(tx.launcher_id, vault.launcher_id);
    assert_eq!(
        tx.authorization_path[0].controller_child,
        VaultOutput::new(new_custody_hash, 1)
    );

    Ok(())
}

#[test]
fn test_clear_signing_controller_envelope_to_unrevealed_coin() -> Result<()> {
    let mut ctx = SpendContext::new();

    let controller = UnmintedVault::new(&mut ctx, 1)?;
    let vault = UnmintedVault::new(&mut ctx, 2)?;

    // An envelope must authorize a revealed controlled vault, rather than any other coin.
    let delegated_puzzle_hash = Bytes32::new([7; 32]);
    let message = controller_message(&mut ctx, vault, delegated_puzzle_hash, false)?;
    let marker = controller_marker(&mut ctx, delegated_puzzle_hash)?;
    let controller_spend = ctx.delegated_spend(controller.recreate().with(message).with(marker))?;

    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            controller.launcher_id,
            controller_spend
        ),
        Err(DriverError::MissingSpend)
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_middle_controller_changes_custody(
    #[values(false, true)] fast_forward: bool,
) -> Result<()> {
    let mut ctx = SpendContext::new();

    let top = UnmintedVault::new(&mut ctx, 1)?;
    let middle = UnmintedVault::new(&mut ctx, 2)?;
    let vault = UnmintedVault::new(&mut ctx, 3)?;

    let delegated_spend = ctx.delegated_spend(vault.recreate())?;
    let delegated_puzzle_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

    // The middle vault authorizes the vault's spend, but changes its own custody. Its custody hash is known, since it's
    // controlled by the top vault, so this is rejected.
    let middle_message = controller_message(&mut ctx, vault, delegated_puzzle_hash, fast_forward)?;
    let middle_spend = ctx.delegated_spend(
        Conditions::new()
            .create_coin(Bytes32::new([9; 32]), 1, Memos::None)
            .with(middle_message),
    )?;
    let middle_hash: Bytes32 = ctx.tree_hash(middle_spend.puzzle).into();

    let mut reveals = Reveals::default();
    vault.reveal_to(&mut ctx, &mut reveals, delegated_spend, None)?;
    middle.reveal_to(&mut ctx, &mut reveals, middle_spend, None)?;

    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let top_spend = ctx.delegated_spend(top.recreate().with(top_message))?;

    assert!(matches!(
        parse_vault_transaction(reveals, &mut ctx, top.launcher_id, top_spend),
        Err(DriverError::VaultControllerChangesVault)
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
        let tx = result?;

        let launcher_ids: Vec<Bytes32> = tx
            .authorization_path
            .iter()
            .map(|authorization| authorization.launcher_id)
            .collect();
        let expected: Vec<Bytes32> = vaults[1..].iter().map(|vault| vault.launcher_id).collect();
        assert_eq!(launcher_ids, expected);
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
    assert_eq!(tx.signed_messages, messages);
    assert_eq!(tx.authorization_path.len(), 2);

    // A vault that authorizes a vault to sign messages must make its spend invalid on chain, since it could otherwise
    // be valid if the vault's custody can receive the message some other way.
    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let unmarked_spend = ctx.delegated_spend(top.recreate().with(top_message))?;
    assert!(matches!(
        parse_vault_transaction(reveals.clone(), &mut ctx, top.launcher_id, unmarked_spend),
        Err(DriverError::InvalidVaultControllerEnvelope)
    ));

    // The marker must have the same message.
    let top_message = controller_message(&mut ctx, middle, middle_hash, fast_forward)?;
    let wrong_marker = controller_marker(&mut ctx, envelope_hash)?;
    let wrong_spend = ctx.delegated_spend(top.recreate().with(top_message).with(wrong_marker))?;
    assert!(matches!(
        parse_vault_transaction(reveals.clone(), &mut ctx, top.launcher_id, wrong_spend),
        Err(DriverError::InvalidVaultControllerEnvelope)
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
        Err(DriverError::InvalidVaultControllerEnvelope)
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
        assert_eq!(result?.authorization_path[0].coin, newer.coin);
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

#[rstest]
fn test_clear_signing_nft_revealed_as_controlled_vault(
    #[values(false, true)] reveal_coin_spend: bool,
) -> Result<()> {
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

    // The delegated spend of the NFT's p2 puzzle, whose hash is the vault's message to the NFT.
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;
    let nft_spend = tx
        .spends
        .iter()
        .find(|spend| matches!(spend.asset, ParsedAsset::Nft(_)))
        .unwrap();
    let nft_puzzle = clvm_quote!(nft_spend.custody.conditions()).to_clvm(&mut ctx)?;
    let nft_delegated_spend = Spend::new(nft_puzzle, NodePtr::NIL);

    // The NFT is also a singleton, so it could be revealed as a controlled vault with that delegated spend, which would
    // hide that the vault is sending it. Its puzzle isn't a vault, so it's rejected, and its coin spend can't be left
    // out to skip that check.
    let coin_spends: Vec<CoinSpend> = result
        .spend_bundle
        .coin_spends
        .iter()
        .filter(|coin_spend| reveal_coin_spend || coin_spend.coin != nft.coin)
        .cloned()
        .collect();
    let mut reveals = Reveals::from_coin_spends(&mut ctx, &coin_spends)?;
    reveals.reveal_controlled_vault(RevealedControlledVault {
        coin: nft.coin,
        launcher_id: nft.info.launcher_id,
        custody_hash: nft.info.inner_puzzle_hash().into(),
        delegated_spend: nft_delegated_spend,
        recovery: None,
    })?;
    let result = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    );

    if reveal_coin_spend {
        assert!(matches!(
            result,
            Err(DriverError::InvalidControlledVaultPuzzle)
        ));
    } else {
        assert!(matches!(result, Err(DriverError::MissingSpend)));
    }

    Ok(())
}

#[test]
fn test_clear_signing_controller_marker_outside_controller_envelope() -> Result<()> {
    let mut ctx = SpendContext::new();

    let vault = UnmintedVault::new(&mut ctx, 1)?;

    // A message to the controller message coin is only allowed in a controller envelope.
    let marker = controller_marker(&mut ctx, Bytes32::default())?;
    let delegated_spend = ctx.delegated_spend(vault.recreate().with(marker))?;
    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            vault.launcher_id,
            delegated_spend
        ),
        Err(DriverError::InvalidVaultControllerEnvelope)
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
        Err(DriverError::VaultControllerChangesVault)
    ));

    Ok(())
}
