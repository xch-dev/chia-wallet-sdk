use chia_sdk_types::Conditions;

use crate::{Delta, Deltas, DriverError, Id, Spend, SpendAction, SpendContext, SpendKind, Spends};

/// Created by [`Action::run_tail`](crate::Action::run_tail).
#[derive(Debug, Clone, Copy)]
pub struct RunTailAction {
    pub id: Id,
    /// The spend of the CAT's TAIL, which must allow the change in supply.
    pub tail_spend: Spend,
    /// The amount issued (input) and melted into XCH (output).
    pub supply_delta: Delta,
}

impl RunTailAction {
    pub fn new(id: Id, tail_spend: Spend, supply_delta: Delta) -> Self {
        Self {
            id,
            tail_spend,
            supply_delta,
        }
    }
}

impl SpendAction for RunTailAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        *deltas.update(Id::Xch) += -self.supply_delta;
        *deltas.update(self.id) += self.supply_delta;
        deltas.set_needed(self.id);

        // Melted value has to be returned as change through an XCH coin in the transaction.
        if self.supply_delta.output > self.supply_delta.input {
            deltas.set_needed(Id::Xch);
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let cat = spends
            .cats
            .get_mut(&self.id)
            .ok_or(DriverError::InvalidAssetId)?;

        let source_index = cat.run_tail_source(ctx)?;
        let source = &mut cat.items[source_index];

        match &mut source.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(
                    Conditions::new()
                        .run_cat_tail(self.tail_spend.puzzle, self.tail_spend.solution),
                );
            }
            SpendKind::Settlement(_) => {
                return Err(DriverError::CannotEmitConditions);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use anyhow::Result;
    use chia_protocol::Bytes32;
    use chia_puzzle_types::cat::EverythingWithSignatureTailArgs;
    use chia_sdk_test::Simulator;
    use clvmr::NodePtr;
    use indexmap::indexmap;
    use rstest::rstest;

    use crate::{Action, Relation};

    use super::*;

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_melt_cat(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;
        let tail_spend = Spend::new(tail, NodePtr::NIL);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::issue_cat(tail_spend, hidden_puzzle_hash, 1),
                Action::run_tail(Id::New(0), tail_spend, Delta::new(0, 1)),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        // TODO: Filter outputs better
        let coin = outputs
            .xch
            .iter()
            .find(|c| c.puzzle_hash == alice.puzzle_hash)
            .expect("missing coin");
        assert_ne!(sim.coin_state(coin.coin_id()), None);
        assert_eq!(coin.amount, 1);

        Ok(())
    }

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_melt_cat_separate_spends(
        #[case] hidden_puzzle_hash: Option<Bytes32>,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;
        let tail_spend = Spend::new(tail, NodePtr::NIL);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::issue_cat(tail_spend, hidden_puzzle_hash, 1)],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let cat = outputs.cats[&Id::New(0)][0];

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(sim.new_coin(alice.puzzle_hash, 0));
        spends.add(cat);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::run_tail(
                Id::Existing(cat.info.asset_id),
                tail_spend,
                Delta::new(0, 1),
            )],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let coin = outputs.xch[0];
        assert_ne!(sim.coin_state(coin.coin_id()), None);
        assert_eq!(coin.puzzle_hash, alice.puzzle_hash);
        assert_eq!(coin.amount, 1);

        Ok(())
    }

    #[test]
    fn test_action_melt_cat_requires_xch() {
        let deltas = Deltas::from_actions(&[Action::run_tail(
            Id::Existing(Bytes32::default()),
            Spend::new(NodePtr::NIL, NodePtr::NIL),
            Delta::new(0, 1),
        )]);
        assert!(deltas.is_needed(&Id::Xch));

        let deltas = Deltas::from_actions(&[Action::run_tail(
            Id::Existing(Bytes32::default()),
            Spend::new(NodePtr::NIL, NodePtr::NIL),
            Delta::new(1, 0),
        )]);
        assert!(!deltas.is_needed(&Id::Xch));
        assert_eq!(deltas.get(&Id::Xch), Some(&Delta::new(0, 1)));
    }

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_run_tail_increase_supply(
        #[case] hidden_puzzle_hash: Option<Bytes32>,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;
        let tail_spend = Spend::new(tail, NodePtr::NIL);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::issue_cat(tail_spend, hidden_puzzle_hash, 1)],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let cat = outputs.cats[&Id::New(0)][0];
        let id = Id::Existing(cat.info.asset_id);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(sim.new_coin(alice.puzzle_hash, 5));
        spends.add(cat);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::run_tail(id, tail_spend, Delta::new(5, 0))],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert!(outputs.xch.is_empty());

        let cats = &outputs.cats[&id];
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].coin.amount, 6);
        assert_eq!(cats[0].info.p2_puzzle_hash, alice.puzzle_hash);
        assert!(
            sim.coin_state(cats[0].coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        Ok(())
    }
}
