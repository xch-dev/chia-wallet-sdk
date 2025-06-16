use chia_sdk_types::Conditions;

use crate::{Deltas, DriverError, Id, Spend, SpendAction, SpendContext, SpendKind, Spends};

#[derive(Debug, Clone, Copy)]
pub struct MeltCatAction {
    pub id: Id,
    pub tail_spend: Spend,
    pub amount: u64,
}

impl MeltCatAction {
    pub fn new(id: Id, tail_spend: Spend, amount: u64) -> Self {
        Self {
            id,
            tail_spend,
            amount,
        }
    }
}

impl SpendAction for MeltCatAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        deltas.update(None).input += self.amount;
        deltas.update(Some(self.id)).output += self.amount;
        deltas.set_xch_needed();
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
                )?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_puzzle_types::cat::EverythingWithSignatureTailArgs;
    use chia_sdk_test::Simulator;
    use clvmr::NodePtr;
    use indexmap::indexmap;

    use crate::Action;

    use super::*;

    #[test]
    fn test_action_melt_cat() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;
        let tail_spend = Spend::new(tail, NodePtr::NIL);

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::issue_cat(tail_spend, 1),
                Action::melt_cat(Id::New(0), tail_spend, 1),
            ],
        )?;
        spends.create_change(&mut ctx, &deltas, alice.puzzle_hash)?;

        let outputs =
            spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let coin = outputs.xch[0];
        assert_ne!(sim.coin_state(coin.coin_id()), None);
        assert_eq!(coin.puzzle_hash, alice.puzzle_hash);
        assert_eq!(coin.amount, 1);

        Ok(())
    }
}
