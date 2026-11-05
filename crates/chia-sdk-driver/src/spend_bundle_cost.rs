use chia_bls::Signature;
use chia_consensus::spendbundle_conditions::get_conditions_from_spendbundle;
use chia_protocol::{CoinSpend, SpendBundle};
use chia_sdk_types::MAINNET_CONSTANTS;
use clvmr::Allocator;

use crate::DriverError;

/// Calculates the cost the mempool charges for a spend bundle, using the consensus rules in effect
/// at `height`, which is the previous transaction block height when the bundle enters the mempool.
///
/// Heights are compared against mainnet's fork heights. When `height` is `None`, the higher of the
/// costs before and after the 3.0 hard fork is returned, so that a fee derived from it is never too
/// low on either side of the fork.
///
/// The coin spends must form a complete, valid spend bundle, since cross-spend conditions such as
/// announcements are checked. The aggregated signature isn't checked.
pub fn spend_bundle_cost(
    coin_spends: &[CoinSpend],
    height: Option<u32>,
) -> Result<u64, DriverError> {
    let spend_bundle = SpendBundle::new(coin_spends.to_vec(), Signature::default());

    let cost_at = |height: u32| -> Result<u64, DriverError> {
        let mut allocator = Allocator::new();
        let conditions = get_conditions_from_spendbundle(
            &mut allocator,
            &spend_bundle,
            MAINNET_CONSTANTS.max_block_cost_clvm,
            height,
            &MAINNET_CONSTANTS,
        )?;
        Ok(conditions.cost)
    };

    if let Some(height) = height {
        return cost_at(height);
    }

    let fork_height = MAINNET_CONSTANTS.hard_fork2_height;
    Ok(cost_at(fork_height - 1)?.max(cost_at(fork_height)?))
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
mod tests {
    use chia_protocol::Bytes32;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;

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
        let cost = spend_bundle_cost(&coin_spends, Some(MAINNET_CONSTANTS.hard_fork2_height - 1))?;
        assert!(cost > 0);
        Ok(())
    }

    #[test]
    fn test_cost_changes_at_fork() -> anyhow::Result<()> {
        let coin_spends = send_spends(2)?;
        let fork_height = MAINNET_CONSTANTS.hard_fork2_height;
        let before = spend_bundle_cost(&coin_spends, Some(fork_height - 1))?;
        let after = spend_bundle_cost(&coin_spends, Some(fork_height))?;
        assert_ne!(before, after);
        Ok(())
    }

    #[test]
    fn test_unknown_height_takes_higher_cost() -> anyhow::Result<()> {
        for outputs in [1, 10] {
            let coin_spends = send_spends(outputs)?;
            let fork_height = MAINNET_CONSTANTS.hard_fork2_height;
            let before = spend_bundle_cost(&coin_spends, Some(fork_height - 1))?;
            let after = spend_bundle_cost(&coin_spends, Some(fork_height))?;
            assert_eq!(spend_bundle_cost(&coin_spends, None)?, before.max(after));
        }
        Ok(())
    }

    #[test]
    fn test_incomplete_bundle_fails() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1_000);
        let conditions = Conditions::new().assert_coin_announcement(Bytes32::default());
        StandardLayer::new(alice.pk).spend(&mut ctx, alice.coin, conditions)?;

        assert!(spend_bundle_cost(&ctx.take(), None).is_err());
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
