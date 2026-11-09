use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized `XCHandles` factor pricing puzzle, compiled from Rue.
///
/// Prices a handle by its length: the base price is multiplied by 128 for 3 characters,
/// 64 for 4, 16 for 5, and 2 for 6 to 63, and halved if the handle contains a digit.
/// Handles must be 3 to 63 lowercase ASCII letters and digits, and `num_periods` must be
/// positive. The puzzle returns `(price . registered_time)`, where both are multiplied
/// by the number of periods. Its solution is an [`XchandlesPricingSolution`].
pub const XCHANDLES_FACTOR_PRICING_PUZZLE: [u8; 397] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff15ff7fff8080ffff01ff04ffff12ff7fff05ffff
    13ffff02ffff03ffff15ffff0dff5f80ffff010280ffff01ff03ffff15ffff0d
    ff5f80ffff010480ffff03ffff09ffff0dff5f80ffff010580ffff0110ffff02
    ffff03ffff15ffff0dff5f80ffff013f80ffff01ff0880ffff01ff010280ff01
    8080ffff03ffff09ffff0dff5f80ffff010380ffff01820080ffff01408080ff
    ff01ff088080ff0180ffff03ffff02ff02ffff04ff02ff5f8080ffff0102ffff
    0101808080ffff12ff7fff0b8080ffff01ff088080ff0180ffff04ffff01ff02
    ffff03ff03ffff01ff02ffff01ff02ffff03ffff22ffff15ff04ffff016080ff
    ff15ffff017bff048080ffff01ff02ff05ffff04ff05ff068080ffff01ff02ff
    ff03ffff22ffff15ff04ffff012f80ffff15ffff013aff048080ffff01ff10ff
    ff02ff05ffff04ff05ff068080ffff010180ffff01ff088080ff018080ff0180
    ffff04ffff04ffff0cff03ff80ffff010180ffff0cff03ffff01018080ff0180
    80ffff018080ff0180ff018080
    "
);

/// Tree hash of [`XCHANDLES_FACTOR_PRICING_PUZZLE`].
pub const XCHANDLES_FACTOR_PRICING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8876f129e711da8776b746cc07ae65f8336e8b1b7f08169de55a200ab287d1a2
    "
));

/// Curried arguments of the factor pricing puzzle ([`XCHANDLES_FACTOR_PRICING_PUZZLE`]).
///
/// # Example
///
/// Run the pricing puzzle and compare it with [`get_price`](Self::get_price):
///
/// ```rust
/// use chia_sdk_types::{
///     run_puzzle,
///     puzzles::{
///         XCHANDLES_FACTOR_PRICING_PUZZLE, XchandlesFactorPricingPuzzleArgs,
///         XchandlesPricingSolution,
///     },
/// };
/// use clvm_traits::{FromClvm, ToClvm};
/// use clvm_utils::CurriedProgram;
/// use clvmr::{Allocator, serde::node_from_bytes};
///
/// let args = XchandlesFactorPricingPuzzleArgs {
///     base_price: 1_000,
///     registration_period: 366 * 24 * 60 * 60,
/// };
///
/// let mut allocator = Allocator::new();
/// let pricing_mod = node_from_bytes(&mut allocator, &XCHANDLES_FACTOR_PRICING_PUZZLE)?;
/// let puzzle = CurriedProgram { program: pricing_mod, args }.to_clvm(&mut allocator)?;
/// let solution = XchandlesPricingSolution {
///     buy_time: 1_700_000_000,
///     current_expiration: 0,
///     handle: "alice".to_string(),
///     num_periods: 2,
/// }
/// .to_clvm(&mut allocator)?;
///
/// let output = run_puzzle(&mut allocator, puzzle, solution)?;
/// let (price, registered_time) = <(u64, u64)>::from_clvm(&allocator, output)?;
///
/// assert_eq!(price, XchandlesFactorPricingPuzzleArgs::get_price(1_000, "alice", 2));
/// assert_eq!(price, 32_000);
/// assert_eq!(registered_time, 2 * args.registration_period);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesFactorPricingPuzzleArgs {
    /// Price of one period for a handle of 6 or more characters that contains a digit (the
    /// cheapest kind).
    pub base_price: u64,
    /// Length of one registration period, in seconds.
    pub registration_period: u64,
}

