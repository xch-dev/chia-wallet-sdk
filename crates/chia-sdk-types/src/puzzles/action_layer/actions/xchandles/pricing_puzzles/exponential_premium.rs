use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE: [u8; 333] = hex!(
    "
    ff02ffff01ff04ffff10ffff05ffff02ff05ff81ff8080ffff02ff06ffff04ff
    02ffff04ffff02ff04ffff04ff02ffff04ff5fffff04ff81bfffff04ffff0101
    ffff04ffff05ffff14ffff12ffff0183010000ffff3dffff11ff82017fff8202
    ff80ff0b8080ff0b8080ffff04ffff05ffff14ff17ffff17ffff0101ffff05ff
    ff14ffff11ff82017fff8202ff80ff0b8080808080ff8080808080808080ffff
    04ff2fff808080808080ffff06ffff02ff05ff81ff808080ffff04ffff01ffff
    02ffff03ff0bffff01ff02ff04ffff04ff02ffff04ff05ffff04ff1bffff04ff
    ff17ff17ffff010180ffff04ff2fffff04ffff02ffff03ffff18ff2fff1780ff
    ff01ff05ffff14ffff12ff5fff1380ff058080ffff015f80ff0180ff80808080
    80808080ffff015f80ff0180ff02ffff03ffff15ff05ff0b80ffff01ff11ff05
    ff0b80ff8080ff0180ff018080
    "
);

pub const XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b54c0f4b73e63e78470366bd4006ca629d94f36c8ea58abacf8cc1cbb7724907
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
