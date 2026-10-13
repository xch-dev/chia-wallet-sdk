use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    Memos,
    cat::{CatArgs, GenesisByCoinIdTailArgs},
    offer::NotarizedPayment,
};
use chia_puzzles::{SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH};
use chia_sdk_types::conditions::{AssertPuzzleAnnouncement, CreateCoin};

use crate::{
    Asset, Cat, Delta, DriverError, Launcher, OptionLauncher, OptionLauncherInfo, OptionType,
    Output, OutputSet, SpendContext, SpendKind, coin_amount,
};

/// The spends of every coin of a fungible asset (XCH, or a single CAT) in the transaction.
#[derive(Debug, Clone)]
pub struct FungibleSpends<A> {
    /// The selected coins, followed by any intermediate coins created by the transaction. All CAT
    /// spends of the same asset are part of the same ring.
    pub items: Vec<FungibleSpend<A>>,
    /// Assertions for the payments made by settlement spends of this asset, which are attached to
    /// a conditions spend by [`Spends::prepare`](crate::Spends::prepare).
    pub payment_assertions: Vec<AssertPuzzleAnnouncement>,
}

impl<A> FungibleSpends<A>
where
    A: FungibleAsset,
{
    pub fn new() -> Self {
        Self::default()
    }

    /// The total amount of the coins that were selected (ie, not created by this transaction).
    pub fn selected_amount(&self) -> u128 {
        self.items
            .iter()
            .filter(|item| !item.ephemeral)
            .map(|item| u128::from(item.asset.amount()))
            .sum()
    }

    /// Finds a spend that can create the output, or creates an intermediate coin to create it from
    /// if none can (for example, because they all create an identical coin already).
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

    /// Finds a settlement spend that can make every payment in the notarized payment, or creates an
    /// intermediate settlement coin to make it from.
    pub fn notarized_payment_source(
        &mut self,
        notarized_payment: &NotarizedPayment,
    ) -> Result<usize, DriverError> {
        if let Some(index) = self.items.iter().position(|item| {
            item.kind.is_settlement()
                && notarized_payment.payments.iter().all(|payment| {
                    item.kind.is_allowed(
                        &Output::new(payment.puzzle_hash, payment.amount),
                        &item.asset.constraints(),
                    )
                })
        }) {
            return Ok(index);
        }

        self.intermediate_settlement_source()?
            .ok_or(DriverError::NoSourceForOutput)
    }

    /// Finds a spend that can run a TAIL, or creates an intermediate coin to run it from if every
    /// conditions spend already runs one.
    pub fn run_tail_source(&mut self, ctx: &mut SpendContext) -> Result<usize, DriverError> {
        if let Some(index) = self
            .items
            .iter()
            .position(|item| item.kind.can_run_cat_tail())
        {
            return Ok(index);
        }

        self.intermediate_conditions_child(ctx)
    }

    /// Finds a spend that can create the eve CAT of an issuance, or creates an intermediate coin to
    /// issue it from. For a single issuance (`asset_id` of `None`), the asset id is derived from the
    /// parent coin, so it's different for each candidate spend.
    pub fn cat_issuance_source(
        &mut self,
        ctx: &mut SpendContext,
        asset_id: Option<Bytes32>,
        amount: u64,
    ) -> Result<usize, DriverError> {
        // The eve CAT inherits the p2 puzzle of the source, and needs to emit conditions to run the TAIL.
        if let Some(index) = self.items.iter().position(|item| {
            item.kind.is_conditions()
                && item.kind.is_allowed(
                    &Output::new(
                        CatArgs::curry_tree_hash(
                            asset_id.unwrap_or_else(|| {
                                GenesisByCoinIdTailArgs::curry_tree_hash(item.asset.coin_id())
                                    .into()
                            }),
                            item.p2_puzzle_hash().into(),
                        )
                        .into(),
                        amount,
                    ),
                    &item.asset.constraints(),
                )
        }) {
            return Ok(index);
        }

        self.intermediate_conditions_child(ctx)
    }

    /// Creates an ephemeral child of the first item that can create one, with the same p2 puzzle hash.
    pub fn intermediate_source(&mut self, ctx: &mut SpendContext) -> Result<usize, DriverError> {
        self.intermediate_child_where(ctx, |_| true)
    }

    /// Like [`FungibleSpends::intermediate_source`], but the child is guaranteed to be able to emit conditions.
    fn intermediate_conditions_child(
        &mut self,
        ctx: &mut SpendContext,
    ) -> Result<usize, DriverError> {
        self.intermediate_child_where(ctx, SpendKind::is_conditions)
    }

    fn intermediate_child_where(
        &mut self,
        ctx: &mut SpendContext,
        predicate: impl Fn(&SpendKind) -> bool,
    ) -> Result<usize, DriverError> {
        let Some((index, amount)) = self.items.iter().enumerate().find_map(|(index, item)| {
            if !predicate(&item.kind) {
                return None;
            }

            item.kind
                .find_amount(item.p2_puzzle_hash(), &item.asset.constraints())
                .map(|amount| (index, amount))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        let source = &mut self.items[index];

        source.kind.create_intermediate_coin(
            source.asset.coin_id(),
            CreateCoin::new(
                source.p2_puzzle_hash(),
                amount,
                source.asset.child_memos(ctx, source.p2_puzzle_hash())?,
            ),
        );

        let child = FungibleSpend::new(
            source.asset.make_child(source.p2_puzzle_hash(), amount),
            true,
        );

        self.items.push(child);

        Ok(self.items.len() - 1)
    }

    /// Creates an intermediate settlement coin from the first spend that can create one, for a
    /// notarized payment that no settlement spend can make without duplicating one of its outputs.
    /// Returns `None` if no spend can create one.
    pub fn intermediate_settlement_source(&mut self) -> Result<Option<usize>, DriverError> {
        let Some((index, amount)) = self.items.iter().enumerate().find_map(|(index, item)| {
            item.kind
                .find_amount(SETTLEMENT_PAYMENT_HASH.into(), &item.asset.constraints())
                .map(|amount| (index, amount))
        }) else {
            return Ok(None);
        };

        let source = &mut self.items[index];

        source.kind.create_intermediate_coin(
            source.asset.coin_id(),
            CreateCoin::new(SETTLEMENT_PAYMENT_HASH.into(), amount, Memos::None),
        );

        let child = FungibleSpend::new(
            source
                .asset
                .make_child(SETTLEMENT_PAYMENT_HASH.into(), amount),
            true,
        );

        self.items.push(child);

        Ok(Some(self.items.len() - 1))
    }

    /// Creates an intermediate coin with the intermediate puzzle hash, which can emit conditions
    /// when no other spend can. Returns `None` if no spend can create one.
    pub fn intermediate_conditions_source(
        &mut self,
        ctx: &mut SpendContext,
        intermediate_puzzle_hash: Bytes32,
    ) -> Result<Option<usize>, DriverError> {
        let Some((index, amount)) = self.items.iter().enumerate().find_map(|(index, item)| {
            item.kind
                .find_amount(intermediate_puzzle_hash, &item.asset.constraints())
                .map(|amount| (index, amount))
        }) else {
            return Ok(None);
        };

        let source = &mut self.items[index];

        let hint = ctx.hint(intermediate_puzzle_hash)?;

        source.kind.create_intermediate_coin(
            source.asset.coin_id(),
            CreateCoin::new(intermediate_puzzle_hash, amount, hint),
        );

        let child = FungibleSpend::new(
            source.asset.make_child(intermediate_puzzle_hash, amount),
            true,
        );

        self.items.push(child);

        Ok(Some(self.items.len() - 1))
    }

    /// Finds a spend that can create a launcher coin. Launchers are created by a spend that must
    /// also emit conditions to assert the launcher's announcement, so settlement spends are skipped.
    pub fn launcher_source(&mut self) -> Result<(usize, u64), DriverError> {
        let Some((index, amount)) = self.items.iter().enumerate().find_map(|(index, item)| {
            if !item.kind.is_conditions() {
                return None;
            }

            item.kind
                .find_amount(SINGLETON_LAUNCHER_HASH.into(), &item.asset.constraints())
                .map(|amount| (index, amount))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        Ok((index, amount))
    }

    /// Creates a singleton launcher from a conditions spend. The caller must add the conditions
    /// that spend the launcher to the spend at the returned index.
    pub fn create_launcher(
        &mut self,
        singleton_amount: u64,
    ) -> Result<(usize, Launcher), DriverError> {
        let (index, launcher_amount) = self.launcher_source()?;

        let parent_coin_id = self.items[index].asset.coin_id();
        let (create_coin, launcher) = Launcher::create_early(parent_coin_id, launcher_amount);

        self.items[index]
            .kind
            .create_intermediate_coin(parent_coin_id, create_coin);

        Ok((index, launcher.with_singleton_amount(singleton_amount)))
    }

    /// Like [`FungibleSpends::create_launcher`], but for an option contract. The option is owned by
    /// the p2 puzzle hash of the spend that creates the launcher.
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
                source.p2_puzzle_hash(),
                seconds,
                underlying_amount,
                strike_type,
            ),
            singleton_amount,
        )?;

        source
            .kind
            .create_intermediate_coin(source.asset.coin_id(), create_coin);

        Ok((index, launcher))
    }

    /// Creates a change coin for the remaining amount, if there is any.
    ///
    /// Returns [`DriverError::InsufficientFunds`] if the selected coins and delta inputs don't
    /// cover the delta outputs, since the transaction would be invalid. Returns
    /// [`DriverError::AmountOverflow`] if the change doesn't fit in a single coin.
    pub fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        delta: &Delta,
        change_puzzle_hash: Bytes32,
    ) -> Result<Option<A>, DriverError> {
        let change = (self.selected_amount() + delta.input)
            .checked_sub(delta.output)
            .ok_or(DriverError::InsufficientFunds)?;
        let change = coin_amount(change)?;

        if change == 0 {
            return Ok(None);
        }

        let output = Output::new(change_puzzle_hash, change);
        let source = self.output_source(ctx, &output)?;
        let item = &mut self.items[source];

        let parent_coin = item.asset.coin();
        let create_coin = CreateCoin::new(
            change_puzzle_hash,
            change,
            item.asset.child_memos(ctx, change_puzzle_hash)?,
        );
        item.kind.create_coin_with_assertion(
            ctx,
            parent_coin,
            &mut self.payment_assertions,
            create_coin,
        );

        Ok(Some(item.asset.make_child(change_puzzle_hash, change)))
    }
}

