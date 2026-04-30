use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{SingletonMember, SingletonMemberSolution},
};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{DriverError, InnerPuzzleSpend, MipsSpend, Spend, SpendContext, mips_puzzle_hash};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct P2Singleton {
    pub launcher_id: Bytes32,
    pub nonce: usize,
}

impl P2Singleton {
    pub fn new(launcher_id: Bytes32, nonce: usize) -> Self {
        Self { launcher_id, nonce }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        let mut mips = MipsSpend::new(delegated_spend);

        let puzzle = ctx.curry(SingletonMember::new(self.launcher_id))?;
        let solution = ctx.alloc(&SingletonMemberSolution::new(
            singleton_inner_puzzle_hash,
            singleton_amount,
        ))?;

        let custody_hash = self.tree_hash();

        mips.members.insert(
            custody_hash,
            InnerPuzzleSpend::new(self.nonce, vec![], Spend::new(puzzle, solution)),
        );

        mips.spend(ctx, custody_hash)
    }
}

impl ToTreeHash for P2Singleton {
    fn tree_hash(&self) -> TreeHash {
        mips_puzzle_hash(
            self.nonce,
            vec![],
            SingletonMember::new(self.launcher_id).curry_tree_hash(),
            true,
        )
    }
}
