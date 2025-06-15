use chia_protocol::Coin;
use chia_puzzle_types::{
    cat::{CatArgs, GenesisByCoinIdTailArgs},
    Memos,
};
use chia_sdk_types::Conditions;
use clvmr::NodePtr;

use crate::{
    Cat, CatInfo, Deltas, DriverError, FungibleAsset, FungibleSpend, Id, Spend, SpendAction,
    SpendContext, SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub enum TailIssuance {
    Single,
    Multiple(Spend),
}

#[derive(Debug, Clone, Copy)]
pub struct IssueCatAction {
    pub issuance: TailIssuance,
    pub amount: u64,
}

impl IssueCatAction {
    pub fn new(issuance: TailIssuance, amount: u64) -> Self {
        Self { issuance, amount }
    }
}

impl SpendAction for IssueCatAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        deltas.update(None).output += self.amount;
        deltas.update(Some(Id::New(index))).input += self.amount;
        deltas.set_xch_needed();
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

        let p2_puzzle_hash = source.asset.p2_puzzle_hash().into();
        let cat_puzzle_hash = CatArgs::curry_tree_hash(asset_id, p2_puzzle_hash).into();

        match &mut source.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().create_coin(
                    cat_puzzle_hash,
                    self.amount,
                    Memos::None,
                ))?;
            }
        }

        let eve_cat = Cat::new(
            Coin::new(source.asset.coin_id(), cat_puzzle_hash, self.amount),
            None,
            CatInfo::new(asset_id, None, p2_puzzle_hash.into()),
        );

        let id = if spends.cats.contains_key(&Id::Existing(asset_id)) {
            Id::Existing(asset_id)
        } else {
            Id::New(index)
        };

        let mut cat_spend = FungibleSpend::new(eve_cat, source.kind.child(), true);

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
                )?;
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

    use crate::Action;

    use super::*;

    #[test]
    fn test_action_single_issuance_cat() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));

        let deltas = spends.apply(&mut ctx, &[Action::single_issue_cat(1)])?;
        spends.create_change(&mut ctx, &deltas, alice.puzzle_hash)?;

        let outputs =
            spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = outputs.cats[&Id::New(0)][0];
        assert_ne!(sim.coin_state(cat.coin.coin_id()), None);
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cat.coin.amount, 1);

        Ok(())
    }

    #[test]
    fn test_action_multiple_issuance_cat() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));

        let deltas = spends.apply(
            &mut ctx,
            &[Action::issue_cat(Spend::new(tail, NodePtr::NIL), 1)],
        )?;
        spends.create_change(&mut ctx, &deltas, alice.puzzle_hash)?;

        let outputs =
            spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = outputs.cats[&Id::New(0)][0];
        assert_ne!(sim.coin_state(cat.coin.coin_id()), None);
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cat.coin.amount, 1);

        Ok(())
    }
}
