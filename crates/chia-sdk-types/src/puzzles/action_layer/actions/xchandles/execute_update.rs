use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
    Mod,
};

pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE: [u8; 1206] = hex!(
    "
    ff02ffff01ff04ff5fffff04ffff04ff10ffff04ff82053fff808080ffff04ff
    ff04ff28ffff04ff820bbfff808080ffff04ffff02ff3effff04ff02ffff04ff
    17ffff04ffff02ff16ffff04ff02ffff04ff82013fff80808080ff8080808080
    ffff04ffff02ff3affff04ff02ffff04ff17ffff04ffff02ff16ffff04ff02ff
    ff04ffff04ff82023fffff04ff82053fff8202bf8080ff80808080ff80808080
    80ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff16ffff04ff02ffff04
    ffff04ff82043fff8202bf80ff80808080ffff04ffff02ff16ffff04ff02ffff
    04ffff04ff8209bfff820bbf80ff80808080ffff04ffff30ff8209bfffff0bff
    52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cff
    ff0bff3cff62ffff02ff16ffff04ff02ffff04ffff04ff05ffff04ff820b3fff
    0b8080ff8080808080ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ff
    8215bf80ffff0bff3cff62ff42808080ff42808080ff42808080ff821dbf80ff
    ff04ffff02ff2effff04ff02ffff04ff05ffff04ff0bffff04ff8204bfffff04
    ff8217bfff80808080808080ffff04ffff02ff2effff04ff02ffff04ff05ffff
    04ff0bffff04ff8206bfffff04ff821fbfff80808080808080ff808080808080
    8080808080808080ffff04ffff01ffffff55ff5333ff43ff4202ffffffffa04b
    f5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa0
    9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2
    ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923
    f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a6
    3400ade7c5ffff04ffff04ff14ffff04ffff013affff04ffff0effff0175ff17
    80ffff04ff2fff8080808080ffff04ffff04ff14ffff04ffff0112ffff04ffff
    0effff016fff1780ffff04ff5fff8080808080ffff04ffff04ff14ffff04ffff
    0112ffff04ffff0effff0172ff1780ffff04ff81bfff8080808080ffff04ffff
    02ff3effff04ff02ffff04ff05ffff04ffff0bffff0102ff0bff1780ff808080
    8080ff8080808080ff04ff38ffff04ffff0bff52ffff0bff3cffff0bff3cff62
    ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101
    ff0b8080ffff0bff3cff62ff42808080ff42808080ffff04ff80ffff04ffff04
    ff05ff8080ff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102
    ffff02ff16ffff04ff02ffff04ff09ff80808080ffff02ff16ffff04ff02ffff
    04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ffff0bff52ffff
    0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff
    3cff62ffff02ff16ffff04ff02ffff04ffff04ff05ffff04ff17ff0b8080ff80
    80808080ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ff2f80ffff0b
    ff3cff62ff42808080ff42808080ff42808080ff04ff2cffff04ffff0112ffff
    04ff80ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cff
    ff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3c
    ff62ff42808080ff42808080ff8080808080ff018080
    "
);

pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    31b8d87f67f901948c9dc31c8eab42e1caaa4ba27659ca45f6554e915d0ec2d6
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExecuteUpdateActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_mod_hash: Bytes32,
    pub handle_slot_1st_curry_hash: Bytes32,
    pub update_slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExecuteUpdateActionSolution {
    pub current_slot_value: XchandlesHandleSlotValue,
    pub new_data: XchandlesDataValue,
    pub current_owner: CompactCoinProof,
    pub min_execution_height: u32,
    pub new_owner_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub new_resolved_inner_puzzle_hash: Bytes32,
}

impl Mod for XchandlesExecuteUpdateActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXECUTE_UPDATE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH
    }
}
