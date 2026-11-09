use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized `XCHandles` exponential premium pricing puzzle, compiled from Rue.
///
/// Expired handles are sold in a reverse auction to deter sniping. This puzzle runs a
/// base pricing program and adds a premium of about
/// `start_premium * 0.5^(elapsed / halving_period) - end_value` (or 0 if negative),
/// where `elapsed = buy_time - current_expiration`. Its solution is an
/// [`XchandlesPricingSolution`](crate::puzzles::XchandlesPricingSolution), which is also
/// passed to the base program.
pub const XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE: [u8; 296] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ffff10ff04ffff02ff0dffff04ffff02ff09ffff
    04ff09ffff04ffff04ff82017fffff13ff2fffff17ffff0101ffff10ffff13ff
    ff11ff8202ffff8205ff80ff178080808080ffff04ff81bfffff04ffff0101ff
    ff13ffff12ffff3dffff11ff8202ffff8205ff80ff1780ffff018301000080ff
    17808080808080ff5f808080ff0680ffff04ffff02ff05ff81ff80ff018080ff
    ff04ffff04ffff01ff02ffff03ff09ffff01ff02ff02ffff04ff02ffff04ffff
    04ff19ffff03ffff18ff1fff1780ffff13ffff12ff0dff1180ff0b80ff0d8080
    ffff04ff0bffff04ffff17ff17ffff010180ff1f8080808080ffff010d80ff01
    80ffff01ff02ffff03ffff15ff02ff0380ffff01ff11ff02ff0380ffff018080
    ff018080ff018080
    "
);

/// Tree hash of [`XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE`].
pub const XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    eec82d74678809577247f69adc8efdc5e8a342c4bb595e1dae00dc7f7aad1d10
    "
));

/// Fixed-point precision of [`PREMIUM_BITS_LIST`]: `10^18`.
pub const PREMIUM_PRECISION: u64 = 1_000_000_000_000_000_000; // 10^18

/// Values of `0.5^(2^i / 65536) * 10^18` for `i` in `0..16`, used to apply the fractional
/// part of the elapsed halving periods with 16 bits of precision.
#[allow(clippy::unreadable_literal)]
// https://github.com/ensdomains/ens-contracts/blob/master/contracts/ethregistrar/ExponentialPremiumPriceOracle.sol
pub const PREMIUM_BITS_LIST: [u64; 16] = [
    999989423469314432, // 0.5 ^ 1/65536 * (10 ** 18)
    999978847050491904, // 0.5 ^ 2/65536 * (10 ** 18)
    999957694548431104,
    999915390886613504,
    999830788931929088,
    999661606496243712,
    999323327502650752,
    998647112890970240,
    997296056085470080,
    994599423483633152,
    989228013193975424,
    978572062087700096,
    957603280698573696,
    917004043204671232,
    840896415253714560,
    707106781186547584,
];

/// Curried arguments of the exponential premium pricing puzzle
/// ([`XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE`]).
///
/// # Example
///
/// ```rust
/// use chia_sdk_types::puzzles::XchandlesExponentialPremiumRenewPuzzleArgs;
///
/// // With 1,000 payment units per dollar, the auction starts at $100 million...
/// let start = XchandlesExponentialPremiumRenewPuzzleArgs::<()>::get_start_premium(1_000);
/// assert_eq!(start, 100_000_000_000);
///
/// // ...and the premium reaches 0 after 28 halving periods.
/// let end = XchandlesExponentialPremiumRenewPuzzleArgs::<()>::get_end_value(1_000);
/// assert_eq!(end, start >> 28);
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExponentialPremiumRenewPuzzleArgs<P> {
    /// Pricing program that computes the base price and registered time, usually a curried
    /// [`XchandlesFactorPricingPuzzleArgs`](crate::puzzles::XchandlesFactorPricingPuzzleArgs)
    /// puzzle.
    pub base_program: P,
    /// Number of seconds after which the premium halves. The SDK uses one day (86,400).
    pub halving_period: u64,
    /// Premium right after expiration (see [`get_start_premium`](Self::get_start_premium)).
    pub start_premium: u64,
    /// Value subtracted from the decayed premium, so the premium reaches 0 at the end of the
    /// auction (see [`get_end_value`](Self::get_end_value)).
    pub end_value: u64,
    /// Fixed-point precision of [`bits_list`](Self::bits_list), normally [`PREMIUM_PRECISION`].
    pub precision: u64,
    /// Fractional halving factors, normally [`PREMIUM_BITS_LIST`].
    pub bits_list: Vec<u64>,
}

impl<P> XchandlesExponentialPremiumRenewPuzzleArgs<P> {
    /// Returns a start premium of $100 million, where `scale_factor` is the number of
    /// payment units per dollar (for example 1,000 for a stablecoin CAT with 3 decimals).
    pub fn get_start_premium(scale_factor: u64) -> u64 {
        100_000_000 * scale_factor // start auction at $100 million
    }

    /// Returns the end value for a [`get_start_premium`](Self::get_start_premium) of the same
    /// `scale_factor`: the premium after 28 halvings, `$100 million / 2^28`, rounded down.
    #[allow(clippy::cast_possible_truncation)]
    pub fn get_end_value(scale_factor: u64) -> u64 {
        // 100000000 * 10 ** 18 // 2 ** 28 = 372529029846191406
        (372_529_029_846_191_406_u128 * u128::from(scale_factor) / 1_000_000_000_000_000_000) as u64
    }
}

impl<P> Mod for XchandlesExponentialPremiumRenewPuzzleArgs<P> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH
    }
}
