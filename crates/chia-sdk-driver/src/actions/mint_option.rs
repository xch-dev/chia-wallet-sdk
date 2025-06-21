use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::conditions::CreateCoin;
use clvm_utils::ToTreeHash;

use crate::{
    Asset, Deltas, DriverError, Id, OptionType, Output, SingletonSpends, SpendAction, SpendContext,
    SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub struct MintOptionAction {
    pub creator_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub underlying_id: Id,
    pub underlying_amount: u64,
    pub strike_type: OptionType,
    pub amount: u64,
}

impl MintOptionAction {
    pub fn new(
        creator_puzzle_hash: Bytes32,
        seconds: u64,
        underlying_id: Id,
        underlying_amount: u64,
        strike_type: OptionType,
        amount: u64,
    ) -> Self {
        Self {
            creator_puzzle_hash,
            seconds,
            underlying_id,
            underlying_amount,
            strike_type,
            amount,
        }
    }

    fn lock_underlying(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        p2_puzzle_hash: Bytes32,
    ) -> Result<Bytes32, DriverError> {
        let output = Output::new(p2_puzzle_hash, self.underlying_amount);
        let create_coin = CreateCoin::new(p2_puzzle_hash, self.underlying_amount, Memos::None);

        if matches!(self.underlying_id, Id::Xch) {
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
                p2_puzzle_hash,
                self.underlying_amount,
            );

            spends.outputs.xch.push(coin);

            return Ok(coin.coin_id());
        } else if let Some(cat) = spends.cats.get_mut(&self.underlying_id) {
            let source = cat.output_source(ctx, &output)?;
            let parent = &mut cat.items[source];
            let parent_puzzle_hash = parent.asset.full_puzzle_hash();

            parent.kind.create_coin_with_assertion(
                ctx,
                parent_puzzle_hash,
                &mut cat.payment_assertions,
                create_coin,
            );

            let cat = parent.asset.child(p2_puzzle_hash, self.underlying_amount);

            spends
                .outputs
                .cats
                .entry(self.underlying_id)
                .or_default()
                .push(cat);

            return Ok(cat.coin_id());
        } else if let Some(nft) = spends.nfts.get_mut(&self.underlying_id) {
            let source = nft.last_mut()?;
            source.child_info.destination = Some(create_coin);

            let Some(nft) = nft.finalize(
                ctx,
                spends.intermediate_puzzle_hash,
                spends.change_puzzle_hash,
            )?
            else {
                return Err(DriverError::NoSourceForOutput);
            };

            spends.outputs.nfts.insert(self.underlying_id, nft);

            return Ok(nft.coin_id());
        }

        Err(DriverError::InvalidAssetId)
    }
}

impl SpendAction for MintOptionAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        deltas.update(Id::Xch).output += self.amount;
        deltas.update(Id::New(index)).input += self.amount;
        deltas.update(self.underlying_id).output += self.underlying_amount;
        deltas.set_needed(Id::Xch);
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        let (source, launcher) = spends.xch.create_option_launcher(
            ctx,
            self.amount,
            self.creator_puzzle_hash,
            self.seconds,
            self.underlying_amount,
            self.strike_type,
        )?;

        let underlying_p2_puzzle_hash = launcher.underlying().tree_hash().into();
        let underlying_coin_id = self.lock_underlying(ctx, spends, underlying_p2_puzzle_hash)?;

        let source = &mut spends.xch.items[source];

        let (parent_conditions, eve_option) = launcher
            .with_underlying(underlying_coin_id)
            .mint_eve(ctx, source.asset.p2_puzzle_hash())?;

        match &mut source.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions);
            }
            SpendKind::Settlement(_) => {
                return Err(DriverError::CannotEmitConditions);
            }
        }

        spends
            .options
            .insert(Id::New(index), SingletonSpends::new(eve_option, true));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;
    use rstest::rstest;

    use crate::{Action, Relation};

    use super::*;

    #[rstest]
    #[case::normal(None)]
    #[case::revocable(Some(Bytes32::default()))]
    fn test_action_mint_option(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(6);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::single_issue_cat(hidden_puzzle_hash, 5),
                Action::mint_option(
                    alice.puzzle_hash,
                    100,
                    Id::New(0),
                    5,
                    OptionType::Xch { amount: 5 },
                    1,
                ),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let option = outputs.options[&Id::New(1)];
        assert_ne!(sim.coin_state(option.coin.coin_id()), None);
        assert_eq!(option.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(option.coin.amount, 1);

        Ok(())
    }
}
