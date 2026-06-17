use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
    Mod,
};

pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE: [u8; 996] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ff81bfffff04ffff04ffff0155ffff04ff8216ff
    ff808080ffff04ffff04ffff0153ffff04ff82017fff808080ffff04ffff04ff
    ff0142ffff04ffff0112ffff04ff80ffff04ffff02ff15ffff04ff2dffff04ff
    2fffff04ffff0bffff0101ffff02ff09ffff04ff09ff8202ff808080ff808080
    8080ff8080808080ffff04ffff02ff3dffff04ffff04ff15ff2d80ffff04ff2f
    ffff02ff09ffff04ff09ffff04ffff10ff8204ffffff010180ffff04ff820aff
    ffff04ff8216ffff8205ff8080808080808080ffff04ffff04ffff0143ffff04
    ffff013affff04ffff0effff0175ff0280ffff04ffff30ff8213ffffff02ff15
    ffff04ff2dffff04ff0bffff04ffff02ff09ffff04ff09ffff04ff0bffff04ff
    822effff1780808080ffff04ff822bffff808080808080ff823bff80ff808080
    8080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff016fff0280
    ffff04ffff02ff15ffff04ff2dffff04ff0bffff04ffff02ff09ffff04ff09ff
    ff04ff0bffff04ff8209ffff1780808080ffff04ff8217ffff808080808080ff
    8080808080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0172
    ff0280ffff04ffff02ff15ffff04ff2dffff04ff0bffff04ffff02ff09ffff04
    ff09ffff04ff0bffff04ff820dffff1780808080ffff04ff821fffff80808080
    8080ff8080808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff
    04ffff02ff15ffff04ff2dffff04ff5fffff04ffff0bffff0101ffff0bffff01
    02ffff02ff09ffff04ff09ffff04ff8213ffff82017f808080ff028080ff8080
    808080ff8080808080ff80808080808080808080ffff04ffff02ff04ffff04ff
    04ffff04ff82097fff8202ff808080ff018080ffff04ffff04ffff01ff02ffff
    03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff
    02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff04
    ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff01
    02ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff
    078080ffff0bffff010180808080ffff04ffff01ff02ffff03ff03ffff01ff0b
    ffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bff
    ff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff
    0bffff010180808080ffff01ff0bffff018201018080ff0180ffff01ff04ffff
    0133ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff0101ff07
    80ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080808080
    ff018080
    "
);

pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    743369017c9e26460703c2ef09c8f78891157925fd131fde589bb4c54058c0a7
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
