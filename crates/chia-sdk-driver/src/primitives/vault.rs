mod vault_info;
mod vault_launcher;

pub use vault_info::*;

use chia_puzzle_types::singleton::{SingletonArgs, SingletonSolution};
use clvm_utils::TreeHash;

use crate::{DriverError, Singleton, Spend, SpendContext};

use super::MipsSpend;

pub type Vault = Singleton<VaultInfo>;

impl Vault {
    pub fn child(&self, custody_hash: TreeHash, amount: u64) -> Self {
        self.child_with(VaultInfo::new(self.info.launcher_id, custody_hash), amount)
    }

    pub fn spend(&self, ctx: &mut SpendContext, spend: &MipsSpend) -> Result<(), DriverError> {
        let custody_spend = spend.spend(ctx, self.info.custody_hash)?;

        let puzzle = ctx.curry(SingletonArgs::new(
            self.info.launcher_id,
            custody_spend.puzzle,
        ))?;
        let solution = ctx.alloc(&SingletonSolution {
            lineage_proof: self.proof,
            amount: self.coin.amount,
            inner_solution: custody_spend.solution,
        })?;

        ctx.spend(self.coin, Spend::new(puzzle, solution))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzle_types::Memos;
    use chia_sdk_test::{K1Pair, Simulator};
    use chia_sdk_types::{
        puzzles::{K1Member, K1MemberSolution},
        Conditions, Mod,
    };
    use chia_secp::{K1SecretKey, K1Signature};
    use chia_sha2::Sha256;
    use rstest::rstest;

    use crate::{mips_puzzle_hash, InnerPuzzleSpend, Launcher, MofN, StandardLayer};

    use super::*;

    fn mint_vault(
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        custody_hash: TreeHash,
    ) -> anyhow::Result<Vault> {
        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (mint_vault, vault) =
            Launcher::new(alice.coin.coin_id(), 1).mint_vault(ctx, custody_hash, ())?;
        alice_p2.spend(ctx, alice.coin, mint_vault)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(vault)
    }

    fn k1_sign(
        ctx: &SpendContext,
        vault: &Vault,
        spend: &MipsSpend,
        k1: &K1SecretKey,
    ) -> anyhow::Result<K1Signature> {
        let mut hasher = Sha256::new();
        hasher.update(ctx.tree_hash(spend.delegated.puzzle));
        hasher.update(vault.coin.coin_id());
        Ok(k1.sign_prehashed(&hasher.finalize())?)
    }

    #[test]
    fn test_simple_vault() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let k1 = K1Pair::default();
        let custody = K1Member::new(k1.pk);
        let custody_hash = mips_puzzle_hash(0, Vec::new(), custody.curry_tree_hash(), true);

        let vault = mint_vault(&mut sim, ctx, custody_hash)?;

        let conditions =
            Conditions::new().create_coin(vault.info.custody_hash.into(), 1, Memos::None);
        let mut spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

        let signature = k1_sign(ctx, &vault, &spend, &k1.sk)?;
        let k1_puzzle = ctx.curry(custody)?;
        let k1_solution = ctx.alloc(&K1MemberSolution::new(vault.coin.coin_id(), signature))?;

        spend.members.insert(
            custody_hash,
            InnerPuzzleSpend::new(0, Vec::new(), Spend::new(k1_puzzle, k1_solution)),
        );

        vault.spend(ctx, &spend)?;

        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }

    #[rstest]
    #[case::vault_1_of_1(1, 1)]
    #[case::vault_1_of_2(1, 2)]
    #[case::vault_1_of_3(1, 3)]
    #[case::vault_1_of_4(1, 4)]
    #[case::vault_2_of_2(2, 2)]
    #[case::vault_2_of_3(2, 3)]
    #[case::vault_2_of_4(2, 4)]
    #[case::vault_3_of_3(3, 3)]
    #[case::vault_3_of_4(3, 4)]
    #[case::vault_4_of_4(4, 4)]
    fn test_m_of_n_vault(#[case] required: usize, #[case] key_count: usize) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let keys = K1Pair::range_vec(key_count);

        let members = keys.iter().map(|k| K1Member::new(k.pk)).collect::<Vec<_>>();

        let hashes = members
            .iter()
            .map(|m| mips_puzzle_hash(0, Vec::new(), m.curry_tree_hash(), false))
            .collect::<Vec<_>>();

        let custody = MofN::new(required, hashes.clone());
        let custody_hash = mips_puzzle_hash(0, Vec::new(), custody.inner_puzzle_hash(), true);

        let mut vault = mint_vault(&mut sim, ctx, custody_hash)?;

        for start in 0..key_count {
            let conditions =
                Conditions::new().create_coin(vault.info.custody_hash.into(), 1, Memos::None);
            let mut spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

            spend.members.insert(
                custody_hash,
                InnerPuzzleSpend::m_of_n(0, Vec::new(), custody.required, custody.items.clone()),
            );

            let mut i = start;

            for _ in 0..required {
                let signature = k1_sign(ctx, &vault, &spend, &keys[i].sk)?;

                let k1_puzzle = ctx.curry(members[i])?;
                let k1_solution =
                    ctx.alloc(&K1MemberSolution::new(vault.coin.coin_id(), signature))?;

                spend.members.insert(
                    hashes[i],
                    InnerPuzzleSpend::new(0, Vec::new(), Spend::new(k1_puzzle, k1_solution)),
                );

                i += 1;

                if i >= key_count {
                    i = 0;
                }
            }

            vault.spend(ctx, &spend)?;
            vault = vault.child(vault.info.custody_hash, vault.coin.amount);

            sim.spend_coins(ctx.take(), &[])?;
        }

        Ok(())
    }
}
