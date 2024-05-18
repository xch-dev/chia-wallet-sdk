use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::{
    did::{DidArgs, DidSolution},
    singleton::{SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
    LineageProof, Proof,
};
use chia_sdk_types::{conditions::CreateCoinWithMemos, puzzles::DidInfo};
use clvm_traits::ToClvm;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{
    puzzles::{spend_singleton, StandardSpend},
    spend_builder::{ChainedSpend, InnerSpend},
    SpendContext, SpendError,
};

pub struct NoDidOutput;

pub enum DidOutput {
    Recreate,
}

pub struct StandardDidSpend<T> {
    standard_spend: StandardSpend,
    output: T,
}

impl<T> StandardDidSpend<T> {
    pub fn chain(mut self, chained_spend: ChainedSpend) -> Self {
        self.standard_spend = self.standard_spend.chain(chained_spend);
        self
    }

    pub fn condition(mut self, condition: NodePtr) -> Self {
        self.standard_spend = self.standard_spend.condition(condition);
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
        ctx: &mut SpendContext,
        synthetic_key: PublicKey,
        mut did_info: DidInfo<M>,
    ) -> Result<DidInfo<M>, SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let create_coin = match self.output {
            DidOutput::Recreate => CreateCoinWithMemos {
                puzzle_hash: did_info.did_inner_puzzle_hash,
                amount: did_info.coin.amount,
                memos: vec![did_info.p2_puzzle_hash.to_vec().into()],
            },
        };

        let inner_spend = self
            .standard_spend
            .condition(ctx.alloc(create_coin)?)
            .inner_spend(ctx, synthetic_key)?;

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
    ctx: &mut SpendContext,
    did_info: &DidInfo<M>,
    inner_spend: InnerSpend,
) -> Result<CoinSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let did_inner_puzzle = ctx.did_inner_puzzle()?;

    let puzzle = ctx.alloc(CurriedProgram {
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

    let solution = ctx.alloc(DidSolution::InnerSpend(inner_spend.solution()))?;

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
    use chia_sdk_test::TestWallet;
    use clvmr::Allocator;

    use crate::puzzles::{CreateDid, Launcher};

    use super::*;

    #[tokio::test]
    async fn test_did_recreation() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);
        let mut wallet = TestWallet::new(1).await;

        let (create_did, mut did_info) = Launcher::new(wallet.coin.coin_id(), 1)
            .create(ctx)?
            .create_standard_did(ctx, wallet.pk)?;

        StandardSpend::new()
            .chain(create_did)
            .finish(ctx, wallet.coin, wallet.pk)?;

        for _ in 0..10 {
            did_info = StandardDidSpend::new()
                .recreate()
                .finish(ctx, wallet.pk, did_info)?;
        }

        wallet.submit(ctx.take_spends()).await?;

        let coin_state = wallet
            .peer
            .register_for_coin_updates(vec![did_info.coin.coin_id()], 0)
            .await?
            .remove(0);
        assert_eq!(coin_state.coin, did_info.coin);

        Ok(())
    }
}
