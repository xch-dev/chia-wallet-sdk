use chia_sdk_types::Conditions;

use crate::{DriverError, Spend, SpendContext};

/// A puzzle that can be spent by outputting a list of [`Conditions`].
///
/// This is implemented by p2 puzzles such as [`StandardLayer`](crate::StandardLayer), so that
/// code that only needs to output conditions can be written once and work with any of them.
///
/// # Example
///
/// ```
/// use chia_sdk_driver::{DriverError, SpendContext, SpendWithConditions, StandardLayer};
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::Conditions;
/// use chia_protocol::{Bytes32, Coin};
/// use chia_puzzle_types::Memos;
///
/// /// Sends the full value of a coin to a puzzle hash, with any p2 puzzle.
/// fn send_all(
///     ctx: &mut SpendContext,
///     p2: &impl SpendWithConditions,
///     coin: Coin,
///     puzzle_hash: Bytes32,
/// ) -> Result<(), DriverError> {
///     let conditions = Conditions::new().create_coin(puzzle_hash, coin.amount, Memos::None);
///     let spend = p2.spend_with_conditions(ctx, conditions)?;
///     ctx.spend(coin, spend)
/// }
///
/// let mut sim = Simulator::new();
/// let mut ctx = SpendContext::new();
///
/// let alice = sim.bls(1_000);
/// let bob = sim.bls(0);
///
/// send_all(&mut ctx, &StandardLayer::new(alice.pk), alice.coin, bob.puzzle_hash)?;
/// sim.spend_coins(ctx.take(), &[alice.sk])?;
///
/// let bob_coins = sim.unspent_coins(bob.puzzle_hash, false);
/// assert!(bob_coins.iter().any(|coin| coin.amount == 1_000));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub trait SpendWithConditions {
    /// Creates a [`Spend`] of this puzzle that outputs the given conditions.
    ///
    /// Depending on the puzzle, the spend may require a signature, and it may add conditions of
    /// its own (for example, an `AGG_SIG_ME` for the key that owns the coin).
    fn spend_with_conditions(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
    ) -> Result<Spend, DriverError>;
}
