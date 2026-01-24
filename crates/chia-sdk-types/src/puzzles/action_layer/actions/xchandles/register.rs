use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{SlotNeigborsInfo, XchandlesDataValue},
    Mod,
};

pub const XCHANDLES_REGISTER_PUZZLE: [u8; 1748] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff82013fffff0bffff0101ff822dbf80
    80ffff20ff8215bf80ffff0aff82013fff824fbf80ffff0aff826fbfff82013f
    80ffff09ff82015fffff02ff2effff04ff02ffff04ff820bbfff8080808080ff
    ff09ff8202dfffff02ff2effff04ff02ffff04ff8202bfff808080808080ffff
    01ff04ff5fffff02ff3affff04ff02ffff04ffff02ff820bbfffff04ffff0bff
    81aaffff0bff12ffff0bff12ff81caff1780ffff0bff12ffff0bff81eaffff0b
    ff12ffff0bff12ff81caff83bfffbf80ffff0bff12ffff0bff81eaffff0bff12
    ffff0bff12ff81caffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ff
    ff04ffff04ff82015fff8217bf80ffff04ff8202dfff8205bf8080ffff04ffff
    04ff822dbfff83ffffbf80ffff04ff8327ffbfff8337ffbf808080ff80808080
    8080ffff0bff12ff81caff818a808080ff818a808080ff818a808080ff8217bf
    8080ffff04ffff05ffff02ff8202bfff8205bf8080ffff04ffff0bff81aaffff
    0bff12ffff0bff12ff81caff0580ffff0bff12ffff0bff81eaffff0bff12ffff
    0bff12ff81caffff02ff2effff04ff02ffff04ffff04ff05ffff04ff8327ffbf
    ff0b8080ff8080808080ffff0bff12ffff0bff81eaffff0bff12ffff0bff12ff
    81caff832fffbf80ffff0bff12ff81caff818a808080ff818a808080ff818a80
    8080ffff04ffff0bff81aaffff0bff12ffff0bff12ff81caff0580ffff0bff12
    ffff0bff81eaffff0bff12ffff0bff12ff81caffff02ff2effff04ff02ffff04
    ffff04ff05ffff04ff8337ffbfff0b8080ff8080808080ffff0bff12ffff0bff
    81eaffff0bff12ffff0bff12ff81caff835fffbf80ffff0bff12ff81caff818a
    808080ff818a808080ff818a808080ffff04ffff04ffff04ff10ffff04ff8209
    bfff808080ffff04ffff02ff3effff04ff02ffff04ff2fffff04ffff02ff2eff
    ff04ff02ffff04ffff04ffff04ff824fbfffff04ff825fbfff826fbf8080ffff
    04ff82bfbfff83017fbf8080ff80808080ff8080808080ffff04ffff02ff3eff
    ff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff
    826fbfffff04ff824fbfff8302ffbf8080ffff04ff8305ffbfff830bffbf8080
    ff80808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff2fffff04
    ffff02ff2effff04ff02ffff04ffff04ffff04ff82013fff822fbf80ffff04ff
    ff10ff8209bfffff06ffff02ff8202bfff8205bf808080ff8317ffbf8080ff80
    808080ff8080808080ffff04ffff02ff16ffff04ff02ffff04ff2fffff04ffff
    02ff2effff04ff02ffff04ffff04ffff04ff824fbfffff04ff825fbfff82013f
    8080ffff04ff82bfbfff83017fbf8080ff80808080ff8080808080ffff04ffff
    02ff16ffff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04
    ffff04ff826fbfffff04ff82013fff8302ffbf8080ffff04ff8305ffbfff830b
    ffbf8080ff80808080ff8080808080ff80808080808080ff8080808080808080
    80ffff01ff088080ff0180ffff04ffff01ffffff5133ff3eff4342ffff02ffff
    ffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785
    459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f59671
    8ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6
    806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91c
    e119a63400ade7c5ff04ffff04ff3cffff04ffff0113ffff04ffff0101ffff04
    ff05ffff04ff0bff808080808080ffff04ffff04ff14ffff04ffff0effff0172
    ff0580ff808080ffff04ffff04ff2cffff04ffff0112ffff04ffff0effff0161
    ff0580ffff04ff17ff8080808080ffff02ffff03ffff09ff17ff2f80ffff015f
    ffff01ff04ffff04ff2cffff04ffff0112ffff04ffff0effff0162ff0580ffff
    04ff2fff80808080808080ff0180808080ffff04ff18ffff04ffff0bff81aaff
    ff0bff12ffff0bff12ff81caff0580ffff0bff12ffff0bff81eaffff0bff12ff
    ff0bff12ff81caffff0bffff0101ff0b8080ffff0bff12ff81caff818a808080
    ff818a808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff02ff
    ff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09
    ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bff
    ff0101ff058080ff0180ff04ff3cffff04ffff0112ffff04ff80ffff04ffff0b
    ff81aaffff0bff12ffff0bff12ff81caff0580ffff0bff12ffff0bff81eaffff
    0bff12ffff0bff12ff81caffff0bffff0101ff0b8080ffff0bff12ff81caff81
    8a808080ff818a808080ff8080808080ff018080
    "
);

pub const XCHANDLES_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    abe8d63064c68d1405328eced202f31dbd9122c91849876453b375f44fe02f46
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRegisterActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_puzzle_hash: Bytes32,
    pub precommit_1st_curry_hash: Bytes32,
    pub handle_slot_1st_curry_hash: Bytes32,
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
    pub owner_inner_puzzle_hash: Bytes32,
    pub resolved_inner_puzzle_hash: Bytes32,
    pub refund_puzzle_hash_hash: Bytes32,
    #[clvm(rest)]
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
