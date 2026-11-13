use chia_bls::Signature;
use chia_consensus::{
    consensus_constants::ConsensusConstants, owned_conditions::OwnedSpendBundleConditions,
    spendbundle_conditions::get_conditions_from_spendbundle,
};
use chia_protocol::{CoinSpend, SpendBundle};
use clvmr::Allocator;

use crate::DriverError;

/// Calculates the cost the mempool charges for a spend bundle, using the consensus rules of the
/// network described by `constants` (such as mainnet or testnet11) in effect at `height`, which is
/// the previous transaction block height when the bundle enters the mempool.
///
/// Pass the height whenever it's known. `None` is for a transaction that may enter the mempool on
/// either side of the 3.0 hard fork, such as one that waits to be signed: it returns the higher of
/// the costs before and after the fork, so that a fee derived from it is never too low. That
/// overprices a transaction once the fork has activated (for example, by about 8% for a send with
/// ten outputs), so don't use it as a default.
///
/// The coin spends must form a complete, valid spend bundle, since cross-spend conditions such as
/// announcements are checked. The aggregated signature isn't checked.
///
/// # Example
///
/// ```
/// use chia_puzzle_types::Memos;
/// use chia_sdk_driver::{SpendContext, StandardLayer, minimum_fee, spend_bundle_cost};
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::{Conditions, MAINNET_CONSTANTS};
///
/// let mut sim = Simulator::new();
/// let mut ctx = SpendContext::new();
/// let alice = sim.bls(1);
///
/// StandardLayer::new(alice.pk).spend(
///     &mut ctx,
///     alice.coin,
///     Conditions::new().create_coin(alice.puzzle_hash, alice.coin.amount, Memos::None),
/// )?;
///
/// let cost = spend_bundle_cost(&ctx.take(), &MAINNET_CONSTANTS, Some(9_500_000))?;
/// assert_eq!(minimum_fee(cost), cost * 5);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn spend_bundle_cost(
    coin_spends: &[CoinSpend],
    constants: &ConsensusConstants,
    height: Option<u32>,
) -> Result<u64, DriverError> {
    let spend_bundle = SpendBundle::new(coin_spends.to_vec(), Signature::default());

    if let Some(height) = height {
        return Ok(conditions_at(&spend_bundle, constants, height)?.cost);
    }

    let fork_height = constants.hard_fork2_height;
    let after = conditions_at(&spend_bundle, constants, fork_height)?.cost;

    let Some(before_height) = fork_height.checked_sub(1) else {
        return Ok(after);
    };

    let before = conditions_at(&spend_bundle, constants, before_height)?.cost;

    Ok(before.max(after))
}

/// Runs a spend bundle the way the mempool does, including its limit of half a block's cost for a
/// single transaction, and returns its conditions, including its
/// [`cost`](OwnedSpendBundleConditions::cost) and the total amounts it removes and adds, whose
/// difference is the fee the mempool sees. See [`spend_bundle_cost`] for the arguments.
pub fn spend_bundle_conditions(
    coin_spends: &[CoinSpend],
    constants: &ConsensusConstants,
    height: u32,
) -> Result<OwnedSpendBundleConditions, DriverError> {
    let spend_bundle = SpendBundle::new(coin_spends.to_vec(), Signature::default());
    conditions_at(&spend_bundle, constants, height)
}

fn conditions_at(
    spend_bundle: &SpendBundle,
    constants: &ConsensusConstants,
    height: u32,
) -> Result<OwnedSpendBundleConditions, DriverError> {
    let mut allocator = Allocator::new();

    // The mempool limits a single transaction to half of a block's cost.
    let conditions = get_conditions_from_spendbundle(
        &mut allocator,
        spend_bundle,
        constants.max_block_cost_clvm / 2,
        height,
        constants,
    )?;

    Ok(OwnedSpendBundleConditions::from(&allocator, conditions))
}

/// The minimum fee rate, in mojos per cost, at which a transaction is both included in blocks by
/// farmers that enforce a minimum block fee and admitted to a full mempool.
pub const MIN_FEE_PER_COST: u64 = 5;

/// Returns the minimum fee, in mojos, for a transaction with the given cost.
pub fn minimum_fee(cost: u64) -> u64 {
    cost.saturating_mul(MIN_FEE_PER_COST)
}

/// Returns the fee, in mojos, to pay for a transaction with the given cost. This is the
/// [`minimum_fee`], raised to `market_fee_per_cost` (in mojos per cost) when that is higher.
///
/// A market rate that isn't a finite, positive number is ignored.
pub fn recommended_fee(cost: u64, market_fee_per_cost: Option<f64>) -> u64 {
    let minimum = minimum_fee(cost);

    let Some(rate) = market_fee_per_cost.filter(|rate| rate.is_finite() && *rate > 0.0) else {
        return minimum;
    };

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let market = (cost as f64 * rate).ceil() as u64;

    minimum.max(market)
}

