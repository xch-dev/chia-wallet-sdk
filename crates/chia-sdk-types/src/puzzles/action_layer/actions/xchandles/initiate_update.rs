use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
};

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE: [u8; 791] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff01ff04ff8202ffffff04ffff04ffff0155ff
    ff04ff822dffff808080ffff04ffff04ffff0157ffff04ff821fffff808080ff
    ff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff2bffff04
    ff5bffff04ff81bfffff04ffff0bffff0101ffff02ff13ffff04ff13ff8205ff
    808080ff8080808080ff8080808080ffff04ffff02ff7bffff04ffff04ff2bff
    5b80ffff04ff81bfffff02ff13ffff04ff13ffff04ffff10ff8209ffffff0101
    80ff820dff808080808080ffff04ffff04ffff0143ffff04ffff013affff04ff
    ff0effff0169ff0280ffff04ff05ff8080808080ffff04ffff04ffff0133ffff
    04ffff02ff2bffff04ff5bffff04ff82017fffff04ffff0bffff0101ff0280ff
    8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ff80808080
    80808080ffff04ffff02ff09ffff04ff09ffff04ffff04ff02ffff10ff820fff
    ff2f8080ffff04ff8212ffff8205ff80808080ff018080ffff04ffff30ff8209
    ffffff02ff0affff04ff16ffff04ff05ffff04ffff02ff04ffff04ff04ffff04
    ff05ffff04ff82177fff0b80808080ffff04ff8215ffff808080808080ff821d
    ff80ff018080ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bff
    ff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080
    ffff01ff0bffff0101ff038080ff0180ffff04ffff01ff0bffff0102ffff0bff
    ff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff05
    80ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff0101808080
    80ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff01820104
    80ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bff
    ff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff
    0bffff018201018080ff0180ffff01ff04ffff0133ffff04ffff02ff04ffff04
    ff06ffff04ff05ffff04ffff0bffff0101ff0780ff8080808080ffff04ff80ff
    ff04ffff04ff05ff8080ff8080808080808080ff018080
    "
);

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    23eeec5ecd731c2822adf82f23a2e73fdd4ad08df56558ad15873c68cadf68f8
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
