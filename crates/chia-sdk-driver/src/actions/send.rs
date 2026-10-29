use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::conditions::CreateCoin;

use crate::{
    Asset, Delta, Deltas, DriverError, Id, Output, SingletonDestination, SpendAction, SpendContext,
    Spends,
};

/// Created by [`Action::send`](crate::Action::send) or [`Action::burn`](crate::Action::burn).
#[derive(Debug, Clone, Copy)]
pub struct SendAction {
    pub id: Id,
    /// The p2 puzzle hash of the coin to create. Outer layers (such as the CAT layer) are added.
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub memos: Memos,
}

impl SendAction {
    pub fn new(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
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
        *deltas.update(self.id) += Delta::new(0, self.amount);
        deltas.set_needed(self.id);
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let output = Output::new(self.puzzle_hash, self.amount);
        let create_coin = CreateCoin::new(self.puzzle_hash, self.amount, self.memos);

        if matches!(self.id, Id::Xch) {
            let source = spends.xch.output_source(ctx, &output)?;
            let parent = &mut spends.xch.items[source];
            let parent_coin = parent.asset.coin();

            parent.kind.create_coin_with_assertion(
                ctx,
                parent_coin,
                &mut spends.xch.payment_assertions,
                create_coin,
            );

            let coin = Coin::new(
                parent.asset.coin_id(),
                create_coin.puzzle_hash,
                create_coin.amount,
            );

            spends.outputs.xch.push(coin);
        } else if let Some(cat) = spends.cats.get_mut(&self.id) {
            let source = cat.output_source(ctx, &output)?;
            let parent = &mut cat.items[source];
            let parent_coin = parent.asset.coin();

            parent.kind.create_coin_with_assertion(
                ctx,
                parent_coin,
                &mut cat.payment_assertions,
                create_coin,
            );

            let cat = parent
                .asset
                .child(create_coin.puzzle_hash, create_coin.amount);

            spends.outputs.cats.entry(self.id).or_default().push(cat);
        } else if let Some(did) = spends.dids.get_mut(&self.id) {
            let source = did.last_mut()?;
            check_singleton_amount(source.asset.coin.amount, self.amount)?;
            source.child_info.destination = Some(SingletonDestination::CreateCoin(create_coin));
        } else if let Some(nft) = spends.nfts.get_mut(&self.id) {
            let source = nft.last_mut()?;
            check_singleton_amount(source.asset.coin.amount, self.amount)?;
            source.child_info.destination = Some(create_coin);
        } else if let Some(option) = spends.options.get_mut(&self.id) {
            let source = option.last_mut()?;
            check_singleton_amount(source.asset.coin.amount, self.amount)?;
            source.child_info.destination = Some(SingletonDestination::CreateCoin(create_coin));
        } else {
            return Err(DriverError::InvalidAssetId);
        }

        Ok(())
    }
}

