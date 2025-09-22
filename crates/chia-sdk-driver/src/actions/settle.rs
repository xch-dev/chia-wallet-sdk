use chia_protocol::Coin;
use chia_puzzle_types::offer::NotarizedPayment;
use chia_sdk_types::{payment_assertion, tree_hash_notarized_payment};

use crate::{Deltas, DriverError, Id, SpendAction, SpendContext, SpendKind, Spends};

#[derive(Debug, Clone)]
pub struct SettleAction {
    pub id: Id,
    pub notarized_payment: NotarizedPayment,
}

impl SettleAction {
    pub fn new(id: Id, notarized_payment: NotarizedPayment) -> Self {
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

        deltas.update(self.id).output += amount;
        deltas.set_needed(self.id);
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        if matches!(self.id, Id::Xch) {
            let source = spends
                .xch
                .notarized_payment_source(&self.notarized_payment)?;

            let parent = &mut spends.xch.items[source];

            if let SpendKind::Settlement(spend) = &mut parent.kind {
                spends.xch.payment_assertions.push(payment_assertion(
                    parent.asset.puzzle_hash,
                    tree_hash_notarized_payment(ctx, &self.notarized_payment),
                ));

                spend.add_notarized_payment(self.notarized_payment.clone());
            } else {
                return Err(DriverError::CannotSettleFromSpend);
            }

            for payment in &self.notarized_payment.payments {
                let coin = Coin::new(parent.asset.coin_id(), payment.puzzle_hash, payment.amount);
                spends.outputs.xch.push(coin);
            }
        } else if let Some(cat) = spends.cats.get_mut(&self.id) {
            let source = cat.notarized_payment_source(&self.notarized_payment)?;
            let parent = &mut cat.items[source];

            if let SpendKind::Settlement(spend) = &mut parent.kind {
                cat.payment_assertions.push(payment_assertion(
                    parent.asset.coin.puzzle_hash,
                    tree_hash_notarized_payment(ctx, &self.notarized_payment),
                ));

                spend.add_notarized_payment(self.notarized_payment.clone());
            } else {
                return Err(DriverError::CannotSettleFromSpend);
            }

            for payment in &self.notarized_payment.payments {
                let cat = parent.asset.child(payment.puzzle_hash, payment.amount);
                spends.outputs.cats.entry(self.id).or_default().push(cat);
            }
        } else if let Some(nft) = spends.nfts.get_mut(&self.id) {
            let index = nft.last_or_create_settlement(ctx)?;
            let source = &mut nft.lineage[index];

            if let SpendKind::Settlement(spend) = &mut source.kind {
                source.payment_assertions.push(payment_assertion(
                    source.asset.coin.puzzle_hash,
                    tree_hash_notarized_payment(ctx, &self.notarized_payment),
                ));

                spend.add_notarized_payment(self.notarized_payment.clone());
            } else {
                return Err(DriverError::CannotSettleFromSpend);
            }

            for payment in &self.notarized_payment.payments {
                if payment.amount % 2 == 0 {
                    let coin = Coin::new(
                        source.asset.coin.coin_id(),
                        payment.puzzle_hash,
                        payment.amount,
                    );
                    spends.outputs.xch.push(coin);
                    continue;
                }

                let nft = source.asset.child(
                    payment.puzzle_hash,
                    source.asset.info.current_owner,
                    source.asset.info.metadata,
                    payment.amount,
                );
                spends.outputs.nfts.insert(self.id, nft);
            }
        } else if let Some(option) = spends.options.get_mut(&self.id) {
            let index = option.last_or_create_settlement(ctx)?;
            let source = &mut option.lineage[index];

            if let SpendKind::Settlement(spend) = &mut source.kind {
                source.payment_assertions.push(payment_assertion(
                    source.asset.coin.puzzle_hash,
                    tree_hash_notarized_payment(ctx, &self.notarized_payment),
                ));

                spend.add_notarized_payment(self.notarized_payment.clone());
            } else {
                return Err(DriverError::CannotSettleFromSpend);
            }

            for payment in &self.notarized_payment.payments {
                if payment.amount % 2 == 0 {
                    let coin = Coin::new(
                        source.asset.coin.coin_id(),
                        payment.puzzle_hash,
                        payment.amount,
                    );
                    spends.outputs.xch.push(coin);
                    continue;
                }

                let option = source.asset.child(payment.puzzle_hash, payment.amount);
                spends.outputs.options.insert(self.id, option);
            }
        } else {
            return Err(DriverError::InvalidAssetId);
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

    use crate::{Action, Relation, StandardLayer, BURN_PUZZLE_HASH};

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
                Action::send(Id::Xch, alice.puzzle_hash, 0, Memos::None),
                Action::send(Id::Xch, SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(
            outputs
                .xch
                .iter()
                .find(|c| c.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into())
                .copied()
                .expect("settlement coin not found"),
        );

        let deltas = spends.apply(&mut ctx, &[Action::settle(Id::Xch, notarized_payment)])?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;
        assert_eq!(outputs.xch.len(), 1);

        StandardLayer::new(alice.pk).spend(
            &mut ctx,
            Coin::new(alice.coin.coin_id(), alice.puzzle_hash, 0),
            Conditions::new().with(payment_assertion(
                SETTLEMENT_PAYMENT_HASH.into(),
                hashed_notarized_payment,
            )),
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
