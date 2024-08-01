use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::{
    singleton::{
        SingletonArgs, SingletonSolution, SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    },
    LineageProof, Proof,
};
use clvm_traits::{FromClvm, FromNodePtr, ToClvm, ToNodePtr};
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{OuterPuzzleLayer, ParseError, Puzzle, PuzzleLayer, SpendContext};

#[derive(Debug)]
pub struct SingletonLayer<IP> {
    pub launcher_id: Bytes32,
    pub inner_puzzle: IP,
}

#[derive(Debug, ToClvm, FromClvm)]
#[clvm(list)]
pub struct SingletonLayerSolution<I>
where
    I: ToClvm<NodePtr> + FromClvm<NodePtr>,
{
    pub lineage_proof: Proof,
    pub amount: u64,
    pub inner_solution: I,
}

impl<IP, IS> PuzzleLayer<SingletonLayerSolution<IS>> for SingletonLayer<IP>
where
    IP: PuzzleLayer<IS>,
    IS: FromClvm<NodePtr> + ToClvm<NodePtr>,
{
    fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = SingletonArgs::<NodePtr>::from_clvm(allocator, parent_puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        if parent_args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || parent_args.singleton_struct.launcher_puzzle_hash
                != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(ParseError::InvalidSingletonStruct);
        }

        let solution = SingletonSolution::<NodePtr>::from_clvm(allocator, layer_solution)
            .map_err(|err| ParseError::FromClvm(err))?;

        match IP::from_parent_spend(allocator, parent_args.inner_puzzle, solution.inner_solution)? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(SingletonLayer::<IP> {
                launcher_id: parent_args.singleton_struct.launcher_id,
                inner_puzzle,
            })),
        }
    }

    fn from_puzzle(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = SingletonArgs::<NodePtr>::from_clvm(allocator, puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        if args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || args.singleton_struct.launcher_puzzle_hash != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(ParseError::InvalidSingletonStruct);
        }

        match IP::from_puzzle(allocator, args.inner_puzzle)? {
            None => return Ok(None),
            Some(inner_puzzle) => Ok(Some(SingletonLayer::<IP> {
                launcher_id: args.singleton_struct.launcher_id,
                inner_puzzle,
            })),
        }
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, ParseError> {
        CurriedProgram {
            program: ctx
                .singleton_top_layer()
                .map_err(|err| ParseError::Spend(err))?,
            args: SingletonArgs {
                singleton_struct: SingletonStruct {
                    mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                    launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    launcher_id: self.launcher_id,
                },
                inner_puzzle: self.inner_puzzle.construct_puzzle(ctx)?,
            },
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: SingletonLayerSolution<IS>,
    ) -> Result<NodePtr, ParseError> {
        SingletonSolution {
            lineage_proof: solution.lineage_proof,
            amount: solution.amount,
            inner_solution: self
                .inner_puzzle
                .construct_solution(ctx, solution.inner_solution)?,
        }
        .to_node_ptr(ctx.allocator_mut())
        .map_err(|err| ParseError::ToClvm(err))
    }
}

impl<IP, IS> OuterPuzzleLayer<SingletonLayerSolution<IS>> for SingletonLayer<IP>
where
    IP: PuzzleLayer<IS>,
    IS: FromClvm<NodePtr> + ToClvm<NodePtr>,
{
    fn solve(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        solution: SingletonLayerSolution<IS>,
    ) -> Result<CoinSpend, ParseError> {
        let puzzle_ptr = self.construct_puzzle(ctx)?;
        let puzzle_reveal = Program::from_node_ptr(ctx.allocator(), puzzle_ptr)
            .map_err(|err| ParseError::FromClvm(err))?;

        let solution_ptr = self.construct_solution(ctx, solution)?;
        let solution_reveal = Program::from_node_ptr(ctx.allocator(), solution_ptr)
            .map_err(|err| ParseError::FromClvm(err))?;

        Ok(CoinSpend {
            coin,
            puzzle_reveal,
            solution: solution_reveal,
        })
    }
}

impl<IP> ToTreeHash for SingletonLayer<IP>
where
    IP: ToTreeHash,
{
    fn tree_hash(&self) -> TreeHash {
        CurriedProgram {
            program: SINGLETON_TOP_LAYER_PUZZLE_HASH,
            args: SingletonArgs {
                singleton_struct: SingletonStruct {
                    mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                    launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    launcher_id: self.launcher_id,
                },
                inner_puzzle: self.inner_puzzle.tree_hash(),
            },
        }
        .tree_hash()
    }
}

impl<IP> SingletonLayer<IP>
where
    IP: ToTreeHash,
{
    pub fn lineage_proof_for_child(&self, my_parent_coin: Coin) -> LineageProof {
        LineageProof {
            parent_parent_coin_id: my_parent_coin.parent_coin_info,
            parent_inner_puzzle_hash: self.inner_puzzle.tree_hash().into(),
            parent_amount: my_parent_coin.amount,
        }
    }

    pub fn lineage_proof_from_parent_spend(
        allocator: &Allocator,
        parent_coin: Coin,
        parent_puzzle: NodePtr,
    ) -> Result<Option<LineageProof>, ParseError> {
        let parent_puzzle = Puzzle::parse(allocator, parent_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = SingletonArgs::<NodePtr>::from_clvm(allocator, parent_puzzle.args)
            .map_err(|err| ParseError::FromClvm(err))?;

        if parent_args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || parent_args.singleton_struct.launcher_puzzle_hash
                != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(ParseError::InvalidSingletonStruct);
        }

        Ok(Some(LineageProof {
            parent_parent_coin_id: parent_coin.parent_coin_info,
            parent_inner_puzzle_hash: tree_hash(&allocator, parent_args.inner_puzzle).into(),
            parent_amount: parent_coin.amount,
        }))
    }
}
