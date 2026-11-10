use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{SingletonMember, SingletonMemberSolution},
};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{DriverError, InnerPuzzleSpend, MipsSpend, Spend, SpendContext, mips_puzzle_hash};

/// A p2 puzzle that can only be spent with a message from a singleton, such as a [`Vault`](crate::Vault).
///
/// The puzzle is a top level MIPS puzzle with a single [`SingletonMember`] for the launcher id.
/// The singleton sends a message to the coin, which authorizes its delegated spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct P2Singleton {
    /// The launcher id of the singleton that controls the coin.
    pub launcher_id: Bytes32,
    /// The MIPS nonce, which can be used to create multiple puzzle hashes for the same singleton.
    pub nonce: usize,
}

impl P2Singleton {
    /// Creates a new [`P2Singleton`].
    pub fn new(launcher_id: Bytes32, nonce: usize) -> Self {
        Self { launcher_id, nonce }
    }

    /// Creates a spend of the coin, with the given delegated spend.
    ///
    /// `singleton_inner_puzzle_hash` and `singleton_amount` describe the controlling singleton,
    /// which must be spent in the same transaction and send the authorizing message.
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
