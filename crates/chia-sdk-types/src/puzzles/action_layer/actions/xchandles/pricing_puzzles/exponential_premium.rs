use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

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

pub const XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    eec82d74678809577247f69adc8efdc5e8a342c4bb595e1dae00dc7f7aad1d10
    "
));

pub const PREMIUM_PRECISION: u64 = 1_000_000_000_000_000_000; // 10^18

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

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExponentialPremiumRenewPuzzleArgs<P> {
    pub base_program: P,
    pub halving_period: u64,
    pub start_premium: u64,
    pub end_value: u64,
    pub precision: u64,
    pub bits_list: Vec<u64>,
}

impl<P> XchandlesExponentialPremiumRenewPuzzleArgs<P> {
    pub fn get_start_premium(scale_factor: u64) -> u64 {
        100_000_000 * scale_factor // start auction at $100 million
    }

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
