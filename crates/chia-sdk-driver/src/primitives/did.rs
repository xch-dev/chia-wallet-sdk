use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::{LineageProof, Proof};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DidLayer, DidLayerSolution, DriverError, Layer, Primitive, Puzzle, SingletonLayer,
    SingletonLayerSolution, Spend, SpendContext, TransparentLayer,
};

use super::DidInfo;

#[derive(Debug, Clone, Copy)]
pub struct Did<M> {
    pub info: DidInfo<M>,
    pub coin: Coin,
    pub proof: Proof,
}

impl<M> Primitive for Did<M> {
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: NodePtr,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let res = SingletonLayer::<DidLayer<M, TransparentLayer<true>>>::from_parent_spend(
            allocator,
            parent_puzzle,
            parent_solution,
        )?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Did {
                coin: Coin::new(cs.coin.coin_id(), res.tree_hash().into(), 1),
                launcher_id: res.launcher_id,
                recovery_list_hash: res.inner_puzzle.recovery_list_hash,
                num_verifications_required: res.inner_puzzle.num_verifications_required,
                metadata: res.inner_puzzle.metadata,
                p2_puzzle_hash: res.inner_puzzle.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.inner_puzzle.puzzle,
            })),
        }
    }
}

/*
  /*
     fn from_parent_spend(
        allocator: &mut Allocator,
        layer_puzzle: NodePtr,
        layer_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

        let Some(parent_puzzle) = parent_puzzle.as_curried() else {
            return Ok(None);
        };

        if parent_puzzle.mod_hash != DID_INNER_PUZZLE_HASH {
            return Ok(None);
        }

        let parent_args = DidArgs::<NodePtr, M>::from_clvm(allocator, parent_puzzle.args)
            .map_err(DriverError::FromClvm)?;

        let parent_inner_solution =
            match DidSolution::<NodePtr>::from_clvm(allocator, layer_solution)
                .map_err(DriverError::FromClvm)?
            {
                DidSolution::Spend(inner_solution) => inner_solution,
            };

        match IP::from_parent_spend(allocator, parent_args.inner_puzzle, parent_inner_sol)? {
            None => Ok(None),
            Some(inner_puzzle) => Ok(Some(DidLayer::<M, IP> {
                launcher_id: parent_args.singleton_struct.launcher_id,
                recovery_list_hash: parent_args.recovery_list_hash,
                num_verifications_required: parent_args.num_verifications_required,
                metadata: parent_args.metadata,
                inner_puzzle,
            })),
        }
    } */

    pub fn from_parent_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
    ) -> Result<Option<Self>, DriverError>
    where
        M: ToTreeHash,
    {
        let puzzle_ptr = cs
            .puzzle_reveal
            .to_clvm(allocator)
            .map_err(DriverError::ToClvm)?;
        let solution_ptr = cs
            .solution
            .to_clvm(allocator)
            .map_err(DriverError::ToClvm)?;

        let res = SingletonLayer::<DidLayer<M, TransparentLayer<true>>>::from_parent_spend(
            allocator,
            puzzle_ptr,
            solution_ptr,
        )?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Did {
                coin: Coin::new(cs.coin.coin_id(), res.tree_hash().into(), 1),
                launcher_id: res.launcher_id,
                recovery_list_hash: res.inner_puzzle.recovery_list_hash,
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
        let res =
            SingletonLayer::<DidLayer<M, TransparentLayer<true>>>::from_puzzle(allocator, puzzle)?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Did {
                coin,
                launcher_id: res.launcher_id,
                recovery_list_hash: res.inner_puzzle.recovery_list_hash,
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
    ) -> SingletonLayer<DidLayer<M, TransparentLayer<true>>>
    where
        M: Clone,
    {
        SingletonLayer {
            launcher_id: self.launcher_id,
            inner_puzzle: DidLayer {
                launcher_id: self.launcher_id,
                recovery_list_hash: self.recovery_list_hash,
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
    ) -> Result<(CoinSpend, Did<M>, Proof), DriverError>
    where
        M: Clone + ToTreeHash,
    {
        let thing = self.get_layered_object(Some(inner_spend.puzzle()));

        let puzzle_ptr = thing.construct_puzzle(ctx)?;
        let puzzle =
            Program::from_clvm(ctx.allocator(), puzzle_ptr).map_err(DriverError::FromClvm)?;

        let solution_ptr = thing.construct_solution(
            ctx,
            SingletonLayerSolution {
                lineage_proof,
                amount: self.coin.amount,
                inner_solution: DidLayerSolution {
                    inner_solution: inner_spend.solution(),
                },
            },
        )?;
        let solution =
            Program::from_clvm(ctx.allocator(), solution_ptr).map_err(DriverError::FromClvm)?;

        let cs = CoinSpend {
            coin: self.coin,
            puzzle_reveal: puzzle,
            solution,
        };
        let lineage_proof = thing.lineage_proof_for_child(self.coin.parent_coin_info, 1);

        Ok((
            cs.clone(),
            Did::from_parent_spend(ctx.allocator_mut(), &cs)?.ok_or(DriverError::MissingChild)?,
            Proof::Lineage(lineage_proof),
        ))
    }


impl<M> DidInfo<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + Clone + ToTreeHash,
{
    pub fn singleton_inner_puzzle_hash(&self) -> TreeHash {
        self.get_layered_object(None).inner_puzzle_hash()
    }

    pub fn lineage_proof_for_child(
        &self,
        my_parent_name: Bytes32,
        my_parent_amount: u64,
    ) -> LineageProof {
        self.get_layered_object(None)
            .lineage_proof_for_child(my_parent_name, my_parent_amount)
    }
}

impl<M> DidInfo<M>
where
    M: ToTreeHash,
{
    pub fn compute_new_did_layer_puzzle_hash(&self, new_inner_puzzle_hash: TreeHash) -> TreeHash {
        DidLayer::<M, ()>::wrap_inner_puzzle_hash(
            self.launcher_id,
            self.recovery_list_hash,
            self.num_verifications_required,
            self.metadata.tree_hash(),
            new_inner_puzzle_hash,
        )
    }
}*/

#[cfg(test)]
mod tests {
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{secret_key, test_transaction, Simulator};

    use crate::{Conditions, Launcher};

    use super::*;

    #[tokio::test]
    async fn test_did_recreation() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let (create_did, mut did, mut did_proof) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        for _ in 0..10 {
            (did, did_proof) = ctx.spend_standard_did(&did, did_proof, pk, Conditions::new())?;
        }

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        let coin_state = sim
            .coin_state(did.coin.coin_id())
            .await
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did.coin);

        Ok(())
    }
}
