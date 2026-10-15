use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{FixedPuzzleMember, SingletonMember, SingletonMemberSolution},
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, InnerPuzzleSpend, MipsSpend, MofN, Spend, SpendContext, mips_puzzle_hash,
};

/// The version of a [`P2ConditionsOrSingleton`] puzzle, which determines the format of its fixed path.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum P2ConditionsOrSingletonVersion {
    /// The original format, where the fixed path's member is the quoted conditions.
    Legacy,
    /// The fixed path's member is a [`FixedPuzzleMember`], which requires the delegated puzzle to
    /// be the quoted conditions.
    #[default]
    FixedPuzzleMember,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct P2ConditionsOrSingleton {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    /// The tree hash of the quoted conditions which are output by the fixed path.
    pub quoted_conditions_hash: Bytes32,
    pub version: P2ConditionsOrSingletonVersion,
}

impl P2ConditionsOrSingleton {
    pub fn new(
        launcher_id: Bytes32,
        nonce: usize,
        quoted_conditions_hash: Bytes32,
        version: P2ConditionsOrSingletonVersion,
    ) -> Self {
        Self {
            launcher_id,
            nonce,
            quoted_conditions_hash,
            version,
        }
    }

    pub fn fixed_path_hash(&self) -> TreeHash {
        let member_hash = match self.version {
            P2ConditionsOrSingletonVersion::Legacy => self.quoted_conditions_hash.into(),
            P2ConditionsOrSingletonVersion::FixedPuzzleMember => {
                FixedPuzzleMember::new(self.quoted_conditions_hash).curry_tree_hash()
            }
        };

        mips_puzzle_hash(0, vec![], member_hash, false)
    }

    pub fn p2_singleton_path_hash(&self) -> TreeHash {
        mips_puzzle_hash(
            self.nonce,
            vec![],
            SingletonMember::new(self.launcher_id).curry_tree_hash(),
            false,
        )
    }

    pub fn p2_singleton_spend(
        &self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        let mut mips = MipsSpend::new(delegated_spend);

        let fixed_hash = self.fixed_path_hash();
        let p2_singleton_hash = self.p2_singleton_path_hash();
        let custody_hash = self.tree_hash();

        mips.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, vec![], 1, vec![fixed_hash, p2_singleton_hash]),
        );

        let puzzle = ctx.curry(SingletonMember::new(self.launcher_id))?;
        let solution = ctx.alloc(&SingletonMemberSolution::new(
            singleton_inner_puzzle_hash,
            singleton_amount,
        ))?;

        mips.members.insert(
            p2_singleton_hash,
            InnerPuzzleSpend::new(self.nonce, vec![], Spend::new(puzzle, solution)),
        );

        mips.spend(ctx, custody_hash)
    }

    /// Spends the fixed path. The spend's puzzle must be the quoted conditions that the quoted
    /// conditions hash was calculated from.
    pub fn fixed_spend(
        &self,
        ctx: &mut SpendContext,
        quoted_conditions: Spend,
    ) -> Result<Spend, DriverError> {
        let quoted_conditions_hash: Bytes32 = ctx.tree_hash(quoted_conditions.puzzle).into();

        if quoted_conditions_hash != self.quoted_conditions_hash {
            return Err(DriverError::WrongConditions);
        }

        let fixed_hash = self.fixed_path_hash();
        let p2_singleton_hash = self.p2_singleton_path_hash();
        let custody_hash = self.tree_hash();

        let (mut mips, member_spend) = match self.version {
            P2ConditionsOrSingletonVersion::Legacy => (
                MipsSpend::new(Spend::new(NodePtr::NIL, NodePtr::NIL)),
                quoted_conditions,
            ),
            P2ConditionsOrSingletonVersion::FixedPuzzleMember => {
                let member_puzzle = ctx.curry(FixedPuzzleMember::new(quoted_conditions_hash))?;

                (
                    MipsSpend::new(quoted_conditions),
                    Spend::new(member_puzzle, NodePtr::NIL),
                )
            }
        };

        mips.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, vec![], 1, vec![fixed_hash, p2_singleton_hash]),
        );

        mips.members
            .insert(fixed_hash, InnerPuzzleSpend::new(0, vec![], member_spend));

        mips.spend(ctx, custody_hash)
    }
}

