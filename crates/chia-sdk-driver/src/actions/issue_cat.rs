use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{Memos, cat::GenesisByCoinIdTailArgs};
use chia_sdk_types::{Conditions, conditions::CreateCoin};
use clvmr::NodePtr;

use crate::{
    Asset, Cat, CatInfo, Delta, Deltas, DriverError, FungibleSpend, Id, Spend, SpendAction,
    SpendContext, SpendKind, Spends,
};

/// The TAIL that a CAT is issued with.
#[derive(Debug, Clone, Copy)]
pub enum TailIssuance {
    /// A TAIL derived from the coin that issues the CAT, so that no more can be issued later.
    Single,
    /// A spend of any TAIL, whose puzzle hash is the asset id.
    Multiple(Spend),
}

/// Created by [`Action::issue_cat`](crate::Action::issue_cat) or
/// [`Action::single_issue_cat`](crate::Action::single_issue_cat).
#[derive(Debug, Clone, Copy)]
pub struct IssueCatAction {
    pub issuance: TailIssuance,
    /// The hidden puzzle hash of the revocation layer, or `None` for a CAT that can't be revoked.
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
        *deltas.update(Id::New(index)) += Delta::new(self.amount, 0);
        *deltas.update(Id::Xch) += Delta::new(0, self.amount);
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

        let cat_info = CatInfo::new(asset_id, self.hidden_puzzle_hash, source.p2_puzzle_hash());

        let create_coin = CreateCoin::new(cat_info.puzzle_hash().into(), self.amount, Memos::None);
        let parent_coin = source.asset.coin();

        source.kind.create_coin_with_assertion(
            ctx,
            parent_coin,
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

        // If coins of this asset are already being spent, the eve CAT joins their ring and change is
        // calculated for them together. The issued amount is only in the deltas under `Id::New(index)`,
        // so the eve CAT is treated as selected (non-ephemeral) to account for it in the change.
        let merged = spends.cats.contains_key(&Id::Existing(asset_id));

        let id = if merged {
            Id::Existing(asset_id)
        } else {
            Id::New(index)
        };

        let mut cat_spend = FungibleSpend::new(eve_cat, !merged);

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

    #[test]
    fn test_action_issue_more_of_existing_cat() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(10);
        let bob = chia_sdk_test::BlsPair::new(1);
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;
        let tail_spend = Spend::new(tail, NodePtr::NIL);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(&mut ctx, &[Action::issue_cat(tail_spend, None, 10)])?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

        let cat = outputs.cats[&Id::New(0)][0];
        let id = Id::Existing(cat.info.asset_id);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(sim.new_coin(alice.puzzle_hash, 5));
        spends.add(cat);

        // Only part of the combined 15 is sent, so the rest must come back as change rather than being melted.
        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::issue_cat(tail_spend, None, 5),
                Action::send(id, bob.puzzle_hash, 12, bob_hint),
            ],
        )?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert!(!outputs.cats.contains_key(&Id::New(0)));

        let cats = &outputs.cats[&id];
        let sent: u64 = cats
            .iter()
            .filter(|cat| cat.info.p2_puzzle_hash == bob.puzzle_hash)
            .map(|cat| cat.coin.amount)
            .sum();
        let change: u64 = cats
            .iter()
            .filter(|cat| cat.info.p2_puzzle_hash == alice.puzzle_hash)
            .map(|cat| cat.coin.amount)
            .sum();

        assert_eq!(sent, 12);
        assert_eq!(change, 3);

        for cat in cats {
            assert!(
                sim.coin_state(cat.coin.coin_id())
                    .is_some_and(|state| state.spent_height.is_none())
            );
        }

        Ok(())
    }
}
