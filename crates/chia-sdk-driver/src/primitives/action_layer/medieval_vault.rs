use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{
    EveProof, LineageProof, Memos, Proof,
    singleton::{LauncherSolution, SingletonArgs, SingletonSolution, SingletonStruct},
};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    Condition, Conditions,
    puzzles::{P2MOfNDelegateDirectArgs, P2MOfNDelegateDirectSolution, StateSchedulerLayerArgs},
};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr, serde::node_from_bytes};

use crate::{
    DriverError, Layer, MOfNLayer, Puzzle, Singleton, SingletonInfo, SingletonLayer, Spend,
    SpendContext,
};

use super::{MedievalVaultHint, MedievalVaultInfo};

/// An m-of-n BLS multisig singleton.
///
/// Any `m` of the vault's public keys can sign a delegated puzzle, which the vault runs to produce
/// its conditions. With [`MedievalVault::delegated_puzzle_for_flexible_send_message`] it can send
/// messages to other singletons, so it can act as the authorizing vault of an action-layer app
/// (for example, as a registry's price singleton).
///
/// Signatures use `AGG_SIG_UNSAFE` over the delegated puzzle hash, so a delegated puzzle must make
/// itself non-replayable. [`MedievalVault::spend`] and the `delegated_puzzle_for_*` helpers do this
/// with [`MedievalVault::delegated_conditions`].
///
/// # Example
///
/// Launch a 1-of-2 vault, then rekey it to 2-of-2:
///
/// ```
/// use chia_puzzles::SINGLETON_LAUNCHER_HASH;
/// use chia_sdk_driver::{Launcher, MedievalVault, MedievalVaultHint, SpendContext};
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::{TESTNET11_CONSTANTS, puzzles::P2MOfNDelegateDirectArgs};
///
/// let mut sim = Simulator::new();
/// let ctx = &mut SpendContext::new();
///
/// let alice = sim.bls(0);
/// let bob = sim.bls(0);
/// let keys = vec![alice.pk, bob.pk];
///
/// // Create and spend a launcher.
/// let launcher_coin = sim.new_coin(SINGLETON_LAUNCHER_HASH.into(), 1);
/// let launcher = Launcher::new(launcher_coin.parent_coin_info, 1);
/// let launcher_id = launcher.coin().coin_id();
/// launcher.spend(
///     ctx,
///     P2MOfNDelegateDirectArgs::curry_tree_hash(1, keys.clone()).into(),
///     MedievalVaultHint {
///         my_launcher_id: launcher_id,
///         m: 1,
///         public_key_list: keys.clone(),
///     },
/// )?;
/// let launcher_spend = ctx.take().remove(0);
/// sim.spend_coins(vec![launcher_spend.clone()], &[])?;
///
/// let vault = MedievalVault::from_parent_spend(ctx, &launcher_spend)?.expect("vault");
/// assert_eq!(vault.info.m, 1);
///
/// // Alice alone can rekey the vault to 2-of-2.
/// let conditions = MedievalVault::rekey_create_coin_unsafe(ctx, launcher_id, 2, keys.clone())?;
/// vault.clone().spend(
///     ctx,
///     &[alice.pk],
///     conditions,
///     TESTNET11_CONSTANTS.genesis_challenge,
/// )?;
/// let vault_spend = ctx.take().remove(0);
/// sim.spend_coins(vec![vault_spend.clone()], &[alice.sk])?;
///
/// let new_vault = MedievalVault::from_parent_spend(ctx, &vault_spend)?.expect("vault");
/// assert_eq!(new_vault, vault.child(2, keys));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub type MedievalVault = Singleton<MedievalVaultInfo>;

impl MedievalVault {
    /// Parses the eve vault created by a singleton launcher spend.
    ///
    /// Returns [`None`] if the coin isn't a launcher, the launcher solution doesn't contain a
    /// [`MedievalVaultHint`], or the hint doesn't match the launched puzzle hash and amount.
    pub fn from_launcher_spend(
        ctx: &mut SpendContext,
        launcher_spend: &CoinSpend,
    ) -> Result<Option<Self>, DriverError> {
        if launcher_spend.coin.puzzle_hash != SINGLETON_LAUNCHER_HASH.into() {
            return Ok(None);
        }

        let solution = node_from_bytes(ctx, &launcher_spend.solution)?;
        let solution = ctx.extract::<LauncherSolution<NodePtr>>(solution)?;

        let Ok(hint) = ctx.extract::<MedievalVaultHint>(solution.key_value_list) else {
            return Ok(None);
        };

        let info = MedievalVaultInfo::from_hint(hint);

        let new_coin = Coin::new(
            launcher_spend.coin.coin_id(),
            SingletonArgs::curry_tree_hash(info.launcher_id, info.inner_puzzle_hash()).into(),
            1,
        );

        if launcher_spend.coin.amount != new_coin.amount
            || new_coin.puzzle_hash != solution.singleton_puzzle_hash
        {
            return Ok(None);
        }

        Ok(Some(Self::new(
            new_coin,
            Proof::Eve(EveProof {
                parent_parent_coin_info: launcher_spend.coin.parent_coin_info,
                parent_amount: launcher_spend.coin.amount,
            }),
            info,
        )))
    }

