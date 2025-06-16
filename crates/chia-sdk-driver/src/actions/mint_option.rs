use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use clvm_utils::ToTreeHash;

use crate::{
    Deltas, DriverError, FungibleAsset, Id, OptionType, SendAction, SingletonSpends, SpendAction,
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
        .ok_or(DriverError::InvalidOutput)?;

        let source = &mut spends.xch.items[source];

        let (parent_conditions, eve_option) = launcher
            .with_underlying(underlying_coin.coin_id())
            .mint_eve(ctx, source.asset.p2_puzzle_hash())?;

        match &mut source.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions)?;
            }
        }

        let kind = source.kind.child();

        spends
            .options
            .insert(Id::New(index), SingletonSpends::new(eve_option, kind, true));

        Ok(())
    }
}
