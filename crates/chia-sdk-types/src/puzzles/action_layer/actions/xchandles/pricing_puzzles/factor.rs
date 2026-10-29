use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

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

pub const XCHANDLES_FACTOR_PRICING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8876f129e711da8776b746cc07ae65f8336e8b1b7f08169de55a200ab287d1a2
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesFactorPricingPuzzleArgs {
    pub base_price: u64,
    pub registration_period: u64,
}

impl XchandlesFactorPricingPuzzleArgs {
    /// Canonical `XCHandles` handle grammar: 3-63 lowercase ASCII letters and digits.
    pub fn is_valid_handle(handle: &str) -> bool {
        let len = handle.len();
        (3..=63).contains(&len)
            && handle
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    }

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

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesPricingSolution {
    pub buy_time: u64,
    pub current_expiration: u64,
    pub handle: String,
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
