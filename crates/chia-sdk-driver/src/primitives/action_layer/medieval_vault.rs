use chia::{
    bls::PublicKey,
    clvm_utils::{CurriedProgram, ToTreeHash},
    protocol::{Bytes32, Coin, CoinSpend},
    puzzles::{
        singleton::{LauncherSolution, SingletonArgs, SingletonSolution, SingletonStruct},
        EveProof, LineageProof, Proof,
    },
};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_wallet_sdk::{
    driver::{DriverError, Layer, Puzzle, SingletonLayer, Spend, SpendContext},
    prelude::{Condition, Conditions, Memos},
};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvmr::{serde::node_from_bytes, Allocator, NodePtr};

use crate::{
    MOfNLayer, P2MOfNDelegateDirectArgs, P2MOfNDelegateDirectSolution, SpendContextExt,
    StateSchedulerLayerArgs,
};

use super::{MedievalVaultHint, MedievalVaultInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MedievalVault {
    pub coin: Coin,
    pub proof: Proof,

    pub info: MedievalVaultInfo,
}

impl MedievalVault {
    pub fn new(coin: Coin, proof: Proof, info: MedievalVaultInfo) -> Self {
        Self { coin, proof, info }
    }

    pub fn from_launcher_spend(
        ctx: &mut SpendContext,
        launcher_spend: CoinSpend,
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

    pub fn child(&self, new_m: usize, new_public_key_list: Vec<PublicKey>) -> Option<Self> {
        let child_proof = Proof::Lineage(LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        });

        let child_info = MedievalVaultInfo::new(self.info.launcher_id, new_m, new_public_key_list);
        let child_inner_puzzle_hash = child_info.inner_puzzle_hash();

        Some(Self {
            coin: Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(self.info.launcher_id, child_inner_puzzle_hash)
                    .into(),
                1,
            ),
            proof: child_proof,
            info: child_info,
        })
    }

    pub fn from_parent_spend(
        ctx: &mut SpendContext,
        parent_spend: CoinSpend,
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

    pub fn delegated_conditions(
        conditions: Conditions,
        coin_id: Bytes32,
        genesis_challenge: NodePtr,
    ) -> Conditions {
        MOfNLayer::ensure_non_replayable(conditions, coin_id, genesis_challenge)
    }

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

    pub fn delegated_puzzle_for_flexible_send_message<M>(
        ctx: &mut SpendContext,
        message: M,
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

        let program = ctx.state_scheduler_puzzle()?;
        ctx.alloc(&CurriedProgram {
            program,
            args: StateSchedulerLayerArgs::<M, NodePtr> {
                singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
                receiver_singleton_struct_hash: SingletonStruct::new(receiver_launcher_id)
                    .tree_hash()
                    .into(),
                message,
                inner_puzzle: innermost_delegated_puzzle_ptr,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use chia_wallet_sdk::{driver::Launcher, test::Simulator, types::TESTNET11_CONSTANTS};

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

        let mut vault = MedievalVault::from_parent_spend(ctx, launcher_spend)?.unwrap();
        assert_eq!(vault.coin, first_vault_coin);

        let mut current_vault_info = MedievalVaultInfo {
            launcher_id: launcher_coin.coin_id(),
            m: multisig_configs[0].0,
            public_key_list: multisig_configs[0].1.clone(),
        };
        assert_eq!(vault.info, current_vault_info);

        for (i, (m, pubkeys)) in multisig_configs.clone().into_iter().enumerate().skip(1) {
            let mut recreate_memos: NodePtr = ctx.alloc(&vec![vault.info.launcher_id])?;

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

            let check_vault = vault.child(m, pubkeys).unwrap();

            vault = MedievalVault::from_parent_spend(ctx, vault_spend)?.unwrap();
            assert_eq!(vault.info, current_vault_info);
            assert_eq!(vault, check_vault);
        }

        Ok(())
    }
}
