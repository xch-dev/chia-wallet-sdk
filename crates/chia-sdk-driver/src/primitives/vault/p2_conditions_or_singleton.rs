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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct P2ConditionsOrSingleton {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    /// The tree hash of the quoted conditions which can be output by anyone.
    pub quoted_conditions_hash: Bytes32,
}

impl P2ConditionsOrSingleton {
    pub fn new(launcher_id: Bytes32, nonce: usize, quoted_conditions_hash: Bytes32) -> Self {
        Self {
            launcher_id,
            nonce,
            quoted_conditions_hash,
        }
    }

    pub fn from_quoted_conditions_hash(
        launcher_id: Bytes32,
        nonce: usize,
        quoted_conditions_hash: Bytes32,
    ) -> Self {
        Self::new(launcher_id, nonce, quoted_conditions_hash)
    }

    /// The fixed path is a member which only allows the delegated puzzle to be the quoted conditions.
    pub fn fixed_path_hash(&self) -> TreeHash {
        mips_puzzle_hash(
            0,
            vec![],
            FixedPuzzleMember::new(self.quoted_conditions_hash).curry_tree_hash(),
            false,
        )
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

    /// Spends the fixed path. The delegated spend must be the quoted conditions that the
    /// fixed conditions hash was calculated from.
    pub fn fixed_spend(
        &self,
        ctx: &mut SpendContext,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        let quoted_conditions_hash: Bytes32 = ctx.tree_hash(delegated_spend.puzzle).into();

        if quoted_conditions_hash != self.quoted_conditions_hash {
            return Err(DriverError::WrongConditions);
        }

        let member = FixedPuzzleMember::new(quoted_conditions_hash);

        let mut mips = MipsSpend::new(delegated_spend);

        let fixed_hash = self.fixed_path_hash();
        let p2_singleton_hash = self.p2_singleton_path_hash();
        let custody_hash = self.tree_hash();

        mips.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, vec![], 1, vec![fixed_hash, p2_singleton_hash]),
        );

        let member_puzzle = ctx.curry(member)?;

        mips.members.insert(
            fixed_hash,
            InnerPuzzleSpend::new(0, vec![], Spend::new(member_puzzle, NodePtr::NIL)),
        );

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

    use super::*;

    #[test]
    fn test_p2_conditions_or_singleton_fixed_spend() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let delegated_spend = ctx.delegated_spend(Conditions::new().create_coin(
            Bytes32::new([1; 32]),
            1,
            Memos::None,
        ))?;
        let quoted_conditions_hash = ctx.tree_hash(delegated_spend.puzzle).into();
        let p2 = P2ConditionsOrSingleton::from_quoted_conditions_hash(
            Bytes32::default(),
            0,
            quoted_conditions_hash,
        );

        let coin = sim.new_coin(p2.tree_hash().into(), 1);
        let spend = p2.fixed_spend(&mut ctx, delegated_spend)?;
        ctx.spend(coin, spend)?;

        sim.spend_coins(ctx.take(), &[])?;

        Ok(())
    }

    #[test]
    fn test_p2_conditions_or_singleton_fixed_spend_other_conditions() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let delegated_spend = ctx.delegated_spend(Conditions::new().reserve_fee(0))?;
        let quoted_conditions_hash = ctx.tree_hash(delegated_spend.puzzle).into();
        let p2 = P2ConditionsOrSingleton::from_quoted_conditions_hash(
            Bytes32::default(),
            0,
            quoted_conditions_hash,
        );

        let coin = sim.new_coin(p2.tree_hash().into(), 1);

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
}
