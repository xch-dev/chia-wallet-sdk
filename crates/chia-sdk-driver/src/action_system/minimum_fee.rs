use chia_consensus::consensus_constants::ConsensusConstants;
use chia_protocol::{Coin, CoinSpend};
use chia_sdk_utils::select_coins;

use crate::{
    Action, Deltas, DriverError, Finished, Id, Relation, SpendContext, Spends, coin_amount,
    recommended_fee, spend_bundle_cost,
};

/// The most times [`spend_with_minimum_fee`] rebuilds a transaction before giving up. Each rebuild raises the fee to
/// cover the cost of the previous attempt, which only grows when the higher fee needs another coin or a larger
/// amount, so it normally settles within two or three attempts.
const MAX_FEE_ATTEMPTS: usize = 8;

/// How [`spend_with_minimum_fee`] prices a transaction.
#[derive(Debug, Clone, Copy)]
pub struct FeeOptions<'a> {
    /// The consensus constants of the network, such as mainnet or testnet11.
    pub constants: &'a ConsensusConstants,
    /// The previous transaction block height when the transaction enters the mempool, if it's known. See
    /// [`spend_bundle_cost`].
    pub height: Option<u32>,
    /// The market fee rate in mojos per cost, if one is available. See [`recommended_fee`].
    pub market_fee_per_cost: Option<f64>,
    /// Cost to add to the cost of the transaction before pricing it, to allow for parts of the real transaction that
    /// an estimate can't reproduce exactly, such as [`VaultDryRun::cost_margin`](crate::VaultDryRun::cost_margin).
    pub extra_cost: u64,
}

impl<'a> FeeOptions<'a> {
    /// Prices a transaction at the [`minimum_fee`](crate::minimum_fee), at an unknown height.
    pub fn new(constants: &'a ConsensusConstants) -> Self {
        Self {
            constants,
            height: None,
            market_fee_per_cost: None,
            extra_cost: 0,
        }
    }
}

/// A transaction built by [`spend_with_minimum_fee`].
#[derive(Debug, Clone)]
pub struct FeeTransaction<T> {
    /// The coin spends of the transaction, including the spend of any coin that pays the fee.
    pub coin_spends: Vec<CoinSpend>,
    /// The fee the transaction pays, in mojos.
    pub fee: u64,
    /// The cost of the transaction. The fee covers this plus [`FeeOptions::extra_cost`].
    pub cost: u64,
    /// The value returned by the build function for the final attempt.
    pub value: T,
}

/// Builds a transaction whose fee covers its own cost, including the cost of the spends that pay the fee.
///
/// `build` is called with the fee to pay, and must add every coin spend of the transaction to the context, including
/// any coins that pay the fee. It's first called with a fee of zero, and then again with the
/// [`recommended_fee`] for the cost of the previous attempt, until the fee covers the cost. The coin spends that
/// were already in the context are included in every attempt. The final coin spends are returned, and left out of
/// the context.
///
/// To price a vault transaction before its members sign it, build it against a
/// [`VaultDryRun`](crate::VaultDryRun) and spend that, with its [`cost_margin`](crate::VaultDryRun::cost_margin) as
/// the [`FeeOptions::extra_cost`]. Then build the real transaction with the same fee and coins, for example with
/// [`Spends::apply_with_fee`].
///
/// Returns [`DriverError::FeeDidNotConverge`] if the fee still doesn't cover the cost after several attempts.
pub fn spend_with_minimum_fee<T>(
    ctx: &mut SpendContext,
    options: FeeOptions<'_>,
    mut build: impl FnMut(&mut SpendContext, u64) -> Result<T, DriverError>,
) -> Result<FeeTransaction<T>, DriverError> {
    let existing = ctx.take();
    let mut fee = 0;

    for _ in 0..MAX_FEE_ATTEMPTS {
        for coin_spend in &existing {
            ctx.insert(coin_spend.clone());
        }

        let value = build(ctx, fee)?;
        let coin_spends = ctx.take();

        let cost = spend_bundle_cost(&coin_spends, options.constants, options.height)?;
        let needed = recommended_fee(
            cost.saturating_add(options.extra_cost),
            options.market_fee_per_cost,
        );

        if fee >= needed {
            return Ok(FeeTransaction {
                coin_spends,
                fee,
                cost,
                value,
            });
        }

        fee = needed;
    }

    Err(DriverError::FeeDidNotConverge)
}

