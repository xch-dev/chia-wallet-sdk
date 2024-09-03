use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::{SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 singleton [`Layer`] allows for requiring that a
/// singleton be spent alongside this coin to authorize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2Singleton {
    pub launcher_id: Bytes32,
}

impl P2Singleton {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self { launcher_id }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        let coin_spend = self.construct_coin_spend(
            ctx,
            coin,
            P2SingletonSolution {
                singleton_inner_puzzle_hash,
                my_id: coin.coin_id(),
            },
        )?;
        ctx.insert(coin_spend);
        Ok(())
    }
}

impl Layer for P2Singleton {
    type Solution = P2SingletonSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_SINGLETON_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2SingletonArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || args.launcher_puzzle_hash != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        Ok(Some(Self {
            launcher_id: args.launcher_id,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2SingletonSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.p2_singleton_puzzle()?,
            args: P2SingletonArgs::new(self.launcher_id),
        };
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for P2Singleton {
    fn tree_hash(&self) -> TreeHash {
        P2SingletonArgs::curry_tree_hash(self.launcher_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2SingletonArgs {
    pub singleton_mod_hash: Bytes32,
    pub launcher_id: Bytes32,
    pub launcher_puzzle_hash: Bytes32,
}

impl P2SingletonArgs {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
            launcher_id,
            launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
        }
    }

    pub fn curry_tree_hash(launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: P2_SINGLETON_PUZZLE_HASH,
            args: Self::new(launcher_id),
        }
        .tree_hash()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2SingletonSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub my_id: Bytes32,
}

pub const P2_SINGLETON_PUZZLE: [u8; 403] = hex!(
    "
    ff02ffff01ff04ffff04ff18ffff04ffff0bffff02ff2effff04ff02ffff04ff
    05ffff04ff2fffff04ffff02ff3effff04ff02ffff04ffff04ff05ffff04ff0b
    ff178080ff80808080ff808080808080ff5f80ff808080ffff04ffff04ff2cff
    ff01ff248080ffff04ffff04ff10ffff04ff5fff808080ff80808080ffff04ff
    ff01ffffff463fff02ff3c04ffff01ff0102ffff02ffff03ff05ffff01ff02ff
    16ffff04ff02ffff04ff0dffff04ffff0bff3affff0bff12ff3c80ffff0bff3a
    ffff0bff3affff0bff12ff2a80ff0980ffff0bff3aff0bffff0bff12ff808080
    8080ff8080808080ffff010b80ff0180ffff0bff3affff0bff12ff1480ffff0b
    ff3affff0bff3affff0bff12ff2a80ff0580ffff0bff3affff02ff16ffff04ff
    02ffff04ff07ffff04ffff0bff12ff1280ff8080808080ffff0bff12ff808080
    8080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02
    ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ff
    ff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_SINGLETON_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "40f828d8dd55603f4ff9fbf6b73271e904e69406982f4fbefae2c8dcceaf9834"
));

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_puzzles::{singleton::SingletonSolution, EveProof, Proof};
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;

    use super::*;

    use crate::{assert_puzzle_hash, Launcher, SingletonLayer, SpendWithConditions, StandardLayer};

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_SINGLETON_PUZZLE => P2_SINGLETON_PUZZLE_HASH);
        Ok(())
    }

    #[test]
    fn test_p2_singleton_layer() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let (sk, pk, puzzle_hash, coin) = sim.new_p2(2)?;
        let p2 = StandardLayer::new(pk);

        let launcher = Launcher::new(coin.coin_id(), 1);
        let launcher_id = launcher.coin().coin_id();
        let (create_singleton, singleton) = launcher.spend(ctx, puzzle_hash, ())?;

        let p2_singleton = P2Singleton::new(launcher_id);
        let p2_singleton_hash = p2_singleton.tree_hash().into();

        p2.spend(
            ctx,
            coin,
            create_singleton.create_coin(p2_singleton_hash, 1, vec![launcher_id.into()]),
        )?;

        let p2_coin = Coin::new(coin.coin_id(), p2_singleton_hash, 1);
        p2_singleton.spend(ctx, p2_coin, puzzle_hash)?;

        let inner_solution = p2
            .spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(puzzle_hash, 1, vec![launcher_id.into()])
                    .create_puzzle_announcement(p2_coin.coin_id().into()),
            )?
            .solution;
        let singleton_spend = SingletonLayer::new(launcher_id, p2.construct_puzzle(ctx)?)
            .construct_coin_spend(
                ctx,
                singleton,
                SingletonSolution {
                    lineage_proof: Proof::Eve(EveProof {
                        parent_parent_coin_info: coin.coin_id(),
                        parent_amount: 1,
                    }),
                    amount: singleton.amount,
                    inner_solution,
                },
            )?;
        ctx.insert(singleton_spend);

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }
}
