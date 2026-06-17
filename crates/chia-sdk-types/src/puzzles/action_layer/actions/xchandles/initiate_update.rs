use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
    Mod,
};

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE: [u8; 807] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff01ff04ff8202ffffff04ffff04ffff0155ff
    ff04ff822dffff808080ffff04ffff04ffff0157ffff04ff821fffff808080ff
    ff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff2bffff04
    ff5bffff04ff81bfffff04ffff0bffff0101ffff02ff13ffff04ff13ff8205ff
    808080ff8080808080ff8080808080ffff04ffff02ff7bffff04ffff04ff2bff
    5b80ffff04ff81bfffff02ff13ffff04ff13ffff04ffff10ff8209ffffff0101
    80ff820dff808080808080ffff04ffff04ffff0143ffff04ffff013affff04ff
    ff0effff0169ff0580ffff04ff02ff8080808080ffff04ffff04ffff0133ffff
    04ffff02ff2bffff04ff5bffff04ff82017fffff04ffff0bffff0101ffff0bff
    ff0102ffff02ff13ffff04ff13ffff04ff02ffff10ff821fffff5f80808080ff
    058080ff8080808080ffff04ff80ffff04ffff04ff02ff8080ff8080808080ff
    8080808080808080ffff04ffff30ff8213ffffff02ff15ffff04ff2dffff04ff
    0bffff04ffff02ff09ffff04ff09ffff04ff0bffff04ff822effff1780808080
    ffff04ff822bffff808080808080ff823bff80ff018080ffff04ffff02ff04ff
    ff04ff04ffff04ff82097fff8202ff808080ff018080ffff04ffff04ffff01ff
    02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff0580
    80ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180
    ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0b
    ffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04
    ff02ff078080ffff0bffff010180808080ffff04ffff01ff02ffff03ff03ffff
    01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ff
    ff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff0780
    80ffff0bffff010180808080ffff01ff0bffff018201018080ff0180ffff01ff
    04ffff0133ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff01
    01ff0780ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080
    808080ff018080
    "
);

pub const XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    4decc3b4b1de7ae78927b039cbfb6af049df4f5d1b10aa968e80be73ae11b1d8
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
