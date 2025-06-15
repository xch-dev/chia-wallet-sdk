use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_sdk_types::Conditions;

use crate::{DriverError, Id, Output, SpendAction, SpendContext, SpendKind, Spends};

#[derive(Debug, Clone, Copy)]
pub struct SendAction {
    pub id: Option<Id>,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub memos: Memos,
}

impl SendAction {
    pub fn new(id: Option<Id>, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self {
            id,
            puzzle_hash,
            amount,
            memos,
        }
    }
}

impl SpendAction for SendAction {
    fn spend(&self, ctx: &mut SpendContext, spends: &mut Spends) -> Result<(), DriverError> {
        let output = Output::new(self.puzzle_hash, self.amount);

        let spend = if let Some(id) = self.id {
            let Some(cat) = spends.cats.get_mut(&id) else {
                return Err(DriverError::InvalidAssetId);
            };
            let source = cat.output_source(ctx, &output)?;
            &mut cat.items[source].kind
        } else {
            let source = spends.xch.output_source(ctx, &output)?;
            &mut spends.xch.items[source].kind
        };

        match spend {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().create_coin(
                    self.puzzle_hash,
                    self.amount,
                    self.memos,
                ))?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;

    use crate::Action;

    use super::*;

    #[test]
    fn test_action_send() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));

        spends.apply(
            &mut ctx,
            &[Action::send_xch(alice.puzzle_hash, 1, Memos::None)],
        )?;

        spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk})?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
