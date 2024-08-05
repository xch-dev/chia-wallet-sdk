use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::Proof;
use clvm_traits::{FromClvm, FromNodePtr, ToClvm, ToNodePtr};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DIDLayer, DIDLayerSolution, DriverError, PuzzleLayer, SingletonLayer, SingletonLayerSolution,
    Spend, SpendContext, TransparentLayer,
};

#[derive(Debug, Clone, Copy)]
pub struct DID<M = NodePtr> {
    pub coin: Coin,

    // singleton layer
    pub launcher_id: Bytes32,

    // DID layer
    pub recovery_did_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,

    // innermost (owner) layer
    pub p2_puzzle_hash: TreeHash,
    pub p2_puzzle: Option<NodePtr>,
}

impl<M> DID<M>
where
    M: ToClvm<NodePtr> + FromClvm<NodePtr>,
{
    pub fn new(
        coin: Coin,
        launcher_id: Bytes32,
        recovery_did_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        p2_puzzle_hash: TreeHash,
        p2_puzzle: Option<NodePtr>,
    ) -> Self {
        DID {
            coin,
            launcher_id,
            recovery_did_list_hash,
            num_verifications_required,
            metadata,
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
    ) -> Result<Option<Self>, DriverError>
    where
        M: ToTreeHash,
    {
        let puzzle_ptr = cs
            .puzzle_reveal
            .to_node_ptr(allocator)
            .map_err(|err| DriverError::ToClvm(err))?;
        let solution_ptr = cs
            .solution
            .to_node_ptr(allocator)
            .map_err(|err| DriverError::ToClvm(err))?;

        let res = SingletonLayer::<DIDLayer<M, TransparentLayer>>::from_parent_spend(
            allocator,
            puzzle_ptr,
            solution_ptr,
        )?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(DID {
                coin: Coin::new(cs.coin.coin_id(), res.tree_hash().into(), 1),
                launcher_id: res.launcher_id,
                recovery_did_list_hash: res.inner_puzzle.recovery_did_list_hash,
                num_verifications_required: res.inner_puzzle.num_verifications_required,
                metadata: res.inner_puzzle.metadata,
                p2_puzzle_hash: res.inner_puzzle.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn from_puzzle(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let res = SingletonLayer::<DIDLayer<M, TransparentLayer>>::from_puzzle(allocator, puzzle)?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(DID {
                coin,
                launcher_id: res.launcher_id,
                recovery_did_list_hash: res.inner_puzzle.recovery_did_list_hash,
                num_verifications_required: res.inner_puzzle.num_verifications_required,
                metadata: res.inner_puzzle.metadata,
                p2_puzzle_hash: res.inner_puzzle.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn get_layered_object(
        &self,
        p2_puzzle: Option<NodePtr>,
    ) -> SingletonLayer<DIDLayer<M, TransparentLayer>>
    where
        M: Clone,
    {
        SingletonLayer {
            launcher_id: self.launcher_id,
            inner_puzzle: DIDLayer {
                launcher_id: self.launcher_id,
                recovery_did_list_hash: self.recovery_did_list_hash,
                num_verifications_required: self.num_verifications_required,
                metadata: self.metadata.clone(),
                inner_puzzle: TransparentLayer {
                    puzzle_hash: self.p2_puzzle_hash,
                    puzzle: match self.p2_puzzle {
                        Some(p2_puzzle) => Some(p2_puzzle),
                        None => p2_puzzle,
                    },
                },
            },
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Proof,
        inner_spend: Spend,
    ) -> Result<(CoinSpend, DID<M>, Proof), DriverError>
    where
        M: Clone + ToTreeHash,
    {
        let thing = self.get_layered_object(Some(inner_spend.puzzle()));

        let puzzle_ptr = thing.construct_puzzle(ctx)?;
        let puzzle = Program::from_node_ptr(ctx.allocator(), puzzle_ptr)
            .map_err(|err| DriverError::FromClvm(err))?;

        let solution_ptr = thing.construct_solution(
            ctx,
            SingletonLayerSolution {
                lineage_proof: lineage_proof,
                amount: self.coin.amount,
                inner_solution: DIDLayerSolution {
                    inner_solution: inner_spend.solution(),
                },
            },
        )?;
        let solution = Program::from_node_ptr(ctx.allocator(), solution_ptr)
            .map_err(|err| DriverError::FromClvm(err))?;

        let cs = CoinSpend {
            coin: self.coin,
            puzzle_reveal: puzzle,
            solution,
        };
        let lineage_proof = thing.lineage_proof_for_child(self.coin.parent_coin_info, 1);

        Ok((
            cs.clone(),
            DID::from_parent_spend(ctx.allocator_mut(), cs)?.ok_or(DriverError::MissingChild)?,
            Proof::Lineage(lineage_proof),
        ))
    }
}
