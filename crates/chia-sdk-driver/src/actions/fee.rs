use crate::{Deltas, DriverError, Id, SpendAction, SpendContext, Spends};

#[derive(Debug, Clone, Copy)]
pub struct FeeAction {
    pub amount: u64,
}

impl FeeAction {
    pub fn new(amount: u64) -> Self {
        Self { amount }
    }
}

impl SpendAction for FeeAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        deltas.update(Id::Xch).output += self.amount;
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        spends.outputs.fee += self.amount;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;

    use crate::{Action, Relation};

    use super::*;

    #[test]
    fn test_action_send_with_fee() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
                Action::fee(1),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let coin = outputs.xch[0];
        assert_eq!(outputs.xch.len(), 1);
        assert_ne!(sim.coin_state(coin.coin_id()), None);
        assert_eq!(coin.amount, 1);

        Ok(())
    }
}
