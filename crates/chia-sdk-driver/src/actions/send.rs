use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::conditions::CreateCoin;

use crate::{
    Asset, Deltas, DriverError, Id, Output, SingletonDestination, SpendAction, SpendContext, Spends,
};

#[derive(Debug, Clone, Copy)]
pub struct SendAction {
    pub id: Id,
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
        deltas.update(self.id).output += self.amount;
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
            let parent_puzzle_hash = parent.asset.full_puzzle_hash();

            parent.kind.create_coin_with_assertion(
                ctx,
                parent_puzzle_hash,
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
            let parent_puzzle_hash = parent.asset.full_puzzle_hash();

            parent.kind.create_coin_with_assertion(
                ctx,
                parent_puzzle_hash,
                &mut cat.payment_assertions,
                create_coin,
            );

            let cat = parent
                .asset
                .child(create_coin.puzzle_hash, create_coin.amount);

            spends.outputs.cats.entry(self.id).or_default().push(cat);
        } else if let Some(did) = spends.dids.get_mut(&self.id) {
            let source = did.last_mut()?;
            source.child_info.destination = Some(SingletonDestination::CreateCoin(create_coin));
        } else if let Some(nft) = spends.nfts.get_mut(&self.id) {
            let source = nft.last_mut()?;
            source.child_info.destination = Some(create_coin);
        } else if let Some(option) = spends.options.get_mut(&self.id) {
            let source = option.last_mut()?;
            source.child_info.destination = Some(SingletonDestination::CreateCoin(create_coin));
        } else {
            return Err(DriverError::InvalidAssetId);
        }

        Ok(())
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
}