    /// Returns the vault coin created by spending this vault with the given new multisig configuration.
    ///
    /// Pass the current `m` and keys if the configuration doesn't change.
    pub fn child(&self, new_m: usize, new_public_key_list: Vec<PublicKey>) -> Self {
        let child_proof = Proof::Lineage(LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        });

        let child_info = MedievalVaultInfo::new(self.info.launcher_id, new_m, new_public_key_list);
        let child_inner_puzzle_hash = child_info.inner_puzzle_hash();

        Self {
            coin: Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(self.info.launcher_id, child_inner_puzzle_hash)
                    .into(),
                1,
            ),
            proof: child_proof,
            info: child_info,
        }
    }

    /// Parses the vault created by the spend of its parent, which may be a launcher or a vault.
    ///
    /// The new multisig configuration is read from a [`MedievalVaultHint`] in the memos of the first
    /// `CREATE_COIN` condition. If the memos don't contain one, the configuration is assumed to be
    /// unchanged. Returns [`None`] if the parent isn't a vault.
    pub fn from_parent_spend(
        ctx: &mut SpendContext,
        parent_spend: &CoinSpend,
    ) -> Result<Option<Self>, DriverError> {
        if parent_spend.coin.puzzle_hash == SINGLETON_LAUNCHER_HASH.into() {
            return Self::from_launcher_spend(ctx, parent_spend);
        }

        let solution = node_from_bytes(ctx, &parent_spend.solution)?;
        let puzzle = node_from_bytes(ctx, &parent_spend.puzzle_reveal)?;

        let puzzle_puzzle = Puzzle::from_clvm(ctx, puzzle)?;
        let Some(parent_layers) = SingletonLayer::<MOfNLayer>::parse_puzzle(ctx, puzzle_puzzle)?
        else {
            return Ok(None);
        };

        let output = ctx.run(puzzle, solution)?;
        let output = ctx.extract::<Conditions<NodePtr>>(output)?;
        let recreate_condition = output
            .into_iter()
            .find(|c| matches!(c, Condition::CreateCoin(..)));
        let Some(Condition::CreateCoin(recreate_condition)) = recreate_condition else {
            return Ok(None);
        };

        let (new_m, new_pubkeys) = if let Memos::Some(memos) = recreate_condition.memos {
            if let Ok(memos) = ctx.extract::<MedievalVaultHint>(memos) {
                (memos.m, memos.public_key_list)
            } else {
                (
                    parent_layers.inner_puzzle.m,
                    parent_layers.inner_puzzle.public_key_list.clone(),
                )
            }
        } else {
            (
                parent_layers.inner_puzzle.m,
                parent_layers.inner_puzzle.public_key_list.clone(),
            )
        };

        let parent_info = MedievalVaultInfo::new(
            parent_layers.launcher_id,
            parent_layers.inner_puzzle.m,
            parent_layers.inner_puzzle.public_key_list,
        );
        let new_info = MedievalVaultInfo::new(parent_layers.launcher_id, new_m, new_pubkeys);

        let new_coin = Coin::new(
            parent_spend.coin.coin_id(),
            SingletonArgs::curry_tree_hash(parent_layers.launcher_id, new_info.inner_puzzle_hash())
                .into(),
            1,
        );

        Ok(Some(Self::new(
            new_coin,
            Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_spend.coin.parent_coin_info,
                parent_inner_puzzle_hash: parent_info.inner_puzzle_hash().into(),
                parent_amount: parent_spend.coin.amount,
            }),
            new_info,
        )))
    }

    /// Makes `conditions` non-replayable by asserting the coin id and committing to the network.
    ///
    /// Adds `ASSERT_MY_COIN_ID` for `coin_id` unless the conditions already assert the coin id or
    /// parent id, and always adds a `REMARK` containing `genesis_challenge`.
    pub fn delegated_conditions(
        conditions: Conditions,
        coin_id: Bytes32,
        genesis_challenge: NodePtr,
    ) -> Conditions {
        MOfNLayer::ensure_non_replayable(conditions, coin_id, genesis_challenge)
    }

    /// Spends the vault with an arbitrary delegated puzzle and solution, signed by `used_pubkeys`.
    ///
    /// This is unsafe because the signatures can be replayed across coin generations and networks
    /// unless the delegated puzzle prevents it, for example with [`MedievalVault::delegated_conditions`].
    /// The required `AGG_SIG_UNSAFE` signatures aren't created here.
    // Mark this as unsafe since the transaction may be replayable
    //  across coin generations and networks if delegated puzzle is not
    //  properly secured.
    pub fn spend_sunsafe(
        self,
        ctx: &mut SpendContext,
        used_pubkeys: &[PublicKey],
        delegated_puzzle: NodePtr,
        delegated_solution: NodePtr,
    ) -> Result<(), DriverError> {
        let lineage_proof = self.proof;
        let coin = self.coin;

        let layers = self.info.into_layers();

        let puzzle = layers.construct_puzzle(ctx)?;
        let solution = layers.construct_solution(
            ctx,
            SingletonSolution {
                lineage_proof,
                amount: coin.amount,
                inner_solution: P2MOfNDelegateDirectSolution {
                    selectors: P2MOfNDelegateDirectArgs::selectors_for_used_pubkeys(
                        &self.info.public_key_list,
                        used_pubkeys,
                    ),
                    delegated_puzzle,
                    delegated_solution,
                },
            },
        )?;

        ctx.spend(coin, Spend::new(puzzle, solution))?;

        Ok(())
    }

    /// Spends the vault with `conditions`, signed by `used_pubkeys`.
    ///
    /// The conditions are made non-replayable with [`MedievalVault::delegated_conditions`] and
    /// quoted into the delegated puzzle. `used_pubkeys` must contain at least `m` of the vault's keys,
    /// and each of them must sign the delegated puzzle hash with `AGG_SIG_UNSAFE`.
    ///
    /// The conditions are responsible for recreating the vault.
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        used_pubkeys: &[PublicKey],
        conditions: Conditions,
        genesis_challenge: Bytes32,
    ) -> Result<(), DriverError> {
        let genesis_challenge = ctx.alloc(&genesis_challenge)?;
        let delegated_puzzle = ctx.alloc(&clvm_quote!(Self::delegated_conditions(
            conditions,
            self.coin.coin_id(),
            genesis_challenge
        )))?;

        self.spend_sunsafe(ctx, used_pubkeys, delegated_puzzle, NodePtr::NIL)
    }

    /// Returns a `CREATE_COIN` condition that recreates the vault with a new multisig configuration.
    ///
    /// The condition's memos are the new [`MedievalVaultHint`], so the new configuration can be
    /// parsed by [`MedievalVault::from_parent_spend`]. It's unsafe on its own because it isn't made
    /// non-replayable; use it with [`MedievalVault::spend`] or
    /// [`MedievalVault::delegated_puzzle_for_rekey`].
    pub fn rekey_create_coin_unsafe(
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        new_m: usize,
        new_pubkeys: Vec<PublicKey>,
    ) -> Result<Conditions, DriverError> {
        let new_info = MedievalVaultInfo::new(launcher_id, new_m, new_pubkeys);

        let memos = ctx.alloc(&new_info.to_hint())?;
        Ok(Conditions::new().create_coin(
            new_info.inner_puzzle_hash().into(),
            1,
            Memos::Some(memos),
        ))
    }

    /// Returns a non-replayable delegated puzzle that rekeys the vault with coin id `coin_id`.
    ///
    /// The signers sign the tree hash of this puzzle.
    pub fn delegated_puzzle_for_rekey(
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        new_m: usize,
        new_pubkeys: Vec<PublicKey>,
        coin_id: Bytes32,
        genesis_challenge: Bytes32,
    ) -> Result<NodePtr, DriverError> {
        let genesis_challenge = ctx.alloc(&genesis_challenge)?;
        let conditions = Self::rekey_create_coin_unsafe(ctx, launcher_id, new_m, new_pubkeys)?;

        ctx.alloc(&clvm_quote!(Self::delegated_conditions(
            conditions,
            coin_id,
            genesis_challenge
        )))
    }

    /// Returns a delegated puzzle that sends a message to another singleton and recreates the vault unchanged.
    ///
    /// The puzzle is the state scheduler layer wrapped around the quoted, non-replayable recreate
    /// conditions. It sends `prefix_and_message` (mode `0x12`) to the singleton with launcher id
    /// `receiver_launcher_id`. Its delegated solution is a
    /// [`StateSchedulerLayerSolution`](chia_sdk_types::puzzles::StateSchedulerLayerSolution) with
    /// the receiver's current inner puzzle hash.
    ///
    /// For example, sending [`XchandlesRegistryReceivedMessagePrefix::update_state`](crate::XchandlesRegistryReceivedMessagePrefix::update_state)
    /// to a registry approves a [`DelegatedStateAction`](crate::DelegatedStateAction) when the vault
    /// is the registry's price singleton.
    pub fn delegated_puzzle_for_flexible_send_message<M>(
        ctx: &mut SpendContext,
        prefix_and_message: M,
        receiver_launcher_id: Bytes32,
        my_coin: Coin,
        my_info: &MedievalVaultInfo,
        genesis_challenge: Bytes32,
    ) -> Result<NodePtr, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let conditions = Conditions::new().create_coin(
            my_info.inner_puzzle_hash().into(),
            my_coin.amount,
            ctx.hint(my_info.launcher_id)?,
        );
        let genesis_challenge = ctx.alloc(&genesis_challenge)?;

        let innermost_delegated_puzzle_ptr = ctx.alloc(&clvm_quote!(
            Self::delegated_conditions(conditions, my_coin.coin_id(), genesis_challenge)
        ))?;

        ctx.curry(StateSchedulerLayerArgs::<M, NodePtr> {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            receiver_singleton_struct_hash: SingletonStruct::new(receiver_launcher_id)
                .tree_hash()
                .into(),
            prefix_and_message,
            inner_puzzle: innermost_delegated_puzzle_ptr,
        })
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::Simulator;
    use chia_sdk_types::TESTNET11_CONSTANTS;

    use crate::Launcher;

    use super::*;

    #[test]
    fn test_medieval_vault() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut sim = Simulator::new();

        let user1 = sim.bls(0);
        let user2 = sim.bls(0);
        let user3 = sim.bls(0);

        let multisig_configs = [
            (1, vec![user1.pk, user2.pk]),
            (2, vec![user1.pk, user2.pk]),
            (3, vec![user1.pk, user2.pk, user3.pk]),
            (3, vec![user1.pk, user2.pk, user3.pk]),
            (1, vec![user1.pk, user2.pk, user3.pk]),
            (2, vec![user1.pk, user2.pk, user3.pk]),
        ];

        let launcher_coin = sim.new_coin(SINGLETON_LAUNCHER_HASH.into(), 1);
        let launcher = Launcher::new(launcher_coin.parent_coin_info, 1);
        let launch_hints = MedievalVaultHint {
            my_launcher_id: launcher_coin.coin_id(),
            m: multisig_configs[0].0,
            public_key_list: multisig_configs[0].1.clone(),
        };
        let (_conds, first_vault_coin) = launcher.spend(
            ctx,
            P2MOfNDelegateDirectArgs::curry_tree_hash(
                multisig_configs[0].0,
                multisig_configs[0].1.clone(),
            )
            .into(),
            launch_hints,
        )?;

        let spends = ctx.take();
        let launcher_spend = spends.first().unwrap().clone();
        sim.spend_coins(spends, &[])?;

        let mut vault = MedievalVault::from_parent_spend(ctx, &launcher_spend)?.unwrap();
        assert_eq!(vault.coin, first_vault_coin);

        let mut current_vault_info = MedievalVaultInfo {
            launcher_id: launcher_coin.coin_id(),
            m: multisig_configs[0].0,
            public_key_list: multisig_configs[0].1.clone(),
        };
        assert_eq!(vault.info, current_vault_info);

        for (i, (m, pubkeys)) in multisig_configs.clone().into_iter().enumerate().skip(1) {
            let mut recreate_memos = ctx.alloc(&vec![vault.info.launcher_id])?;

            let info_changed =
                multisig_configs[i - 1].0 != m || multisig_configs[i - 1].1 != pubkeys;
            if info_changed {
                recreate_memos = ctx.alloc(&MedievalVaultHint {
                    my_launcher_id: vault.info.launcher_id,
                    m,
                    public_key_list: pubkeys.clone(),
                })?;
            }
            current_vault_info = MedievalVaultInfo {
                launcher_id: vault.info.launcher_id,
                m,
                public_key_list: pubkeys.clone(),
            };

            let recreate_condition = Conditions::<NodePtr>::new().create_coin(
                current_vault_info.inner_puzzle_hash().into(),
                1,
                Memos::Some(recreate_memos),
            );

            let mut used_keys = 0;
            let mut used_pubkeys = vec![];
            while used_keys < vault.info.m {
                used_pubkeys.push(current_vault_info.public_key_list[used_keys]);
                used_keys += 1;
            }
            vault.clone().spend(
                ctx,
                &used_pubkeys,
                recreate_condition,
                TESTNET11_CONSTANTS.genesis_challenge,
            )?;

            let spends = ctx.take();
            let vault_spend = spends.first().unwrap().clone();
            sim.spend_coins(
                spends,
                &[user1.sk.clone(), user2.sk.clone(), user3.sk.clone()],
            )?;

            let check_vault = vault.child(m, pubkeys);

            vault = MedievalVault::from_parent_spend(ctx, &vault_spend)?.unwrap();
            assert_eq!(vault.info, current_vault_info);
            assert_eq!(vault, check_vault);
        }

        Ok(())
    }
}
