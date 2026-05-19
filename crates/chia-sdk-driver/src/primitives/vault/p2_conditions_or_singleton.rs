use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{SingletonMember, SingletonMemberSolution},
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
    pub fixed_conditions_hash: Bytes32,
}

impl P2ConditionsOrSingleton {
    pub fn new(launcher_id: Bytes32, nonce: usize, fixed_conditions_hash: Bytes32) -> Self {
        Self {
            launcher_id,
            nonce,
            fixed_conditions_hash,
        }
    }

    pub fn fixed_conditions_hash(quoted_conditions_hash: Bytes32) -> Bytes32 {
        mips_puzzle_hash(0, vec![], quoted_conditions_hash.into(), false).into()
    }

    pub fn from_quoted_conditions_hash(
        launcher_id: Bytes32,
        nonce: usize,
        quoted_conditions_hash: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            nonce,
            fixed_conditions_hash: Self::fixed_conditions_hash(quoted_conditions_hash),
        }
    }

    pub fn fixed_path_hash(&self) -> TreeHash {
        self.fixed_conditions_hash.into()
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

    pub fn fixed_spend(
        &self,
        ctx: &mut SpendContext,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        let mut mips = MipsSpend::new(Spend::new(NodePtr::NIL, NodePtr::NIL));

        let fixed_hash = self.fixed_path_hash();
        let p2_singleton_hash = self.p2_singleton_path_hash();
        let custody_hash = self.tree_hash();

        mips.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, vec![], 1, vec![fixed_hash, p2_singleton_hash]),
        );

        mips.members.insert(
            fixed_hash,
            InnerPuzzleSpend::new(0, vec![], delegated_spend),
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