impl Spends {
    /// Applies the actions and a fee of `fee`, selecting XCH from `xch_coins` to cover them. `actions` shouldn't include
    /// a fee, since one is added.
    ///
    /// The coins already added to the spends are always spent, and `xch_coins` are only spent as needed. The coins are
    /// selected the same way for the same fee, so this builds the same transaction that
    /// [`Spends::apply_with_minimum_fee`] priced. `spend_p2` provides the p2 spends after the spends are prepared, as
    /// with [`Spends::spend`], and its return value is returned.
    ///
    /// Returns [`DriverError::InsufficientFunds`] if the coins can't cover the actions and the fee.
    pub fn apply_with_fee<T>(
        &self,
        ctx: &mut SpendContext,
        actions: &[Action],
        xch_coins: &[Coin],
        fee: u64,
        relation: Relation,
        spend_p2: impl FnOnce(&mut SpendContext, Spends<Finished>) -> Result<T, DriverError>,
    ) -> Result<T, DriverError> {
        let mut actions = actions.to_vec();

        if fee > 0 {
            actions.push(Action::fee(fee));
        }

        let mut spends = self.clone();
        let delta = Deltas::from_actions(&actions)
            .get(&Id::Xch)
            .copied()
            .unwrap_or_default();

        let added = spends
            .xch
            .items
            .iter()
            .try_fold(0u64, |total, item| total.checked_add(item.asset.amount))
            .ok_or(DriverError::AmountOverflow)?;
        let required = coin_amount(delta.output.saturating_sub(delta.input))?;
        let missing = required.saturating_sub(added);

        if missing > 0 {
            let coins = select_coins(xch_coins.to_vec(), missing)
                .map_err(|_| DriverError::InsufficientFunds)?;

            for coin in coins {
                spends.add(coin);
            }
        }

        let deltas = spends.apply(ctx, &actions)?;
        let spends = spends.prepare(ctx, &deltas, relation)?;

        spend_p2(ctx, spends)
    }

