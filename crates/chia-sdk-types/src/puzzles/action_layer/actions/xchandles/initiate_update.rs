use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::CoinProof;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{XchandlesDataValue, XchandlesHandleSlotValue},
    Mod,
};

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE: [u8; 1051] = hex!(
    "
    ff02ffff01ff04ff81bfffff04ffff04ff28ffff04ff820a7fff808080ffff04
    ffff04ff10ffff04ff820f7fff808080ffff02ff12ffff04ff02ffff04ff2fff
    ff04ffff02ff2effff04ff02ffff04ff82027fff80808080ffff04ffff02ff3a
    ffff04ff02ffff04ff5fffff04ffff10ff820f7fff1780ffff04ffff02ff2eff
    ff04ff02ffff04ffff04ff82087fff82057f80ff80808080ffff04ffff30ff82
    137fffff0bff81aaffff0bff3cffff0bff3cff81caff0580ffff0bff3cffff0b
    ff81eaffff0bff3cffff0bff3cff81caffff02ff2effff04ff02ffff04ffff04
    ff05ffff04ff82167fff0b8080ff8080808080ffff0bff3cffff0bff81eaffff
    0bff3cffff0bff3cff81caff822b7f80ffff0bff3cff81caff818a808080ff81
    8a808080ff818a808080ff823b7f80ff80808080808080ff8080808080808080
    80ffff04ffff01ffffff57ff5533ff43ff4202ffffff04ffff02ff3effff04ff
    02ffff04ff05ffff04ff0bff8080808080ffff04ffff02ff26ffff04ff02ffff
    04ff05ffff04ff0bff8080808080ff178080ffffffa04bf5122f344554c53bde
    2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d1
    1a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210
    fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63
    fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ffff
    04ff14ffff04ffff013affff04ffff0effff0169ff1780ffff04ff2fff808080
    8080ffff04ffff02ff36ffff04ff02ffff04ff05ffff04ffff0bffff0102ffff
    0bffff0102ffff0bffff0101ff2f80ffff0bffff0101ff0b8080ff1780ffff04
    ff2fff808080808080ff808080ffffff04ff38ffff04ffff0bff81aaffff0bff
    3cffff0bff3cff81caff0580ffff0bff3cffff0bff81eaffff0bff3cffff0bff
    3cff81caffff0bffff0101ff0b8080ffff0bff3cff81caff818a808080ff818a
    808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ff04ff38ffff04
    ffff0bff81aaffff0bff3cffff0bff3cff81caff0580ffff0bff3cffff0bff81
    eaffff0bff3cffff0bff3cff81caffff0bffff0101ff0b8080ffff0bff3cff81
    caff818a808080ff818a808080ffff04ff80ffff04ffff04ff17ff8080ff8080
    808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04
    ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff80808080
    80ffff01ff0bffff0101ff058080ff0180ff04ff2cffff04ffff0112ffff04ff
    80ffff04ffff0bff81aaffff0bff3cffff0bff3cff81caff0580ffff0bff3cff
    ff0bff81eaffff0bff3cffff0bff3cff81caffff0bffff0101ff0b8080ffff0b
    ff3cff81caff818a808080ff818a808080ff8080808080ff018080
    "
);

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    907c64355ccafb37340aea49a4a6ed618e985784030b9d55d00fc9a3a2ec1a0f
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesInitiateUpdateActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_mod_hash: Bytes32,
    pub relative_block_height: u32,
    pub handle_slot_1st_curry_hash: Bytes32,
    pub update_slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesInitiateUpdateActionSolution {
    pub current_slot_value: XchandlesHandleSlotValue,
    pub new_data: XchandlesDataValue,
    pub current_owner: CoinProof,
    #[clvm(rest)]
    pub min_height: u32,
}

impl Mod for XchandlesInitiateUpdateActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_INITIATE_UPDATE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH
    }
}
