use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{cat::GenesisByCoinIdTailArgs, Memos};
use chia_sdk_types::{conditions::CreateCoin, Conditions};
use clvmr::NodePtr;

use crate::{
    Asset, Cat, CatInfo, Deltas, DriverError, FungibleSpend, Id, Spend, SpendAction, SpendContext,
    SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub enum TailIssuance {
    Single,
    Multiple(Spend),
}

#[derive(Debug, Clone, Copy)]
pub struct IssueCatAction {
    pub issuance: TailIssuance,
    pub hidden_puzzle_hash: Option<Bytes32>,
    pub amount: u64,
}

impl IssueCatAction {
    pub fn new(issuance: TailIssuance, hidden_puzzle_hash: Option<Bytes32>, amount: u64) -> Self {
        Self {
            issuance,
            hidden_puzzle_hash,
            amount,
        }
    }
}

impl SpendAction for IssueCatAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        deltas.update(Id::New(index)).input += self.amount;
        deltas.update(Id::Xch).output += self.amount;
        deltas.set_needed(Id::Xch);
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        let asset_id = match self.issuance {
            TailIssuance::Single => None,
            TailIssuance::Multiple(spend) => Some(ctx.tree_hash(spend.puzzle).into()),
        };

        let source_index = spends.xch.cat_issuance_source(ctx, asset_id, self.amount)?;
        let source = &mut spends.xch.items[source_index];

        let asset_id = asset_id.unwrap_or_else(|| {
            GenesisByCoinIdTailArgs::curry_tree_hash(source.asset.coin_id()).into()
        });

        let cat_info = CatInfo::new(
            asset_id,
            self.hidden_puzzle_hash,
            source.asset.p2_puzzle_hash(),
        );

        let create_coin = CreateCoin::new(cat_info.puzzle_hash().into(), self.amount, Memos::None);
        let parent_puzzle_hash = source.asset.full_puzzle_hash();

        source.kind.create_coin_with_assertion(
            ctx,
            parent_puzzle_hash,
            &mut spends.xch.payment_assertions,
            create_coin,
        );

        let eve_cat = Cat::new(
            Coin::new(
                source.asset.coin_id(),
                cat_info.puzzle_hash().into(),
                self.amount,
            ),
            None,
            cat_info,
        );

        let id = if spends.cats.contains_key(&Id::Existing(asset_id)) {
            Id::Existing(asset_id)
        } else {
            Id::New(index)
        };

        let mut cat_spend = FungibleSpend::new(eve_cat, true);

        let tail_spend = match self.issuance {
            TailIssuance::Single => {
                let puzzle = ctx.curry(GenesisByCoinIdTailArgs::new(source.asset.coin_id()))?;
                Spend::new(puzzle, NodePtr::NIL)
            }
            TailIssuance::Multiple(spend) => spend,
        };

        match &mut cat_spend.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(
                    Conditions::new().run_cat_tail(tail_spend.puzzle, tail_spend.solution),
                );
            }
            SpendKind::Settlement(_) => {
                return Err(DriverError::CannotEmitConditions);
            }
        }

        spends.cats.entry(id).or_default().items.push(cat_spend);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_puzzle_types::cat::EverythingWithSignatureTailArgs;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;
    use rstest::rstest;

    use crate::{Action, Relation};

    use super::*;

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_single_issuance_cat(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(&mut ctx, &[Action::single_issue_cat(hidden_puzzle_hash, 1)])?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = outputs.cats[&Id::New(0)][0];
        assert_ne!(sim.coin_state(cat.coin.coin_id()), None);
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cat.coin.amount, 1);

        Ok(())
    }

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_multiple_issuance_cat(
        #[case] hidden_puzzle_hash: Option<Bytes32>,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::issue_cat(
                Spend::new(tail, NodePtr::NIL),
                hidden_puzzle_hash,
                1,
            )],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = outputs.cats[&Id::New(0)][0];
        assert_ne!(sim.coin_state(cat.coin.coin_id()), None);
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cat.coin.amount, 1);

        Ok(())
    }
}
