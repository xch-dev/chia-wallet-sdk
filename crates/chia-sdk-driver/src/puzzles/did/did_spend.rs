use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::{
    did::{DidArgs, DidSolution},
    singleton::{SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
    LineageProof, Proof,
};
use chia_sdk_types::puzzles::DidInfo;
use clvm_traits::ToClvm;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{
    puzzles::{spend_singleton, StandardSpend},
    spend_builder::{InnerSpend, P2Spend, ParentConditions},
    SpendContext, SpendError,
};

#[derive(Debug, Clone, Copy)]
pub struct NoDidOutput;

#[derive(Debug, Clone, Copy)]
pub enum DidOutput {
    Recreate,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct StandardDidSpend<T> {
    standard_spend: StandardSpend,
    output: T,
}

impl<T> StandardDidSpend<T> {
    pub fn chain(mut self, chained: ParentConditions) -> Self {
        self.standard_spend = self.standard_spend.chain(chained);
        self
    }
}

impl<T> P2Spend for StandardDidSpend<T> {
    fn raw_condition(mut self, condition: NodePtr) -> Self {
        self.standard_spend = self.standard_spend.raw_condition(condition);
        self
    }
}

impl Default for StandardDidSpend<NoDidOutput> {
    fn default() -> Self {
        Self {
            output: NoDidOutput,
            standard_spend: StandardSpend::new(),
        }
    }
}

impl StandardDidSpend<NoDidOutput> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recreate(self) -> StandardDidSpend<DidOutput> {
        StandardDidSpend {
            standard_spend: self.standard_spend,
            output: DidOutput::Recreate,
        }
    }
}

impl StandardDidSpend<DidOutput> {
    pub fn finish<M>(
        self,
        ctx: &mut SpendContext<'_>,
        synthetic_key: PublicKey,
        mut did_info: DidInfo<M>,
    ) -> Result<DidInfo<M>, SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let spend = match self.output {
            DidOutput::Recreate => self.standard_spend.create_hinted_coin(
                ctx,
                did_info.did_inner_puzzle_hash,
                did_info.coin.amount,
                did_info.p2_puzzle_hash,
            )?,
        };

        let inner_spend = spend.inner_spend(ctx, synthetic_key)?;
        let did_spend = raw_did_spend(ctx, &did_info, inner_spend)?;

        ctx.spend(did_spend);

        match self.output {
            DidOutput::Recreate => {
                did_info.proof = Proof::Lineage(LineageProof {
                    parent_parent_coin_id: did_info.coin.parent_coin_info,
                    parent_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
                    parent_amount: did_info.coin.amount,
                });

                did_info.coin = Coin::new(
                    did_info.coin.coin_id(),
                    did_info.coin.puzzle_hash,
                    did_info.coin.amount,
                );
            }
        }

        Ok(did_info)
    }
}

pub fn raw_did_spend<M>(
    ctx: &mut SpendContext<'_>,
    did_info: &DidInfo<M>,
    inner_spend: InnerSpend,
) -> Result<CoinSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let did_inner_puzzle = ctx.did_inner_puzzle()?;

    let puzzle = ctx.alloc(&CurriedProgram {
        program: did_inner_puzzle,
        args: DidArgs {
            inner_puzzle: inner_spend.puzzle(),
            recovery_did_list_hash: did_info.recovery_did_list_hash,
            num_verifications_required: did_info.num_verifications_required,
            singleton_struct: SingletonStruct {
                mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                launcher_id: did_info.launcher_id,
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            },
            metadata: &did_info.metadata,
        },
    })?;

    let solution = ctx.alloc(&DidSolution::InnerSpend(inner_spend.solution()))?;

    let did_spend = InnerSpend::new(puzzle, solution);

    spend_singleton(
        ctx,
        did_info.coin,
        did_info.launcher_id,
        did_info.proof,
        did_spend,
    )
}

#[cfg(test)]
mod tests {
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{test_transaction, Simulator};
    use clvmr::Allocator;

    use crate::puzzles::{CreateDid, Launcher};

    use super::*;

    #[tokio::test]
    async fn test_did_recreation() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, mut did_info) = Launcher::new(coin.coin_id(), 1)
            .create(ctx)?
            .create_standard_did(ctx, pk)?;

        StandardSpend::new()
            .chain(create_did)
            .finish(ctx, coin, pk)?;

        for _ in 0..10 {
            did_info = StandardDidSpend::new()
                .recreate()
                .finish(ctx, pk, did_info)?;
        }

        test_transaction(&peer, ctx.take_spends(), &[sk]).await;

        let coin_state = sim
            .coin_state(did_info.coin.coin_id())
            .await
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did_info.coin);

        Ok(())
    }
}
