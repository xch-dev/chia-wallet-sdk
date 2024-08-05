use chia_protocol::{Bytes32, Coin, CoinSpend};
use clvm_traits::ToNodePtr;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{CATLayer, ParseError, PuzzleLayer, TransparentLayer};

#[derive(Debug, Clone, Copy)]
pub struct CAT {
    pub coin: Coin,

    pub asset_id: Bytes32,

    // innermost (owner) layer
    pub p2_puzzle_hash: TreeHash,
    pub p2_puzzle: Option<NodePtr>,
}

impl CAT {
    pub fn new(
        coin: Coin,
        asset_id: Bytes32,
        p2_puzzle_hash: TreeHash,
        p2_puzzle: Option<NodePtr>,
    ) -> Self {
        CAT {
            coin,
            asset_id,
            p2_puzzle_hash,
            p2_puzzle,
        }
    }

    pub fn with_coin(mut self, coin: Coin) -> Self {
        self.coin = coin;
        self
    }

    pub fn with_p2_puzzle(mut self, p2_puzzle: NodePtr) -> Self {
        self.p2_puzzle = Some(p2_puzzle);
        self
    }

    pub fn from_parent_spend(
        allocator: &mut Allocator,
        cs: CoinSpend,
    ) -> Result<Option<Self>, ParseError> {
        let puzzle_ptr = cs
            .puzzle_reveal
            .to_node_ptr(allocator)
            .map_err(|err| ParseError::ToClvm(err))?;
        let solution_ptr = cs
            .solution
            .to_node_ptr(allocator)
            .map_err(|err| ParseError::ToClvm(err))?;

        let res =
            CATLayer::<TransparentLayer>::from_parent_spend(allocator, puzzle_ptr, solution_ptr)?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(CAT {
                coin: Coin::new(cs.coin.coin_id(), res.tree_hash().into(), 1),
                asset_id: res.asset_id,
                p2_puzzle_hash: res.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn from_puzzle(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let res = CATLayer::<TransparentLayer>::from_puzzle(allocator, puzzle)?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(CAT {
                coin,
                asset_id: res.asset_id,
                p2_puzzle_hash: res.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn get_layered_object(&self, p2_puzzle: Option<NodePtr>) -> CATLayer<TransparentLayer> {
        CATLayer {
            asset_id: self.asset_id,
            inner_puzzle: TransparentLayer {
                puzzle_hash: self.p2_puzzle_hash,
                puzzle: match self.p2_puzzle {
                    Some(p2_puzzle) => Some(p2_puzzle),
                    None => p2_puzzle,
                },
            },
        }
    }

    // pub fn spend(
    //     &self,
    //     ctx: &mut SpendContext,
    //     lineage_proof: Proof,
    //     inner_spend: Spend,
    // ) -> Result<(CoinSpend, NFT<M>, Proof), ParseError>
    // where
    //     M: Clone + ToTreeHash,
    // {
    //     let thing = self.get_layered_object(Some(inner_spend.puzzle()));

    //     let puzzle_ptr = thing.construct_puzzle(ctx)?;
    //     let puzzle = Program::from_node_ptr(ctx.allocator(), puzzle_ptr)
    //         .map_err(|err| ParseError::FromClvm(err))?;

    //     let solution_ptr = thing.construct_solution(
    //         ctx,
    //         SingletonLayerSolution {
    //             lineage_proof: lineage_proof,
    //             amount: self.coin.amount,
    //             inner_solution: NFTStateLayerSolution {
    //                 inner_solution: NFTOwnershipLayerSolution {
    //                     inner_solution: inner_spend.solution(),
    //                 },
    //             },
    //         },
    //     )?;
    //     let solution = Program::from_node_ptr(ctx.allocator(), solution_ptr)
    //         .map_err(|err| ParseError::FromClvm(err))?;

    //     let cs = CoinSpend {
    //         coin: self.coin,
    //         puzzle_reveal: puzzle,
    //         solution,
    //     };
    //     let lineage_proof = thing.lineage_proof_for_child(self.coin.parent_coin_info, 1);
    //     Ok((
    //         cs.clone(),
    //         NFT::from_parent_spend(ctx.allocator_mut(), cs)?.ok_or(ParseError::MissingChild)?,
    //         Proof::Lineage(lineage_proof),
    //     ))
    // }
}
