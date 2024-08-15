use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::{LineageProof, Proof};
use clvm_traits::{FromClvm, FromNodePtr, ToClvm, ToNodePtr};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DidLayer, DidLayerSolution, DriverError, Layer, SingletonLayer, SingletonLayerSolution, Spend,
    SpendContext, TransparentLayer,
};

#[derive(Debug, Clone, Copy)]
pub struct Did<M = NodePtr> {
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

impl<M> Did<M>
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
        Did {
            coin,
            launcher_id,
            recovery_did_list_hash,
            num_verifications_required,
            metadata,
            p2_puzzle_hash,
            p2_puzzle,
        }
    }

    #[must_use]
    pub fn with_coin(mut self, coin: Coin) -> Self {
        self.coin = coin;
        self
    }

    #[must_use]
    pub fn with_p2_puzzle(mut self, p2_puzzle: NodePtr) -> Self {
        self.p2_puzzle = Some(p2_puzzle);
        self
    }

    pub fn from_parent_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
    ) -> Result<Option<Self>, DriverError>
    where
        M: ToTreeHash,
    {
        let puzzle_ptr = cs
            .puzzle_reveal
            .to_node_ptr(allocator)
            .map_err(DriverError::ToClvm)?;
        let solution_ptr = cs
            .solution
            .to_node_ptr(allocator)
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
        let res =
            SingletonLayer::<DidLayer<M, TransparentLayer<true>>>::from_puzzle(allocator, puzzle)?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Did {
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
    ) -> SingletonLayer<DidLayer<M, TransparentLayer<true>>>
    where
        M: Clone,
    {
        SingletonLayer {
            launcher_id: self.launcher_id,
            inner_puzzle: DidLayer {
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
    ) -> Result<(CoinSpend, Did<M>, Proof), DriverError>
    where
        M: Clone + ToTreeHash,
    {
        let thing = self.get_layered_object(Some(inner_spend.puzzle()));

        let puzzle_ptr = thing.construct_puzzle(ctx)?;
        let puzzle =
            Program::from_node_ptr(ctx.allocator(), puzzle_ptr).map_err(DriverError::FromClvm)?;

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
            Program::from_node_ptr(ctx.allocator(), solution_ptr).map_err(DriverError::FromClvm)?;

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
}

impl<M> Did<M>
where
    M: ToClvm<NodePtr> + FromClvm<NodePtr> + Clone + ToTreeHash,
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

impl<M> Did<M>
where
    M: ToTreeHash,
{
    pub fn compute_new_did_layer_puzzle_hash(&self, new_inner_puzzle_hash: TreeHash) -> TreeHash {
        DidLayer::<M, ()>::wrap_inner_puzzle_hash(
            self.launcher_id,
            self.recovery_did_list_hash,
            self.num_verifications_required,
            self.metadata.tree_hash(),
            new_inner_puzzle_hash,
        )
    }
}

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
