use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
    Mod,
};

pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE: [u8; 1120] = hex!(
    "
    ff02ffff01ff04ff5fffff04ffff04ff10ffff04ff82057fff808080ffff04ff
    ff04ff28ffff04ff81bfff808080ffff04ffff02ff3effff04ff02ffff04ff17
    ffff04ffff02ff16ffff04ff02ffff04ff82017fff80808080ff8080808080ff
    ff04ffff02ff3affff04ff02ffff04ff17ffff04ffff02ff16ffff04ff02ffff
    04ffff04ff82027fffff04ff82057fff8202ff8080ff80808080ff8080808080
    ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff16ffff04ff02ffff04ff
    ff04ff82047fff8202ff80ff80808080ffff04ffff02ff16ffff04ff02ffff04
    ffff04ff8209ffff81bf80ff80808080ffff04ffff30ff8209ffffff02ff2eff
    ff04ff02ffff04ff05ffff04ff0bffff04ff820b7fffff04ff8215ffff808080
    80808080ff821dff80ffff04ffff02ff2effff04ff02ffff04ff05ffff04ff0b
    ffff04ff8204ffffff04ff820bffff80808080808080ffff04ffff02ff2effff
    04ff02ffff04ff05ffff04ff0bffff04ff8206ffffff04ff820fffff80808080
    808080ff8080808080808080808080808080ffff04ffff01ffffff55ff5333ff
    43ff4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385
    a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e8
    78a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531
    e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7
    152a6e7298a91ce119a63400ade7c5ffff04ffff04ff14ffff04ffff013affff
    04ffff0effff0175ff0b80ffff04ff2fff8080808080ffff04ffff04ff14ffff
    04ffff0112ffff04ffff0effff016fff0b80ffff04ff5fff8080808080ffff04
    ffff04ff14ffff04ffff0112ffff04ffff0effff0172ff0b80ffff04ff81bfff
    8080808080ffff04ffff02ff3effff04ff02ffff04ff05ffff04ffff0bffff01
    02ff17ff0b80ff8080808080ff8080808080ff04ff38ffff04ffff0bff52ffff
    0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff
    3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ff
    ff04ff80ffff04ffff04ff05ff8080ff8080808080ffff02ffff03ffff07ff05
    80ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff80808080ffff
    02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080
    ff0180ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff
    72ffff0bff3cffff0bff3cff62ffff02ff16ffff04ff02ffff04ffff04ff05ff
    ff04ff17ff0b8080ff8080808080ffff0bff3cffff0bff72ffff0bff3cffff0b
    ff3cff62ff2f80ffff0bff3cff62ff42808080ff42808080ff42808080ff04ff
    2cffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff3cffff0bff3cff
    62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff01
    01ff0b8080ffff0bff3cff62ff42808080ff42808080ff8080808080ff018080
    "
);

pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    6aac0dda02af18afbc0b31ad62a40cd7d4c052bb7ae5b84d1db0e2724f891344
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

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesNewDataPuzzleHashes {
    pub new_owner_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub new_resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesNewDataPuzzleHashes {
    pub fn new(
        new_owner_inner_puzzle_hash: Bytes32,
        new_resolved_inner_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            new_owner_inner_puzzle_hash,
            new_resolved_inner_puzzle_hash,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExecuteUpdateActionSolution {
    pub min_execution_height: u32,
    pub current_slot_value: XchandlesHandleSlotValue,
    pub new_data: XchandlesDataValue,
    pub current_owner: CompactCoinProof,
    #[clvm(rest)]
    pub new_data_puzzle_hashes: XchandlesNewDataPuzzleHashes,
}

impl Mod for XchandlesExecuteUpdateActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXECUTE_UPDATE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH
    }
}
