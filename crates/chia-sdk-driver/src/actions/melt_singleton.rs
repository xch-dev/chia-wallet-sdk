use crate::{
    Delta, Deltas, DriverError, Id, SingletonDestination, SpendAction, SpendContext, Spends,
    check_singleton_amount,
};

/// Created by [`Action::melt_singleton`](crate::Action::melt_singleton).
#[derive(Debug, Clone, Copy)]
pub struct MeltSingletonAction {
    pub id: Id,
    /// The amount of the singleton, which is returned to the transaction as XCH.
    pub amount: u64,
}

impl MeltSingletonAction {
    pub fn new(id: Id, amount: u64) -> Self {
        Self { id, amount }
    }
}

impl SpendAction for MeltSingletonAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        deltas.set_needed(self.id);
        *deltas.update(self.id) += Delta::new(0, self.amount);
        *deltas.update(Id::Xch) += Delta::new(self.amount, 0);

        // The singleton can't create an odd coin alongside the melt condition, so the melted value
        // must be returned as change through an XCH coin in the transaction.
        if self.amount > 0 {
            deltas.set_needed(Id::Xch);
        }
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        if let Some(did) = spends.dids.get_mut(&self.id) {
            let source = did.last_mut()?;
            check_singleton_amount(source.asset.coin.amount, self.amount)?;
            source.child_info.destination = Some(SingletonDestination::Melt);
        } else if let Some(option) = spends.options.get_mut(&self.id) {
            let source = option.last_mut()?;
            check_singleton_amount(source.asset.coin.amount, self.amount)?;
            source.child_info.destination = Some(SingletonDestination::Melt);
        } else {
            return Err(DriverError::InvalidAssetId);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_protocol::Bytes32;
    use chia_puzzle_types::{
        Memos,
        offer::{NotarizedPayment, Payment},
    };
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;
    use rstest::rstest;

    use crate::{Action, Cat, CatSpend, OptionType, OptionUnderlying, Relation, SingletonInfo};

    use super::*;

    #[test]
    fn test_action_melt_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::melt_singleton(Id::New(0), 1),
            ],
        )?;

        spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_action_melt_singleton_amount_mismatch() {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let result = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::melt_singleton(Id::New(0), 3),
            ],
        );

        assert!(matches!(result, Err(DriverError::SingletonAmountMismatch)));
    }

    #[test]
    fn test_action_melt_nft_is_rejected() {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let result = spends.apply(
            &mut ctx,
            &[
                Action::mint_empty_nft(),
                Action::melt_singleton(Id::New(0), 1),
            ],
        );

        assert!(matches!(result, Err(DriverError::InvalidAssetId)));
    }

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_exercise_option(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);
        let bob = sim.bls(1);
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::single_issue_cat(hidden_puzzle_hash, 1),
                Action::mint_option(
                    alice.puzzle_hash,
                    10,
                    Id::New(0),
                    1,
                    OptionType::Xch { amount: 1 },
                    1,
                ),
                Action::send(Id::New(1), bob.puzzle_hash, 1, bob_hint),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        let underlying_cat = outputs.cats[&Id::New(0)][0];
        let option = outputs.options[&Id::New(1)];

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let underlying = OptionUnderlying::new(
            option.info.launcher_id,
            alice.puzzle_hash,
            10,
            1,
            OptionType::Xch { amount: 1 },
        );

        let underlying_spend =
            underlying.exercise_spend(&mut ctx, option.info.inner_puzzle_hash().into(), 1)?;

        let settlement_cats =
            Cat::spend_all(&mut ctx, &[CatSpend::new(underlying_cat, underlying_spend)])?;

        let mut spends = Spends::new(bob.puzzle_hash);
        spends.add(bob.coin);
        spends.add(option);
        spends.add(settlement_cats[0]);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::melt_singleton(Id::Existing(option.info.launcher_id), 1),
                Action::settle(
                    Id::Xch,
                    NotarizedPayment::new(
                        option.info.launcher_id,
                        vec![Payment::new(alice.puzzle_hash, 1, Memos::None)],
                    ),
                ),
            ],
        )?;

        spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { bob.puzzle_hash => bob.pk },
        )?;

        sim.spend_coins(ctx.take(), &[bob.sk])?;

        Ok(())
    }
}
