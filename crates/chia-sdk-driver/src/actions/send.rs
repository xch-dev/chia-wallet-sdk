use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_sdk_types::Conditions;

use crate::{Deltas, DriverError, Id, Output, SpendAction, SpendContext, SpendKind, Spends};

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
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        deltas.update(self.id).output += self.amount;

        if self.id.is_none() {
            deltas.set_xch_needed();
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let output = Output::new(self.puzzle_hash, self.amount);

        let spend = if let Some(id) = self.id {
            if let Some(cat) = spends.cats.get_mut(&id) {
                let source = cat.output_source(ctx, &output)?;
                &mut cat.items[source].kind
            } else if let Some(did) = spends.dids.get_mut(&id) {
                let source = did.last_mut()?;
                source.child_info.destination = Some((self.puzzle_hash, self.memos));
                return Ok(());
            } else if let Some(nft) = spends.nfts.get_mut(&id) {
                let source = nft.last_mut()?;
                source.child_info.destination = Some((self.puzzle_hash, self.memos));
                return Ok(());
            } else if let Some(option) = spends.options.get_mut(&id) {
                let source = option.last_mut()?;
                source.child_info.destination = Some((self.puzzle_hash, self.memos));
                return Ok(());
            } else {
                return Err(DriverError::InvalidAssetId);
            }
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
    use chia_puzzle_types::standard::StandardArgs;
    use chia_sdk_test::{BlsPair, Simulator};
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
        spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert_eq!(
            sim.unspent_coins(alice.puzzle_hash, false)
                .iter()
                .fold(0, |acc, coin| acc + coin.amount),
            1
        );

        Ok(())
    }

    #[test]
    fn test_action_send_with_change() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);
        let bob = BlsPair::new(0);
        let bob_puzzle_hash = StandardArgs::curry_tree_hash(bob.pk).into();

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));
        let deltas = spends.apply(
            &mut ctx,
            &[Action::send_xch(bob_puzzle_hash, 2, Memos::None)],
        )?;
        spends.create_change(&mut ctx, &deltas, alice.puzzle_hash)?;
        spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert_eq!(
            sim.unspent_coins(alice.puzzle_hash, false)
                .iter()
                .fold(0, |acc, coin| acc + coin.amount),
            3
        );

        assert_eq!(
            sim.unspent_coins(bob_puzzle_hash, false)
                .iter()
                .fold(0, |acc, coin| acc + coin.amount),
            2
        );

        Ok(())
    }
}
