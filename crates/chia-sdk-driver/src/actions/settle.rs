use chia_puzzle_types::offer::NotarizedPayment;

use crate::{Deltas, DriverError, Id, SpendAction, SpendContext, SpendKind, Spends};

#[derive(Debug, Clone)]
pub struct SettleAction {
    pub id: Option<Id>,
    pub notarized_payment: NotarizedPayment,
}

impl SettleAction {
    pub fn new(id: Option<Id>, notarized_payment: NotarizedPayment) -> Self {
        Self {
            id,
            notarized_payment,
        }
    }
}

impl SpendAction for SettleAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        let amount: u64 = self
            .notarized_payment
            .payments
            .iter()
            .map(|p| p.amount)
            .sum();

        if let Some(id) = self.id {
            deltas.update(id).output += amount;
        } else {
            deltas.update_xch().output += amount;
        }

        if self.id.is_none() {
            deltas.set_xch_needed();
        }
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let spend = if let Some(id) = self.id {
            if let Some(cat) = spends.cats.get_mut(&id) {
                let source = cat.notarized_payment_source(&self.notarized_payment)?;
                &mut cat.items[source].kind
            } else if let Some(nft) = spends.nfts.get_mut(&id) {
                let source = nft.last_mut()?;
                &mut source.kind
            } else if let Some(option) = spends.options.get_mut(&id) {
                let source = option.last_mut()?;
                &mut source.kind
            } else {
                return Err(DriverError::InvalidAssetId);
            }
        } else {
            let source = spends
                .xch
                .notarized_payment_source(&self.notarized_payment)?;
            &mut spends.xch.items[source].kind
        };

        if let SpendKind::Settlement(spend) = spend {
            spend.add_notarized_payment(self.notarized_payment.clone());
        } else {
            return Err(DriverError::CannotSettleFromSpend);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_protocol::{Bytes32, Coin};
    use chia_puzzle_types::{offer::Payment, Memos};
    use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::{payment_assertion, tree_hash_notarized_payment, Conditions};
    use indexmap::indexmap;

    use crate::{Action, StandardLayer, BURN_PUZZLE_HASH};

    use super::*;

    #[test]
    fn test_action_settle_xch() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);

        let notarized_payment = NotarizedPayment::new(
            Bytes32::new([42; 32]),
            vec![Payment::new(BURN_PUZZLE_HASH, 1, Memos::None)],
        );
        let hashed_notarized_payment = tree_hash_notarized_payment(&ctx, &notarized_payment);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::send_xch(alice.puzzle_hash, 1, Memos::None),
                Action::settle_xch(notarized_payment),
            ],
        )?;

        spends.finish_with_keys(
            &mut ctx,
            &deltas,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        StandardLayer::new(alice.pk).spend(
            &mut ctx,
            Coin::new(alice.coin.coin_id(), alice.puzzle_hash, 1),
            Conditions::new()
                .create_coin(alice.puzzle_hash, 1, Memos::None)
                .with(payment_assertion(
                    SETTLEMENT_PAYMENT_HASH.into(),
                    hashed_notarized_payment,
                )),
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