    /// Applies the actions with a fee that covers the cost of the whole transaction, including the coins that pay the
    /// fee, selecting XCH from `xch_coins` with [`Spends::apply_with_fee`]. See [`spend_with_minimum_fee`] for how
    /// the fee is found.
    pub fn apply_with_minimum_fee<T>(
        &self,
        ctx: &mut SpendContext,
        actions: &[Action],
        xch_coins: &[Coin],
        relation: Relation,
        options: FeeOptions<'_>,
        mut spend_p2: impl FnMut(&mut SpendContext, Spends<Finished>) -> Result<T, DriverError>,
    ) -> Result<FeeTransaction<T>, DriverError> {
        spend_with_minimum_fee(ctx, options, |ctx, fee| {
            self.apply_with_fee(ctx, actions, xch_coins, fee, relation, &mut spend_p2)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chia_bls::PublicKey;
    use chia_protocol::Bytes32;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::{Conditions, MAINNET_CONSTANTS};

    use crate::{
        Outputs, SpendKind, SpendWithConditions, StandardLayer, minimum_fee,
        spend_bundle_conditions,
    };

    use super::*;

    fn spend_standard(
        ctx: &mut SpendContext,
        spends: Spends<Finished>,
        public_key: PublicKey,
    ) -> Result<Outputs, DriverError> {
        let mut coin_spends = HashMap::new();

        for (asset, kind) in spends.unspent() {
            let SpendKind::Conditions(spend) = kind else {
                unreachable!("there are no settlement spends");
            };
            coin_spends.insert(
                asset.coin().coin_id(),
                StandardLayer::new(public_key).spend_with_conditions(ctx, spend.finish())?,
            );
        }

        spends.spend(ctx, coin_spends)
    }

    #[test]
    fn test_fee_coin_is_selected_and_priced() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        // The first coin covers the payment exactly, so the fee needs the second one.
        let alice = sim.bls(0);
        let payment = sim.new_coin(alice.puzzle_hash, 1_000);
        let fee_coin = sim.new_coin(alice.puzzle_hash, 1_000_000_000_000);

        let spends = Spends::new(alice.puzzle_hash);
        let actions = [Action::send(
            Id::Xch,
            Bytes32::new([7; 32]),
            1_000,
            Memos::None,
        )];

        let tx = spends.apply_with_minimum_fee(
            &mut ctx,
            &actions,
            &[payment, fee_coin],
            Relation::None,
            FeeOptions::new(&MAINNET_CONSTANTS),
            |ctx, spends| spend_standard(ctx, spends, alice.pk),
        )?;

        assert!(tx.coin_spends.iter().any(|cs| cs.coin == fee_coin));
        assert_eq!(tx.fee, minimum_fee(tx.cost));
        assert_eq!(
            spend_bundle_cost(&tx.coin_spends, &MAINNET_CONSTANTS, None)?,
            tx.cost
        );

        let height = MAINNET_CONSTANTS.hard_fork2_height - 1;
        let conds = spend_bundle_conditions(&tx.coin_spends, &MAINNET_CONSTANTS, height)?;
        assert_eq!(
            u64::try_from(conds.removal_amount - conds.addition_amount)?,
            tx.fee
        );

        // The fee for the same transaction can be rebuilt with the same coins.
        spends.apply_with_fee(
            &mut ctx,
            &actions,
            &[payment, fee_coin],
            tx.fee,
            Relation::None,
            |ctx, spends| spend_standard(ctx, spends, alice.pk),
        )?;
        assert_eq!(ctx.take(), tx.coin_spends);

        sim.spend_coins(tx.coin_spends, &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_market_rate_raises_the_fee() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1_000_000_000_000);

        let mut options = FeeOptions::new(&MAINNET_CONSTANTS);
        options.market_fee_per_cost = Some(8.5);

        let tx = Spends::new(alice.puzzle_hash).apply_with_minimum_fee(
            &mut ctx,
            &[Action::send(
                Id::Xch,
                Bytes32::new([7; 32]),
                1_000,
                Memos::None,
            )],
            &[alice.coin],
            Relation::None,
            options,
            |ctx, spends| spend_standard(ctx, spends, alice.pk),
        )?;

        assert_eq!(tx.fee, recommended_fee(tx.cost, Some(8.5)));
        assert!(tx.fee > minimum_fee(tx.cost));

        sim.spend_coins(tx.coin_spends, &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_existing_spends_are_included() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1_000_000_000_000);
        let bob = sim.bls(1);

        // A spend that's already in the context is part of every attempt, and of the cost.
        StandardLayer::new(bob.pk).spend(
            &mut ctx,
            bob.coin,
            Conditions::new().create_coin(bob.puzzle_hash, 1, Memos::None),
        )?;

        let tx =
            spend_with_minimum_fee(&mut ctx, FeeOptions::new(&MAINNET_CONSTANTS), |ctx, fee| {
                StandardLayer::new(alice.pk).spend(
                    ctx,
                    alice.coin,
                    Conditions::new()
                        .create_coin(alice.puzzle_hash, alice.coin.amount - fee, Memos::None)
                        .reserve_fee(fee),
                )
            })?;

        assert_eq!(tx.coin_spends.len(), 2);
        assert!(tx.coin_spends.iter().any(|cs| cs.coin == bob.coin));
        assert!(tx.fee >= minimum_fee(tx.cost));
        assert!(ctx.take().is_empty());

        sim.spend_coins(tx.coin_spends, &[alice.sk, bob.sk])?;

        Ok(())
    }

    #[test]
    fn test_insufficient_funds_for_the_fee() {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1_000);

        let result = Spends::new(alice.puzzle_hash).apply_with_minimum_fee(
            &mut ctx,
            &[Action::send(
                Id::Xch,
                Bytes32::new([7; 32]),
                1_000,
                Memos::None,
            )],
            &[alice.coin],
            Relation::None,
            FeeOptions::new(&MAINNET_CONSTANTS),
            |ctx, spends| spend_standard(ctx, spends, alice.pk),
        );

        assert!(matches!(result, Err(DriverError::InsufficientFunds)));
    }
}
