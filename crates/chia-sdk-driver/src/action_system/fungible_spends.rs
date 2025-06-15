use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::Conditions;

use crate::{Cat, DriverError, Output, SpendContext, SpendKind, Spendable};

const INTERMEDIATE_AMOUNT: u64 = 1;

#[derive(Debug, Clone)]
pub struct FungibleSpends<A> {
    pub items: Vec<Spendable<A>>,
}

impl<A> FungibleSpends<A>
where
    A: FungibleAsset,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_source_for_output(
        &mut self,
        ctx: &mut SpendContext,
        output: &Output,
    ) -> Result<usize, DriverError> {
        if let Some(index) = self
            .items
            .iter()
            .position(|spend| spend.kind.can_output(output))
        {
            return Ok(index);
        }

        self.get_intermediate_source(ctx)
    }

    pub fn get_intermediate_source(
        &mut self,
        ctx: &mut SpendContext,
    ) -> Result<usize, DriverError> {
        let Some(index) = self.items.iter().position(|spend| {
            spend.kind.can_output(&Output::new(
                spend.asset.p2_puzzle_hash(),
                INTERMEDIATE_AMOUNT,
            ))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        let source = &mut self.items[index];

        match &mut source.kind {
            SpendKind::Conditions(spend) => spend.add_conditions(Conditions::new().create_coin(
                source.asset.p2_puzzle_hash(),
                INTERMEDIATE_AMOUNT,
                source.asset.intermediate_memos(ctx)?,
            ))?,
        }

        let child = source.fungible_child(source.asset.p2_puzzle_hash(), INTERMEDIATE_AMOUNT);
        self.items.push(child);

        Ok(self.items.len() - 1)
    }
}

impl<A> Default for FungibleSpends<A> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

pub trait FungibleAsset: Clone {
    fn p2_puzzle_hash(&self) -> Bytes32;
    #[must_use]
    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self;
    fn intermediate_memos(&self, ctx: &mut SpendContext) -> Result<Memos, DriverError>;
}

impl FungibleAsset for Coin {
    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.puzzle_hash
    }

    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        Coin::new(self.coin_id(), p2_puzzle_hash, amount)
    }

    fn intermediate_memos(&self, _ctx: &mut SpendContext) -> Result<Memos, DriverError> {
        Ok(Memos::None)
    }
}

impl FungibleAsset for Cat {
    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        self.child(p2_puzzle_hash, amount)
    }

    fn intermediate_memos(&self, ctx: &mut SpendContext) -> Result<Memos, DriverError> {
        ctx.hint(self.info.p2_puzzle_hash)
    }
}
