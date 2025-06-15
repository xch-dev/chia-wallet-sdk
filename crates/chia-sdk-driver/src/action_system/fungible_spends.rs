use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::Conditions;

use crate::{Cat, DriverError, Launcher, Output, SpendContext, SpendKind};

const INTERMEDIATE_AMOUNT: u64 = 1;

#[derive(Debug, Clone)]
pub struct FungibleSpends<A> {
    pub items: Vec<FungibleSpend<A>>,
}

impl<A> FungibleSpends<A>
where
    A: FungibleAsset,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected_amount(&self) -> u64 {
        self.items
            .iter()
            .filter(|item| !item.ephemeral)
            .map(|item| item.asset.amount())
            .sum()
    }

    pub fn output_source(
        &mut self,
        ctx: &mut SpendContext,
        output: &Output,
    ) -> Result<usize, DriverError> {
        if let Some(index) = self
            .items
            .iter()
            .position(|item| item.kind.outputs().is_allowed(output))
        {
            return Ok(index);
        }

        self.intermediate_source(ctx)
    }

    pub fn intermediate_source(&mut self, ctx: &mut SpendContext) -> Result<usize, DriverError> {
        let Some(index) = self.items.iter().position(|item| {
            item.kind.outputs().is_allowed(&Output::new(
                item.asset.p2_puzzle_hash(),
                INTERMEDIATE_AMOUNT,
            ))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        let source = &mut self.items[index];

        match &mut source.kind {
            SpendKind::Conditions(spend) => spend.add_conditions(
                Conditions::new().create_coin(
                    source.asset.p2_puzzle_hash(),
                    INTERMEDIATE_AMOUNT,
                    source
                        .asset
                        .child_memos(ctx, source.asset.p2_puzzle_hash())?,
                ),
            )?,
        }

        let child = source.fungible_child(source.asset.p2_puzzle_hash(), INTERMEDIATE_AMOUNT);
        self.items.push(child);

        Ok(self.items.len() - 1)
    }

    pub fn launcher_source(&mut self) -> Result<(usize, u64), DriverError> {
        let Some((index, amount)) = self.items.iter().enumerate().find_map(|(index, item)| {
            item.kind
                .outputs()
                .launcher_amount()
                .map(|amount| (index, amount))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        Ok((index, amount))
    }

    pub fn create_launcher(
        &mut self,
        singleton_amount: u64,
    ) -> Result<(usize, Launcher), DriverError> {
        let (index, launcher_amount) = self.launcher_source()?;

        let (parent_conditions, launcher) =
            Launcher::create_early(self.items[index].asset.get_coin_id(), launcher_amount);

        match &mut self.items[index].kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions)?;
            }
        }

        Ok((index, launcher.with_singleton_amount(singleton_amount)))
    }

    pub fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        spent_amount: u64,
        change_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        let change = self.selected_amount().saturating_sub(spent_amount);

        if change == 0 {
            return Ok(());
        }

        let output = Output::new(change_puzzle_hash, change);
        let source = self.output_source(ctx, &output)?;
        let item = &mut self.items[source];

        match &mut item.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().create_coin(
                    change_puzzle_hash,
                    change,
                    item.asset.child_memos(ctx, change_puzzle_hash)?,
                ))?;
            }
        }

        Ok(())
    }
}

impl<A> Default for FungibleSpends<A> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

#[derive(Debug, Clone)]
pub struct FungibleSpend<T> {
    pub asset: T,
    pub kind: SpendKind,
    pub ephemeral: bool,
}

impl<T> FungibleSpend<T> {
    pub fn new(asset: T, kind: SpendKind, ephemeral: bool) -> Self {
        Self {
            asset,
            kind,
            ephemeral,
        }
    }

    #[must_use]
    pub fn fungible_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self
    where
        T: FungibleAsset,
    {
        Self::new(
            self.asset.make_child(p2_puzzle_hash, amount),
            self.kind.child(),
            true,
        )
    }
}

pub trait FungibleAsset: Clone {
    fn get_coin_id(&self) -> Bytes32;
    fn p2_puzzle_hash(&self) -> Bytes32;
    fn amount(&self) -> u64;
    #[must_use]
    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self;
    fn child_memos(
        &self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
    ) -> Result<Memos, DriverError>;
}

impl FungibleAsset for Coin {
    fn get_coin_id(&self) -> Bytes32 {
        self.coin_id()
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.amount
    }

    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        Coin::new(self.coin_id(), p2_puzzle_hash, amount)
    }

    fn child_memos(
        &self,
        _ctx: &mut SpendContext,
        _p2_puzzle_hash: Bytes32,
    ) -> Result<Memos, DriverError> {
        Ok(Memos::None)
    }
}

impl FungibleAsset for Cat {
    fn get_coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.coin.amount
    }

    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        self.child(p2_puzzle_hash, amount)
    }

    fn child_memos(
        &self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
    ) -> Result<Memos, DriverError> {
        ctx.hint(p2_puzzle_hash)
    }
}