impl XchandlesFactorPricingPuzzleArgs {
    /// Returns whether the handle matches the canonical `XCHandles` grammar accepted by the
    /// pricing puzzle: 3 to 63 lowercase ASCII letters and digits.
    pub fn is_valid_handle(handle: &str) -> bool {
        let len = handle.len();
        (3..=63).contains(&len)
            && handle
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    }

    /// Computes the price the pricing puzzle charges for `num_periods` periods.
    ///
    /// This doesn't validate the handle (the puzzle rejects invalid handles), so check it
    /// with [`is_valid_handle`](Self::is_valid_handle) first.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chia_sdk_types::puzzles::XchandlesFactorPricingPuzzleArgs;
    ///
    /// assert_eq!(XchandlesFactorPricingPuzzleArgs::get_price(5, "abc", 1), 640);
    /// // Handles with a digit cost half as much.
    /// assert_eq!(XchandlesFactorPricingPuzzleArgs::get_price(5, "ab1", 1), 320);
    /// assert_eq!(XchandlesFactorPricingPuzzleArgs::get_price(5, "example", 3), 30);
    /// ```
    pub fn get_price(base_price: u64, handle: &str, num_periods: u64) -> u64 {
        base_price
            * match handle.len() {
                3 => 128,
                4 => 64,
                5 => 16,
                _ => 2,
            }
            / if handle.contains(|c: char| c.is_numeric()) {
                2
            } else {
                1
            }
            * num_periods
    }
}

/// Solution to an `XCHandles` pricing puzzle, encoded as
/// `(buy_time current_expiration handle . num_periods)`.
///
/// Actions check `buy_time` and `current_expiration` against the chain and the handle's
/// slot, so pricing puzzles can trust them.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesPricingSolution {
    /// Purchase timestamp, in seconds. Actions assert that it has passed.
    pub buy_time: u64,
    /// Current expiration of the handle: 0 when registering a new handle, or the handle's
    /// expiration when extending or re-registering it.
    pub current_expiration: u64,
    /// Handle being priced.
    pub handle: String,
    /// Number of registration periods to buy.
    #[clvm(rest)]
    pub num_periods: u64,
}

impl Mod for XchandlesFactorPricingPuzzleArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_FACTOR_PRICING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_FACTOR_PRICING_PUZZLE_HASH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_handle_grammar() {
        assert!(XchandlesFactorPricingPuzzleArgs::is_valid_handle("abc"));
        assert!(XchandlesFactorPricingPuzzleArgs::is_valid_handle("a1b"));
        assert!(XchandlesFactorPricingPuzzleArgs::is_valid_handle(
            &"a".repeat(63)
        ));
        assert!(XchandlesFactorPricingPuzzleArgs::is_valid_handle(
            "ashorttermmindgetsinthewayofalongtermgrind"
        ));
    }

    #[test]
    fn rejects_invalid_handle_grammar() {
        for handle in [
            "",
            "a",
            "aa",
            &*"a".repeat(64),
            "ABC",
            "yak@test",
            "foo bar",
            "café",
        ] {
            assert!(
                !XchandlesFactorPricingPuzzleArgs::is_valid_handle(handle),
                "expected invalid: {handle:?}"
            );
        }
    }

    #[test]
    fn digit_and_multi_period_pricing_unchanged() {
        assert_eq!(
            XchandlesFactorPricingPuzzleArgs::get_price(5, "abc", 1),
            640
        );
        assert_eq!(
            XchandlesFactorPricingPuzzleArgs::get_price(5, "ab1", 1),
            320
        );
        assert_eq!(
            XchandlesFactorPricingPuzzleArgs::get_price(5, "example", 3),
            30
        );
        assert_eq!(
            XchandlesFactorPricingPuzzleArgs::get_price(5, "example1", 2),
            10
        );
    }
}
