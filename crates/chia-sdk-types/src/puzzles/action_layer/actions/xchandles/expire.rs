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