#[cfg(test)]
mod mempool_tests;

#[cfg(test)]
mod tests {
    use chia_protocol::Bytes32;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::{Conditions, MAINNET_CONSTANTS, TESTNET11_CONSTANTS};

    use crate::{SpendContext, StandardLayer};

    use super::*;

    fn send_spends(outputs: usize) -> anyhow::Result<Vec<CoinSpend>> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1_000);
        let p2 = StandardLayer::new(alice.pk);

        let mut conditions = Conditions::new();
        for i in 0..outputs {
            let puzzle_hash = Bytes32::new([u8::try_from(i)?; 32]);
            conditions = conditions.create_coin(puzzle_hash, 1, Memos::None);
        }
        p2.spend(&mut ctx, alice.coin, conditions)?;

        Ok(ctx.take())
    }

    #[test]
    fn test_cost_before_fork() -> anyhow::Result<()> {
        let coin_spends = send_spends(2)?;
        let cost = spend_bundle_cost(
            &coin_spends,
            &MAINNET_CONSTANTS,
            Some(MAINNET_CONSTANTS.hard_fork2_height - 1),
        )?;
        assert!(cost > 0);
        Ok(())
    }

    #[test]
    fn test_cost_changes_at_fork() -> anyhow::Result<()> {
        let coin_spends = send_spends(2)?;
        let fork_height = MAINNET_CONSTANTS.hard_fork2_height;
        let before = spend_bundle_cost(&coin_spends, &MAINNET_CONSTANTS, Some(fork_height - 1))?;
        let after = spend_bundle_cost(&coin_spends, &MAINNET_CONSTANTS, Some(fork_height))?;
        assert_ne!(before, after);
        Ok(())
    }

    #[test]
    fn test_unknown_height_takes_higher_cost() -> anyhow::Result<()> {
        for outputs in [1, 10] {
            let coin_spends = send_spends(outputs)?;
            let fork_height = MAINNET_CONSTANTS.hard_fork2_height;
            let before =
                spend_bundle_cost(&coin_spends, &MAINNET_CONSTANTS, Some(fork_height - 1))?;
            let after = spend_bundle_cost(&coin_spends, &MAINNET_CONSTANTS, Some(fork_height))?;
            assert_eq!(
                spend_bundle_cost(&coin_spends, &MAINNET_CONSTANTS, None)?,
                before.max(after)
            );
        }
        Ok(())
    }

    #[test]
    fn test_constants_choose_the_fork_height() -> anyhow::Result<()> {
        let coin_spends = send_spends(2)?;
        let mainnet_after = spend_bundle_cost(
            &coin_spends,
            &MAINNET_CONSTANTS,
            Some(MAINNET_CONSTANTS.hard_fork2_height),
        )?;

        let mut constants = TESTNET11_CONSTANTS.clone();
        constants.hard_fork2_height = 100;
        assert_eq!(
            spend_bundle_cost(&coin_spends, &constants, Some(100))?,
            mainnet_after
        );
        assert_ne!(
            spend_bundle_cost(&coin_spends, &constants, Some(99))?,
            mainnet_after
        );

        constants.hard_fork2_height = 0;
        assert_eq!(
            spend_bundle_cost(&coin_spends, &constants, None)?,
            mainnet_after
        );
        Ok(())
    }

    #[test]
    fn test_incomplete_bundle_fails() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1_000);
        let conditions = Conditions::new().assert_coin_announcement(Bytes32::default());
        StandardLayer::new(alice.pk).spend(&mut ctx, alice.coin, conditions)?;

        assert!(spend_bundle_cost(&ctx.take(), &MAINNET_CONSTANTS, None).is_err());
        Ok(())
    }

    #[test]
    fn test_minimum_fee() {
        assert_eq!(minimum_fee(0), 0);
        assert_eq!(minimum_fee(50_000_000), 250_000_000);
        assert_eq!(minimum_fee(u64::MAX), u64::MAX);
    }

    #[test]
    fn test_recommended_fee() {
        let cost = 50_000_000;
        let minimum = minimum_fee(cost);
        assert_eq!(recommended_fee(cost, None), minimum);
        assert_eq!(recommended_fee(cost, Some(0.37)), minimum);
        assert_eq!(recommended_fee(cost, Some(8.5)), 425_000_000);
        assert_eq!(recommended_fee(cost, Some(f64::NAN)), minimum);
        assert_eq!(recommended_fee(cost, Some(f64::INFINITY)), minimum);
        assert_eq!(recommended_fee(cost, Some(-1.0)), minimum);
    }
}
