use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{SlotNeigborsInfo, XchandlesDataValue},
    Mod,
};

pub const XCHANDLES_REGISTER_PUZZLE: [u8; 1345] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff4fffff0bffff0101ff820b6f8080ff
    ff20ff82056f80ffff0aff4fff8213ef80ffff0aff821befff4f80ffff09ff57
    ffff02ff2effff04ff02ffff04ff8202efff8080808080ffff09ff81b7ffff02
    ff2effff04ff02ffff04ff81afff8080808080ffff09ffff0dff8309ffef80ff
    ff012080ffff15ffff0141ffff0dff830dffef808080ffff01ff04ff17ffff02
    ff1affff04ff02ffff04ffff02ff8202efffff04ffff0bff52ffff0bff3cffff
    0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ff83
    0bffef80ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff01
    01ffff02ff2effff04ff02ffff04ffff04ffff04ffff04ff57ff8205ef80ffff
    04ff81b7ff82016f8080ffff04ffff04ff820b6fff8317ffef80ffff04ff8309
    ffefff830dffef808080ff808080808080ffff0bff3cff62ff42808080ff4280
    8080ff42808080ff8205ef8080ffff04ffff05ffff02ff81afff82016f8080ff
    ff04ffff04ffff04ff10ffff04ff82026fff808080ffff04ffff02ff3effff04
    ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff8213
    efffff04ff8217efff821bef8080ffff04ff822fefff825fef8080ff80808080
    ff8080808080ffff04ffff02ff3effff04ff02ffff04ff0bffff04ffff02ff2e
    ffff04ff02ffff04ffff04ffff04ff821befffff04ff8213efff82bfef8080ff
    ff04ff83017fefff8302ffef8080ff80808080ff8080808080ffff04ffff02ff
    16ffff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff
    04ff4fff820bef80ffff04ffff10ff82026fffff06ffff02ff81afff82016f80
    8080ff8305ffef8080ff80808080ff8080808080ffff04ffff02ff16ffff04ff
    02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff8213ef
    ffff04ff8217efff4f8080ffff04ff822fefff825fef8080ff80808080ff8080
    808080ffff04ffff02ff16ffff04ff02ffff04ff0bffff04ffff02ff2effff04
    ff02ffff04ffff04ffff04ff821befffff04ff4fff82bfef8080ffff04ff8301
    7fefff8302ffef8080ff80808080ff8080808080ff80808080808080ff808080
    80808080ffff01ff088080ff0180ffff04ffff01ffffff5133ff3eff4202ffff
    ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c77
    85459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596
    718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225
    f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a9
    1ce119a63400ade7c5ff04ffff04ff2cffff04ffff0113ffff04ffff0101ffff
    04ff05ffff04ff0bff808080808080ffff04ffff04ff14ffff04ffff0effff01
    72ff0580ff808080ff178080ffff04ff18ffff04ffff0bff52ffff0bff3cffff
    0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff
    0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ffff04ff80ff
    ff04ffff04ff05ff8080ff8080808080ffff02ffff03ffff07ff0580ffff01ff
    0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff
    04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04
    ff2cffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff3cffff0bff3c
    ff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff
    0101ff0b8080ffff0bff3cff62ff42808080ff42808080ff8080808080ff0180
    80
    "
);

pub const XCHANDLES_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    07848cf0db85d13490c15331a065364add5f5b52d8059c410f1ff7aa87e66722
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRegisterActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRegisterActionSolution<PP, PS, CMP, CMS, S> {
    pub handle_hash: Bytes32,
    pub pricing_puzzle_reveal: PP,
    pub pricing_puzzle_solution: PS,
    pub cat_maker_reveal: CMP,
    pub cat_maker_solution: CMS,
    pub neighbors: SlotNeigborsInfo,
    pub left_left_value: Bytes32,
    pub left_expiration: u64,
    pub left_data: XchandlesDataValue,
    pub right_right_value: Bytes32,
    pub right_expiration: u64,
    pub right_data: XchandlesDataValue,
    pub data: XchandlesDataValue,
    pub refund_puzzle_hash_hash: Bytes32,
    pub secret: S,
}

impl Mod for XchandlesRegisterActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_REGISTER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_REGISTER_PUZZLE_HASH
    }
}

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
