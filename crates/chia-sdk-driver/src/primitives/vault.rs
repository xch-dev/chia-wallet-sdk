mod p2_conditions_or_singleton;
mod p2_singleton;
mod vault_info;
mod vault_launcher;

pub use p2_conditions_or_singleton::*;
pub use p2_singleton::*;
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
        Conditions, Mod,
        puzzles::{K1Member, K1MemberSolution},
    };
    use chia_secp::{K1SecretKey, K1Signature};
    use chia_sha2::Sha256;
    use rstest::rstest;

    use crate::{InnerPuzzleSpend, Launcher, MofN, StandardLayer, mips_puzzle_hash};

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

    #[test]
    fn test_m_of_n_vault_extra_member_spends() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let keys = K1Pair::range_vec(4);
        let members = keys.iter().map(|k| K1Member::new(k.pk)).collect::<Vec<_>>();
        let hashes = members
            .iter()
            .map(|m| mips_puzzle_hash(0, Vec::new(), m.curry_tree_hash(), false))
            .collect::<Vec<_>>();

        let custody = MofN::new(2, hashes.clone());
        let custody_hash = mips_puzzle_hash(0, Vec::new(), custody.inner_puzzle_hash(), true);

        let vault = mint_vault(&mut sim, ctx, custody_hash)?;

        let conditions =
            Conditions::new().create_coin(vault.info.custody_hash.into(), 1, Memos::None);
        let mut spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

        spend.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, Vec::new(), custody.required, custody.items.clone()),
        );

        // The puzzle requires exactly 2 members, so the extra spends must be left out.
        for (key, (member, hash)) in keys.iter().zip(members.iter().zip(&hashes)) {
            let signature = k1_sign(ctx, &vault, &spend, &key.sk)?;
            let puzzle = ctx.curry(*member)?;
            let solution = ctx.alloc(&K1MemberSolution::new(vault.coin.coin_id(), signature))?;
            spend.members.insert(
                *hash,
                InnerPuzzleSpend::new(0, Vec::new(), Spend::new(puzzle, solution)),
            );
        }

        vault.spend(ctx, &spend)?;
        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }

    #[rstest]
    #[case::zero_of_two(0, 2)]
    #[case::three_of_two(3, 2)]
    #[case::one_of_zero(1, 0)]
    fn test_m_of_n_invalid_required(
        #[case] required: usize,
        #[case] count: usize,
    ) -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let items = K1Pair::range_vec(count)
            .iter()
            .map(|k| mips_puzzle_hash(0, Vec::new(), K1Member::new(k.pk).curry_tree_hash(), false))
            .collect::<Vec<_>>();
        let m_of_n = MofN::new(required, items);
        let custody_hash = mips_puzzle_hash(0, Vec::new(), m_of_n.inner_puzzle_hash(), true);

        let mut spend = MipsSpend::new(ctx.delegated_spend(Conditions::new())?);
        spend.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, Vec::new(), required, m_of_n.items.clone()),
        );

        assert!(matches!(
            spend.spend(ctx, custody_hash),
            Err(DriverError::InvalidMofNRequired { .. })
        ));

        Ok(())
    }

    #[test]
    fn test_vault_wrapper_stacks_of_different_lengths() -> anyhow::Result<()> {
        use chia_sdk_types::puzzles::{AddDelegatedPuzzleWrapper, PreventConditionOpcode};
        use chia_secp::K1PublicKey;

        use crate::{Restriction, RestrictionKind};

        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let wrappers = [
            PreventConditionOpcode::new(60),
            PreventConditionOpcode::new(62),
        ];
        let wrapper_restrictions = wrappers
            .iter()
            .map(|wrapper| Restriction {
                kind: RestrictionKind::DelegatedPuzzleWrapper,
                puzzle_hash: wrapper.curry_tree_hash(),
            })
            .collect::<Vec<_>>();

        // Both members are spent, but the first only enforces the outer wrapper, so its enforcer
        // must be given the delegated puzzle hash wrapped in the inner wrapper.
        let keys = K1Pair::range_vec(2);
        let member = |pk: K1PublicKey| K1Member::new(pk);
        let member_restrictions = [
            wrapper_restrictions[..1].to_vec(),
            wrapper_restrictions.clone(),
        ];
        let hashes = keys
            .iter()
            .zip(&member_restrictions)
            .map(|(key, restrictions)| {
                mips_puzzle_hash(
                    0,
                    restrictions.clone(),
                    member(key.pk).curry_tree_hash(),
                    false,
                )
            })
            .collect::<Vec<_>>();

        let custody = MofN::new(2, hashes.clone());
        let custody_hash = mips_puzzle_hash(0, Vec::new(), custody.inner_puzzle_hash(), true);

        let vault = mint_vault(&mut sim, ctx, custody_hash)?;

        let conditions =
            Conditions::new().create_coin(vault.info.custody_hash.into(), 1, Memos::None);
        let mut spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

        spend.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, Vec::new(), custody.required, custody.items.clone()),
        );

        for wrapper in wrappers {
            let puzzle = ctx.curry(wrapper)?;
            spend.restrictions.insert(
                wrapper.curry_tree_hash(),
                Spend::new(puzzle, clvmr::NodePtr::NIL),
            );
        }

        // Each member signs the fully wrapped delegated puzzle hash.
        let mut delegated_puzzle_hash = ctx.tree_hash(spend.delegated.puzzle);
        for wrapper in wrappers.iter().rev() {
            delegated_puzzle_hash =
                AddDelegatedPuzzleWrapper::new(wrapper.curry_tree_hash(), delegated_puzzle_hash)
                    .curry_tree_hash();
        }

        for ((key, restrictions), hash) in keys.iter().zip(member_restrictions).zip(&hashes) {
            let mut hasher = Sha256::new();
            hasher.update(delegated_puzzle_hash);
            hasher.update(vault.coin.coin_id());
            let signature = key.sk.sign_prehashed(&hasher.finalize())?;

            let puzzle = ctx.curry(member(key.pk))?;
            let solution = ctx.alloc(&K1MemberSolution::new(vault.coin.coin_id(), signature))?;
            spend.members.insert(
                *hash,
                InnerPuzzleSpend::new(0, restrictions, Spend::new(puzzle, solution)),
            );
        }

        vault.spend(ctx, &spend)?;
        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }

    #[cfg(feature = "chip-0037")]
    fn eip712_member_spend(
        ctx: &mut SpendContext,
        vault: &Vault,
        spend: &MipsSpend,
        member: chia_sdk_types::puzzles::Eip712Member,
        k1: &K1SecretKey,
    ) -> anyhow::Result<Spend> {
        use chia_sdk_types::puzzles::{Eip712MemberSolution, eip712_hash_to_sign};

        let signed_hash = eip712_hash_to_sign(
            &member.prefix_and_domain_separator,
            vault.coin.coin_id(),
            ctx.tree_hash(spend.delegated.puzzle).into(),
        );
        let signature = k1.sign_prehashed(&signed_hash.to_bytes())?;

        let puzzle = ctx.curry(member)?;
        let solution = ctx.alloc(&Eip712MemberSolution::new(
            vault.coin.coin_id(),
            signed_hash,
            signature,
        ))?;

        Ok(Spend::new(puzzle, solution))
    }

    #[cfg(feature = "chip-0037")]
    #[test]
    fn test_eip712_vault() -> anyhow::Result<()> {
        use chia_consensus::consensus_constants::TEST_CONSTANTS;
        use chia_sdk_types::puzzles::{
            Eip712Member, eip712_prefix_and_domain_separator, eip712_type_hash,
        };

        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let k1 = K1Pair::default();
        let custody = Eip712Member::new(
            eip712_prefix_and_domain_separator(TEST_CONSTANTS.genesis_challenge),
            eip712_type_hash(),
            k1.pk,
        );
        let custody_hash = mips_puzzle_hash(0, Vec::new(), custody.curry_tree_hash(), true);

        let vault = mint_vault(&mut sim, ctx, custody_hash)?;

        let conditions =
            Conditions::new().create_coin(vault.info.custody_hash.into(), 1, Memos::None);
        let mut spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

        let member_spend = eip712_member_spend(ctx, &vault, &spend, custody, &k1.sk)?;
        spend.members.insert(
            custody_hash,
            InnerPuzzleSpend::new(0, Vec::new(), member_spend),
        );

        vault.spend(ctx, &spend)?;

        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }

    #[cfg(feature = "chip-0037")]
    #[test]
    fn test_eip712_and_k1_2_of_2_vault() -> anyhow::Result<()> {
        use chia_consensus::consensus_constants::TEST_CONSTANTS;
        use chia_sdk_types::puzzles::{
            Eip712Member, eip712_prefix_and_domain_separator, eip712_type_hash,
        };

        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let keys = K1Pair::range_vec(2);

        let k1_member = K1Member::new(keys[0].pk);
        let eip712_member = Eip712Member::new(
            eip712_prefix_and_domain_separator(TEST_CONSTANTS.genesis_challenge),
            eip712_type_hash(),
            keys[1].pk,
        );

        let k1_hash = mips_puzzle_hash(0, Vec::new(), k1_member.curry_tree_hash(), false);
        let eip712_hash = mips_puzzle_hash(0, Vec::new(), eip712_member.curry_tree_hash(), false);

        let custody = MofN::new(2, vec![k1_hash, eip712_hash]);
        let custody_hash = mips_puzzle_hash(0, Vec::new(), custody.inner_puzzle_hash(), true);

        let vault = mint_vault(&mut sim, ctx, custody_hash)?;

        let conditions =
            Conditions::new().create_coin(vault.info.custody_hash.into(), 1, Memos::None);
        let mut spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

        spend.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, Vec::new(), custody.required, custody.items.clone()),
        );

        let signature = k1_sign(ctx, &vault, &spend, &keys[0].sk)?;
        let k1_puzzle = ctx.curry(k1_member)?;
        let k1_solution = ctx.alloc(&K1MemberSolution::new(vault.coin.coin_id(), signature))?;
        spend.members.insert(
            k1_hash,
            InnerPuzzleSpend::new(0, Vec::new(), Spend::new(k1_puzzle, k1_solution)),
        );

        let member_spend = eip712_member_spend(ctx, &vault, &spend, eip712_member, &keys[1].sk)?;
        spend.members.insert(
            eip712_hash,
            InnerPuzzleSpend::new(0, Vec::new(), member_spend),
        );

        vault.spend(ctx, &spend)?;

        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }
}