impl ToTreeHash for P2ConditionsOrSingleton {
    fn tree_hash(&self) -> TreeHash {
        let fixed_hash = self.fixed_path_hash();

        let p2_singleton_hash = self.p2_singleton_path_hash();

        mips_puzzle_hash(
            0,
            vec![],
            MofN::new(1, vec![fixed_hash, p2_singleton_hash]).inner_puzzle_hash(),
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_p2_conditions_or_singleton_fixed_spend(
        #[values(
            P2ConditionsOrSingletonVersion::Legacy,
            P2ConditionsOrSingletonVersion::FixedPuzzleMember
        )]
        version: P2ConditionsOrSingletonVersion,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let quoted_conditions = ctx.delegated_spend(Conditions::new().create_coin(
            Bytes32::new([1; 32]),
            1,
            Memos::None,
        ))?;
        let quoted_conditions_hash = ctx.tree_hash(quoted_conditions.puzzle).into();
        let p2 =
            P2ConditionsOrSingleton::new(Bytes32::default(), 0, quoted_conditions_hash, version);

        let coin = sim.new_coin(p2.tree_hash().into(), 1);
        let spend = p2.fixed_spend(&mut ctx, quoted_conditions)?;
        ctx.spend(coin, spend)?;

        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }

    #[rstest]
    fn test_p2_conditions_or_singleton_fixed_spend_wrong_conditions(
        #[values(
            P2ConditionsOrSingletonVersion::Legacy,
            P2ConditionsOrSingletonVersion::FixedPuzzleMember
        )]
        version: P2ConditionsOrSingletonVersion,
    ) -> Result<()> {
        let mut ctx = SpendContext::new();

        let quoted_conditions = ctx.delegated_spend(Conditions::new().reserve_fee(0))?;
        let quoted_conditions_hash = ctx.tree_hash(quoted_conditions.puzzle).into();
        let p2 =
            P2ConditionsOrSingleton::new(Bytes32::default(), 0, quoted_conditions_hash, version);

        let other_conditions = ctx.delegated_spend(Conditions::new().reserve_fee(1))?;

        assert!(matches!(
            p2.fixed_spend(&mut ctx, other_conditions).unwrap_err(),
            DriverError::WrongConditions
        ));

        Ok(())
    }

    #[test]
    fn test_p2_conditions_or_singleton_fixed_spend_delegated_puzzle() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let quoted_conditions = ctx.delegated_spend(Conditions::new().reserve_fee(0))?;
        let quoted_conditions_hash = ctx.tree_hash(quoted_conditions.puzzle).into();
        let p2 = P2ConditionsOrSingleton::new(
            Bytes32::default(),
            0,
            quoted_conditions_hash,
            P2ConditionsOrSingletonVersion::FixedPuzzleMember,
        );

        let coin = sim.new_coin(p2.tree_hash().into(), 1);

        // The fixed path only allows the quoted conditions as the delegated puzzle.
        let other_spend = ctx.delegated_spend(Conditions::new().create_coin(
            Bytes32::new([1; 32]),
            1,
            Memos::None,
        ))?;

        let mut mips = MipsSpend::new(other_spend);
        let custody_hash = p2.tree_hash();
        mips.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(
                0,
                vec![],
                1,
                vec![p2.fixed_path_hash(), p2.p2_singleton_path_hash()],
            ),
        );
        let member_puzzle = ctx.curry(FixedPuzzleMember::new(quoted_conditions_hash))?;
        mips.members.insert(
            p2.fixed_path_hash(),
            InnerPuzzleSpend::new(0, vec![], Spend::new(member_puzzle, NodePtr::NIL)),
        );
        let spend = mips.spend(&mut ctx, custody_hash)?;
        ctx.spend(coin, spend)?;

        assert!(sim.spend_coins(ctx.take(), &[]).is_err());

        Ok(())
    }

    #[test]
    fn test_p2_conditions_or_singleton_legacy_puzzle_hash() {
        // The legacy puzzle hash must not change, so that existing coins can still be spent.
        let quoted_conditions_hash = Bytes32::new([1; 32]);

        let p2 = P2ConditionsOrSingleton::new(
            Bytes32::new([2; 32]),
            3,
            quoted_conditions_hash,
            P2ConditionsOrSingletonVersion::Legacy,
        );

        let fixed_hash = mips_puzzle_hash(0, vec![], quoted_conditions_hash.into(), false);

        assert_eq!(p2.fixed_path_hash(), fixed_hash);
        assert_eq!(
            p2.tree_hash(),
            mips_puzzle_hash(
                0,
                vec![],
                MofN::new(1, vec![fixed_hash, p2.p2_singleton_path_hash()]).inner_puzzle_hash(),
                true,
            )
        );
    }
}
