use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::{did::DidSolution, singleton::SingletonSolution, LineageProof, Proof};
use chia_sdk_types::{run_puzzle, Condition};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{DidLayer, DriverError, Layer, Primitive, Puzzle, SingletonLayer, Spend, SpendContext};

use super::DidInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Did<M> {
    pub coin: Coin,
    pub proof: Proof,
    pub info: DidInfo<M>,
}

impl<M> Did<M> {
    pub fn new(coin: Coin, proof: Proof, info: DidInfo<M>) -> Self {
        Self { coin, proof, info }
    }

    /// Creates a coin spend for this DID.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<CoinSpend, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let layers = self.info.clone().into_layers(inner_spend.puzzle);

        let puzzle_ptr = layers.construct_puzzle(ctx)?;
        let solution_ptr = layers.construct_solution(
            ctx,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution: DidSolution::Spend(inner_spend.solution),
            },
        )?;

        let puzzle = ctx.serialize(&puzzle_ptr)?;
        let solution = ctx.serialize(&solution_ptr)?;

        Ok(CoinSpend::new(self.coin, puzzle, solution))
    }

    /// Returns the lineage proof that would be used by the child.
    pub fn child_lineage_proof(&self) -> LineageProof
    where
        M: ToTreeHash,
    {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    /// Creates a new spendable DID for the child, with no modifications.
    #[must_use]
    pub fn recreate_self(&self) -> Self
    where
        M: ToTreeHash + Clone,
    {
        Self {
            coin: Coin::new(self.coin.coin_id(), self.coin.puzzle_hash, self.coin.amount),
            proof: Proof::Lineage(self.child_lineage_proof()),
            info: self.info.clone(),
        }
    }
}

impl<M> Primitive for Did<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + ToTreeHash,
{
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(singleton_layer) =
            SingletonLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        let Some(did_layer) =
            DidLayer::<M, Puzzle>::parse_puzzle(allocator, singleton_layer.inner_puzzle)?
        else {
            return Ok(None);
        };

        if singleton_layer.launcher_id != did_layer.launcher_id {
            return Err(DriverError::InvalidSingletonStruct);
        }

        let singleton_solution =
            SingletonLayer::<NodePtr>::parse_solution(allocator, parent_solution)?;

        let output = run_puzzle(
            allocator,
            singleton_layer.inner_puzzle.ptr(),
            singleton_solution.inner_solution,
        )?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let Some(create_coin) = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .find(|create_coin| create_coin.amount % 2 == 1)
        else {
            return Err(DriverError::MissingChild);
        };

        let Some(hint) = create_coin
            .memos
            .into_iter()
            .find_map(|memo| memo.try_into().ok())
        else {
            return Err(DriverError::MissingHint);
        };

        let parent_inner_puzzle_hash = did_layer.tree_hash().into();
        let layers = SingletonLayer::new(singleton_layer.launcher_id, did_layer);

        let mut info = DidInfo::from_layers(layers);
        info.p2_puzzle_hash = hint;

        Ok(Some(Self {
            coin,
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash,
                parent_amount: parent_coin.amount,
            }),
            info,
        }))
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{secret_key, test_transaction, Simulator};
    use chia_sdk_types::Conditions;

    use crate::Launcher;

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

        let (create_did, mut did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        for _ in 0..10 {
            did = ctx.spend_standard_did(&did, pk, Conditions::new())?;
        }

        test_transaction(&peer, ctx.take_spends(), &[sk], &sim.config().constants).await;

        let coin_state = sim
            .coin_state(did.coin.coin_id())
            .await
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did.coin);

        Ok(())
    }
}
