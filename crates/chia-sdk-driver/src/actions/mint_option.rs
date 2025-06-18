use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use clvm_utils::ToTreeHash;

use crate::{
    Asset, Deltas, DriverError, Id, OptionType, SendAction, SingletonSpends, SpendAction,
    SpendContext, SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub struct MintOptionAction {
    pub creator_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub underlying_id: Option<Id>,
    pub underlying_amount: u64,
    pub strike_type: OptionType,
    pub amount: u64,
}

impl MintOptionAction {
    pub fn new(
        creator_puzzle_hash: Bytes32,
        seconds: u64,
        underlying_id: Option<Id>,
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
}

impl SpendAction for MintOptionAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        deltas.update_xch().output += self.amount;
        deltas.update(Id::New(index)).input += self.amount;
        if let Some(underlying_id) = self.underlying_id {
            deltas.update(underlying_id).output += self.underlying_amount;
        } else {
            deltas.update_xch().output += self.underlying_amount;
        }
        deltas.set_xch_needed();
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
        let underlying_coin = SendAction::new(
            self.underlying_id,
            underlying_p2_puzzle_hash,
            self.underlying_amount,
            Memos::None,
        )
        .run_standalone(ctx, spends, true)?
        .ok_or(DriverError::AlreadyFinalized)?;

        let source = &mut spends.xch.items[source];

        let (parent_conditions, eve_option) = launcher
            .with_underlying(underlying_coin.coin_id())
            .mint_eve(ctx, source.asset.p2_puzzle_hash())?;

        match &mut source.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions);
            }
            SpendKind::Settlement(_) => {
                return Err(DriverError::CannotEmitConditions);
            }
        }

        let kind = source.kind.empty_copy();

        spends
            .options
            .insert(Id::New(index), SingletonSpends::new(eve_option, kind, true));

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
    fn test_action_mint_option() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(6);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add_xch(alice.coin, SpendKind::conditions());

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::single_issue_cat(5),
                Action::mint_option(
                    alice.puzzle_hash,
                    100,
                    Some(Id::New(0)),
                    5,
                    OptionType::Xch { amount: 5 },
                    1,
                ),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
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
