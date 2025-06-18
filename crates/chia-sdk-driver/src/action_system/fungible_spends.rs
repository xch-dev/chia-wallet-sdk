use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    cat::{CatArgs, GenesisByCoinIdTailArgs},
    Memos,
};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::conditions::CreateCoin;

use crate::{
    Asset, Cat, Delta, DriverError, Launcher, OptionLauncher, OptionLauncherInfo, OptionType,
    Output, OutputSet, SpendContext, SpendKind,
};

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
            .position(|item| item.kind.is_allowed(output, &item.asset.constraints()))
        {
            return Ok(index);
        }

        self.intermediate_source(ctx)
    }

    pub fn run_tail_source(&mut self, ctx: &mut SpendContext) -> Result<usize, DriverError> {
        if let Some(index) = self
            .items
            .iter()
            .position(|item| item.kind.can_run_cat_tail())
        {
            return Ok(index);
        }

        self.intermediate_source(ctx)
    }

    pub fn cat_issuance_source(
        &mut self,
        ctx: &mut SpendContext,
        asset_id: Option<Bytes32>,
        amount: u64,
    ) -> Result<usize, DriverError> {
        if let Some(index) = self.items.iter().position(|item| {
            item.kind.is_allowed(
                &Output::new(
                    CatArgs::curry_tree_hash(
                        asset_id.unwrap_or_else(|| {
                            GenesisByCoinIdTailArgs::curry_tree_hash(item.asset.coin_id()).into()
                        }),
                        item.asset.p2_puzzle_hash().into(),
                    )
                    .into(),
                    amount,
                ),
                &item.asset.constraints(),
            )
        }) {
            return Ok(index);
        }

        self.intermediate_source(ctx)
    }

    pub fn intermediate_source(&mut self, ctx: &mut SpendContext) -> Result<usize, DriverError> {
        let Some((index, amount)) = self.items.iter().enumerate().find_map(|(index, item)| {
            item.kind
                .find_amount(item.asset.p2_puzzle_hash(), &item.asset.constraints())
                .map(|amount| (index, amount))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        let source = &mut self.items[index];

        source.kind.create_coin(CreateCoin::new(
            source.asset.p2_puzzle_hash(),
            amount,
            source
                .asset
                .child_memos(ctx, source.asset.p2_puzzle_hash())?,
        ));

        let child = source.fungible_child(source.asset.p2_puzzle_hash(), amount);
        self.items.push(child);

        Ok(self.items.len() - 1)
    }

    pub fn launcher_source(&mut self) -> Result<(usize, u64), DriverError> {
        let Some((index, amount)) = self.items.iter().enumerate().find_map(|(index, item)| {
            item.kind
                .find_amount(SINGLETON_LAUNCHER_HASH.into(), &item.asset.constraints())
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

        let (create_coin, launcher) =
            Launcher::create_early(self.items[index].asset.coin_id(), launcher_amount);

        self.items[index].kind.create_coin(create_coin);

        Ok((index, launcher.with_singleton_amount(singleton_amount)))
    }

    pub fn create_option_launcher(
        &mut self,
        ctx: &mut SpendContext,
        singleton_amount: u64,
        creator_puzzle_hash: Bytes32,
        seconds: u64,
        underlying_amount: u64,
        strike_type: OptionType,
    ) -> Result<(usize, OptionLauncher), DriverError> {
        let (index, launcher_amount) = self.launcher_source()?;

        let source = &mut self.items[index];

        let (create_coin, launcher) = OptionLauncher::create_early(
            ctx,
            source.asset.coin_id(),
            launcher_amount,
            OptionLauncherInfo::new(
                creator_puzzle_hash,
                source.asset.p2_puzzle_hash(),
                seconds,
                underlying_amount,
                strike_type,
            ),
            singleton_amount,
        )?;

        source.kind.create_coin(create_coin);

        Ok((index, launcher))
    }

    pub fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        delta: &Delta,
        change_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        let change = (self.selected_amount() + delta.input).saturating_sub(delta.output);

        if change == 0 {
            return Ok(());
        }

        let output = Output::new(change_puzzle_hash, change);
        let source = self.output_source(ctx, &output)?;
        let item = &mut self.items[source];

        item.kind.create_coin(CreateCoin::new(
            change_puzzle_hash,
            change,
            item.asset.child_memos(ctx, change_puzzle_hash)?,
        ));

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
            self.kind.empty_copy(),
            true,
        )
    }
}

pub trait FungibleAsset: Clone + Asset {
    #[must_use]
    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self;
    fn child_memos(
        &self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
    ) -> Result<Memos, DriverError>;
}

impl FungibleAsset for Coin {
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
