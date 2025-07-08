use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{SlotNeigborsInfo, XchandlesDataValue},
    Mod,
};

pub const XCHANDLES_EXPIRE_PUZZLE: [u8; 1073] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff02ff16ffff04ff02ffff04ff4fff
    80808080ff5780ffff09ffff02ff16ffff04ff02ffff04ff82016fff80808080
    ff81f780ffff09ffff0dff825fef80ffff012080ffff15ffff0141ffff0dff82
    7fef808080ffff01ff04ff17ffff02ff2effff04ff02ffff04ffff02ff4fffff
    04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ff
    ff0bff3cffff0bff3cff62ff8205ef80ffff0bff3cffff0bff72ffff0bff3cff
    ff0bff3cff62ffff0bffff0101ffff02ff16ffff04ff02ffff04ffff04ffff04
    ffff04ff57ff81af80ffff04ff81f7ff8202ef8080ffff04ffff04ff8216efff
    820bef80ffff04ff825fefff827fef808080ff808080808080ffff0bff3cff62
    ff42808080ff42808080ff42808080ff81af8080ffff04ffff05ffff02ff8201
    6fff8202ef8080ffff04ffff04ffff04ff10ffff04ff8204efff808080ffff04
    ffff04ff10ffff04ff820aefff808080ffff04ffff02ff3effff04ff02ffff04
    ff0bffff04ffff02ff16ffff04ff02ffff04ffff04ffff04ffff0bffff0101ff
    8216ef80ff8217ef80ffff04ff820aefff822fef8080ff80808080ff80808080
    80ffff04ffff02ff1affff04ff02ffff04ff0bffff04ffff02ff16ffff04ff02
    ffff04ffff04ffff04ffff0bffff0101ff8216ef80ff8217ef80ffff04ffff10
    ffff06ffff02ff82016fff8202ef8080ff8204ef80ff823fef8080ff80808080
    ff8080808080ff8080808080ff80808080808080ffff01ff088080ff0180ffff
    04ffff01ffffff5133ff3eff4202ffffffffa04bf5122f344554c53bde2ebb8c
    d2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a7312
    4ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619
    291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471
    ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff18ffff04
    ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff
    0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff428080
    80ff42808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff02ff
    ff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09
    ff80808080ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bff
    ff0101ff058080ff0180ffff04ffff04ff2cffff04ffff0113ffff04ffff0101
    ffff04ff05ffff04ff0bff808080808080ffff04ffff04ff14ffff04ffff0eff
    ff0178ff0580ff808080ff178080ff04ff2cffff04ffff0112ffff04ff80ffff
    04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ff
    ff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff4280
    8080ff42808080ff8080808080ff018080
    "
);

pub const XCHANDLES_EXPIRE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    514d248262b0b1607f305a26bf315f6ecb7d7705bfcf5856f12a9a22344af728
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExpireActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExpireActionSolution<CMP, CMS, EP, ES, S> {
    pub cat_maker_puzzle_reveal: CMP,
    pub cat_maker_puzzle_solution: CMS,
    pub expired_handle_pricing_puzzle_reveal: EP,
    pub expired_handle_pricing_puzzle_solution: ES,
    pub refund_puzzle_hash_hash: Bytes32,
    pub secret: S,
    pub neighbors: SlotNeigborsInfo,
    pub old_rest: XchandlesDataValue,
    #[clvm(rest)]
    pub new_rest: XchandlesDataValue,
}

impl Mod for XchandlesExpireActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXPIRE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXPIRE_PUZZLE_HASH
    }
}

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

impl<P> Mod for XchandlesExponentialPremiumRenewPuzzleArgs<P> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH
    }
}
