use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const XCHANDLES_FACTOR_PRICING_PUZZLE: [u8; 475] = hex!(
    "
    ff02ffff01ff02ffff03ffff15ff7fff8080ffff01ff04ffff12ff7fff05ffff
    02ff06ffff04ff02ffff04ffff0dff5f80ffff04ffff02ff04ffff04ff02ffff
    04ff5fff80808080ff808080808080ffff12ff7fff0b8080ffff01ff088080ff
    0180ffff04ffff01ffff02ffff03ff05ffff01ff02ffff03ffff22ffff15ffff
    0cff05ff80ffff010180ffff016080ffff15ffff017bffff0cff05ff80ffff01
    01808080ffff01ff02ff04ffff04ff02ffff04ffff0cff05ffff010180ff8080
    8080ffff01ff02ffff03ffff22ffff15ffff0cff05ff80ffff010180ffff012f
    80ffff15ffff013affff0cff05ff80ffff0101808080ffff01ff10ffff0101ff
    ff02ff04ffff04ff02ffff04ffff0cff05ffff010180ff8080808080ffff01ff
    088080ff018080ff0180ff8080ff0180ff05ffff14ffff02ffff03ffff15ff05
    ffff010280ffff01ff02ffff03ffff15ff05ffff010480ffff01ff02ffff03ff
    ff09ff05ffff010580ffff01ff0110ffff01ff02ffff03ffff15ff05ffff011f
    80ffff01ff0880ffff01ff010280ff018080ff0180ffff01ff02ffff03ffff09
    ff05ffff010380ffff01ff01820080ffff01ff014080ff018080ff0180ffff01
    ff088080ff0180ffff03ff0bffff0102ffff0101808080ff018080
    "
);

pub const XCHANDLES_FACTOR_PRICING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    a7edc890e6c256e4e729e826e7b45ad0616ec8d431e4e051ee68ddf4cae868bb
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesFactorPricingPuzzleArgs {
    pub base_price: u64,
    pub registration_period: u64,
}

impl XchandlesFactorPricingPuzzleArgs {
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