/// Singletons keep their amount when they are sent or melted, so the amount in the action must match.
pub(crate) fn check_singleton_amount(expected: u64, actual: u64) -> Result<(), DriverError> {
    if expected == actual {
        Ok(())
    } else {
        Err(DriverError::SingletonAmountMismatch)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_protocol::Coin;
    use chia_puzzle_types::standard::StandardArgs;
    use chia_sdk_test::{BlsPair, Simulator};
    use indexmap::indexmap;
    use rstest::rstest;

    use crate::{Action, Cat, Relation};

    use super::*;

    #[test]
    fn test_action_send_xch() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None)],
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

    #[test]
    fn test_action_send_xch_with_change() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);
        let bob = BlsPair::new(0);
        let bob_puzzle_hash = StandardArgs::curry_tree_hash(bob.pk).into();

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::send(Id::Xch, bob_puzzle_hash, 2, Memos::None)],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert_eq!(outputs.xch.len(), 2);

        let change = outputs.xch[0];
        assert_ne!(sim.coin_state(change.coin_id()), None);
        assert_eq!(change.amount, 2);
        assert_eq!(change.puzzle_hash, bob_puzzle_hash);

        let coin = outputs.xch[1];
        assert_ne!(sim.coin_state(coin.coin_id()), None);
        assert_eq!(coin.amount, 3);
        assert_eq!(coin.puzzle_hash, alice.puzzle_hash);

        Ok(())
    }

    #[test]
    fn test_action_send_xch_split() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(3);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
                Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
                Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert_eq!(outputs.xch.len(), 3);

        let coins: Vec<Coin> = outputs
            .xch
            .iter()
            .copied()
            .filter(|coin| {
                sim.coin_state(coin.coin_id())
                    .expect("missing coin")
                    .spent_height
                    .is_none()
            })
            .collect();

        assert_eq!(coins.len(), 3);

        for coin in coins {
            assert_eq!(coin.puzzle_hash, alice.puzzle_hash);
            assert_eq!(coin.amount, 1);
        }

        Ok(())
    }

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_send_cat(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);
        let hint = ctx.hint(alice.puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::single_issue_cat(hidden_puzzle_hash, 1),
                Action::send(Id::New(0), alice.puzzle_hash, 1, hint),
            ],
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
        assert_eq!(cat.coin.amount, 1);

        Ok(())
    }

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_send_cat_with_change(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);
        let bob = BlsPair::new(0);
        let bob_puzzle_hash = StandardArgs::curry_tree_hash(bob.pk).into();
        let bob_hint = ctx.hint(bob_puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::single_issue_cat(hidden_puzzle_hash, 5),
                Action::send(Id::New(0), bob_puzzle_hash, 2, bob_hint),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cats = &outputs.cats[&Id::New(0)];
        assert_eq!(cats.len(), 2);

        let change = cats[0];
        assert_ne!(sim.coin_state(change.coin.coin_id()), None);
        assert_eq!(change.coin.amount, 2);
        assert_eq!(change.info.p2_puzzle_hash, bob_puzzle_hash);

        let cat = cats[1];
        assert_ne!(sim.coin_state(cat.coin.coin_id()), None);
        assert_eq!(cat.coin.amount, 3);
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_send_cat_split(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(3);
        let hint = ctx.hint(alice.puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::single_issue_cat(hidden_puzzle_hash, 3),
                Action::send(Id::New(0), alice.puzzle_hash, 1, hint),
                Action::send(Id::New(0), alice.puzzle_hash, 1, hint),
                Action::send(Id::New(0), alice.puzzle_hash, 1, hint),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cats = &outputs.cats[&Id::New(0)];
        assert_eq!(cats.len(), 3);

        let cats: Vec<Cat> = cats
            .iter()
            .copied()
            .filter(|cat| {
                sim.coin_state(cat.coin.coin_id())
                    .expect("missing coin")
                    .spent_height
                    .is_none()
            })
            .collect();

        assert_eq!(cats.len(), 3);

        for cat in cats {
            assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
            assert_eq!(cat.coin.amount, 1);
        }

        Ok(())
    }

    #[test]
    fn test_action_send_existing_nft() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);
        let bob = BlsPair::new(1);
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(&mut ctx, &[Action::mint_empty_nft()])?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

        let nft = outputs.nfts[&Id::New(0)];
        let id = Id::Existing(nft.info.launcher_id);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(nft);

        let deltas = spends.apply(&mut ctx, &[Action::send(id, bob.puzzle_hash, 1, bob_hint)])?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let nft = outputs.nfts[&id];
        assert_eq!(nft.info.p2_puzzle_hash, bob.puzzle_hash);
        assert!(
            sim.coin_state(nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        Ok(())
    }

    #[test]
    fn test_action_send_existing_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);
        let bob = BlsPair::new(1);
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(&mut ctx, &[Action::create_empty_did()])?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

        let did = outputs.dids[&Id::New(0)];
        let id = Id::Existing(did.info.launcher_id);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(did);

        let deltas = spends.apply(&mut ctx, &[Action::send(id, bob.puzzle_hash, 1, bob_hint)])?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let did = outputs.dids[&id];
        assert_eq!(did.info.p2_puzzle_hash, bob.puzzle_hash);
        assert!(
            sim.coin_state(did.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        Ok(())
    }

    #[test]
    fn test_action_send_singleton_amount_mismatch() {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);

        for action in [
            Action::create_empty_did(),
            Action::mint_empty_nft(),
            Action::mint_option(
                alice.puzzle_hash,
                100,
                Id::Xch,
                1,
                crate::OptionType::Xch { amount: 1 },
                1,
            ),
        ] {
            let mut spends = Spends::new(alice.puzzle_hash);
            spends.add(alice.coin);

            let result = spends.apply(
                &mut ctx,
                &[
                    action,
                    Action::send(Id::New(0), alice.puzzle_hash, 3, Memos::None),
                ],
            );

            assert!(matches!(result, Err(DriverError::SingletonAmountMismatch)));
        }
    }

    #[test]
    fn test_action_send_unknown_asset() {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let result = spends.apply(
            &mut ctx,
            &[Action::send(
                Id::Existing(Bytes32::new([1; 32])),
                alice.puzzle_hash,
                1,
                Memos::None,
            )],
        );

        assert!(matches!(result, Err(DriverError::InvalidAssetId)));
    }
}
