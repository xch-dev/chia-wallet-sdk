use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
    Mod,
};

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE: [u8; 1001] = hex!(
    "
    ff02ffff01ff04ff81bfffff04ffff04ff30ffff04ff82057fff808080ffff04
    ffff04ff20ffff04ff8207ffff808080ffff02ff2cffff04ff02ffff04ff2fff
    ff04ffff02ff2effff04ff02ffff04ff82017fff80808080ffff04ffff02ff32
    ffff04ff02ffff04ff5fffff04ffff10ff8207ffff1780ffff04ffff02ff2eff
    ff04ff02ffff04ffff04ff82047fff8202ff80ff80808080ffff04ffff30ff82
    09ffffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff
    04ffff04ff05ffff04ff820b7fff0b8080ff80808080ffff04ff8215ffff8080
    80808080ff821dff80ff80808080808080ff808080808080808080ffff04ffff
    01ffffffff5755ff3343ffff4202ffff04ffff02ff3effff04ff02ffff04ff05
    ffff04ff0bff8080808080ffff04ffff02ff2affff04ff02ffff04ff05ffff04
    ff0bff8080808080ff178080ff02ffff03ff05ffff01ff0bff81e2ffff02ff36
    ffff04ff02ffff04ff09ffff04ffff02ff3cffff04ff02ffff04ff0dff808080
    80ff808080808080ffff0181c280ff0180ffffffffffa04bf5122f344554c53b
    de2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623
    d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee2
    10fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd
    63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff
    ff04ff38ffff04ffff013affff04ffff0effff0169ff1780ffff04ff2fff8080
    808080ffff04ffff02ff3affff04ff02ffff04ff05ffff04ffff0bffff0102ff
    ff0bffff0102ffff0bffff0101ff2f80ffff0bffff0101ff0b8080ff1780ffff
    04ff2fff808080808080ff808080ffff04ff28ffff04ffff02ff26ffff04ff02
    ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04
    ffff04ff05ff8080ff8080808080ff04ff28ffff04ffff02ff26ffff04ff02ff
    ff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ff
    ff04ff17ff8080ff8080808080ffffff0bff81a2ffff02ff36ffff04ff02ffff
    04ff05ffff04ffff02ff3cffff04ff02ffff04ff07ff80808080ff8080808080
    80ff0bff34ffff0bff34ff81c2ff0580ffff0bff34ff0bff81828080ffff02ff
    ff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09
    ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bff
    ff0101ff058080ff0180ff04ff24ffff04ffff0112ffff04ff80ffff04ffff02
    ff26ffff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff
    8080808080ff018080
    "
);

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    61203aff9a5dc943e59eeb8abf8a16968129764352c1dbe99a748ad143de9e14
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
    pub current_owner: CompactCoinProof,
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