impl<A> Default for FungibleSpends<A> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            payment_assertions: Vec::new(),
        }
    }
}

/// The spend of a single coin of a fungible asset.
#[derive(Debug, Clone)]
pub struct FungibleSpend<T> {
    pub asset: T,
    /// What the coin's p2 puzzle will output, which depends on whether it's a settlement coin.
    pub kind: SpendKind,
    /// Whether the coin is created in the same transaction. Ephemeral coins don't count toward
    /// the selected amount, since their value is already accounted for by the action that created them.
    pub ephemeral: bool,
    /// Whether the coin is spent with its hidden puzzle rather than its p2 puzzle. The outputs of a
    /// revocation spend are wrapped in the same revocation layer (and hinted with the p2 puzzle hash)
    /// by [`Spends::prepare`](crate::Spends::prepare), so they remain revocable.
    pub revoke: bool,
}

impl<T> FungibleSpend<T>
where
    T: FungibleAsset,
{
    /// A spend of the coin with its p2 puzzle. Coins with the settlement payments puzzle as their p2
    /// puzzle are spent as settlement spends.
    pub fn new(asset: T, ephemeral: bool) -> Self {
        let kind = if asset.p2_puzzle_hash() == SETTLEMENT_PAYMENT_HASH.into() {
            SpendKind::settlement()
        } else {
            SpendKind::conditions()
        };

        Self {
            asset,
            kind,
            ephemeral,
            revoke: false,
        }
    }

    /// A spend of a selected coin with its hidden puzzle, which always emits conditions.
    ///
    /// Returns [`DriverError::NotRevocable`] if the asset doesn't have a hidden puzzle.
    pub fn revocation(asset: T) -> Result<Self, DriverError> {
        if asset.hidden_puzzle_hash().is_none() {
            return Err(DriverError::NotRevocable);
        }

        Ok(Self {
            asset,
            kind: SpendKind::conditions(),
            ephemeral: false,
            revoke: true,
        })
    }

    /// The puzzle hash of the puzzle that authorizes this spend. For revocation spends this is
    /// the hidden puzzle hash, otherwise it's the p2 puzzle hash of the asset.
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        if self.revoke
            && let Some(hidden_puzzle_hash) = self.asset.hidden_puzzle_hash()
        {
            return hidden_puzzle_hash;
        }

        self.asset.p2_puzzle_hash()
    }
}

/// An asset whose coins can be split and combined freely.
pub trait FungibleAsset: Clone + Asset {
    /// The child of the coin with the given p2 puzzle hash and amount, which has the same outer
    /// puzzles as the coin.
    #[must_use]
    fn make_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self;

    /// The memos for a child with the given p2 puzzle hash, so that wallets can find it. CATs are
    /// hinted with the p2 puzzle hash, since their puzzle hash is wrapped in the CAT layer.
    fn child_memos(
        &self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
    ) -> Result<Memos, DriverError>;

    /// The hidden puzzle hash of a revocable CAT, which can be used to revoke it.
    fn hidden_puzzle_hash(&self) -> Option<Bytes32>;
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

    fn hidden_puzzle_hash(&self) -> Option<Bytes32> {
        None
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

    fn hidden_puzzle_hash(&self) -> Option<Bytes32> {
        self.info.hidden_puzzle_hash
    }
}
