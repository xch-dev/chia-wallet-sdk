use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{
        PuzzleAndSolution, SlotNeigborsInfo, XchandlesDataValue, XchandlesNewDataPuzzleHashes,
        XchandlesOtherPrecommitData,
    },
};

pub const XCHANDLES_REGISTER_PUZZLE: [u8; 1478] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff03ffff22ffff22ffff22ffff22ffff22ffff
    09ff82017fffff0bffff0101ff82bbff8080ffff20ff825bff8080ffff0aff82
    017fff8204ff8080ffff0aff8206ffff82017f8080ffff09ff8202bfffff02ff
    09ffff04ff09ff8209ff80808080ffff09ff8205bfffff02ff09ffff04ff09ff
    8213ff80808080ffff01ff02ffff01ff02ffff01ff04ff8202ffffff04ffff04
    ffff0151ffff04ff82afffff808080ffff04ffff04ffff0142ffff04ffff0112
    ffff04ff80ffff04ffff02ff57ffff04ff81f7ffff04ff82017fffff04ffff0b
    ffff0101ffff02ff27ffff04ff27ffff04ff829fffffff04ffff04ff8213ffff
    ff04ff83015fffff821bff8080ff8301dfff8080808080ff8080808080ff8080
    808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff
    57ffff04ff81f7ffff04ff82017fffff04ffff0bffff0101ffff02ff27ffff04
    ff27ffff04ff83013fffffff04ffff04ff821bffffff04ff8213ffff8302bfff
    8080ff8303bfff8080808080ff8080808080ff8080808080ffff04ffff02ff81
    b7ffff04ffff04ff57ff81f780ffff04ff82017fffff02ff27ffff04ff27ffff
    04ff80ffff04ffff04ff8205ffff820bff80ffff04ffff10ff82afffff1b80ff
    8302ffff8080808080808080ffff04ffff02ff81b7ffff04ffff04ff57ff81f7
    80ffff04ff82017fffff02ff27ffff04ff27ffff04ffff10ff829fffffff0101
    80ffff04ffff04ff8213ffffff04ff83015fffff8205ff8080ff8301dfff8080
    8080808080ffff04ffff02ff81b7ffff04ffff04ff57ff81f780ffff04ff8201
    7fffff02ff27ffff04ff27ffff04ffff10ff83013fffffff010180ffff04ffff
    04ff821bffffff04ff8205ffff8302bfff8080ff8303bfff80808080808080ff
    ff04ffff04ffff0142ffff04ffff0113ffff04ffff0101ffff04ff02ffff04ff
    13ff808080808080ffff04ffff04ffff013effff04ffff0effff0172ff0280ff
    808080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0161ff02
    80ffff04ff09ff8080808080ffff03ffff09ff09ff0d80ff80ffff04ffff04ff
    ff0143ffff04ffff0112ffff04ffff0effff0162ff0280ffff04ff0dff808080
    8080ff80808080808080808080808080ffff04ffff02ff8213ffffff04ffff02
    ff2bffff04ff7bffff04ff5fffff04ff8302ffffffff04ffff0bffff0101ffff
    02ff13ffff04ff13ffff04ffff04ffff04ff82057fff821bff80ffff04ff820b
    7fff8237ff8080ffff04ffff04ff830177ffff8303ffff80ff83017fff808080
    8080ff808080808080ff821bff8080ff018080ffff04ffff04ffff02ff15ffff
    04ff3dffff04ff0bffff04ffff02ff09ffff04ff09ffff04ff0bffff04ff8301
    3fffff1780808080ffff04ff829fffff808080808080ffff02ff15ffff04ff3d
    ffff04ff0bffff04ffff02ff09ffff04ff09ffff04ff0bffff04ff8301bfffff
    1780808080ffff04ff82dfffff80808080808080ff018080ffff01ff088080ff
    0180ffff04ffff02ff8209ffff820dff80ff018080ffff04ffff04ffff01ff02
    ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080
    ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ff
    ff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bff
    ff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff
    02ff078080ffff0bffff010180808080ffff04ffff01ff04ffff0133ffff04ff
    ff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff0101ff0780ff80808080
    80ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff01ff02ffff03ff
    03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff
    0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02
    ff078080ffff0bffff010180808080ffff01ff0bffff018201018080ff018080
    8080ff018080
    "
);

pub const XCHANDLES_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8
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

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRestOfSlot {
    pub this_counter: u64,
    pub this_this_value: Bytes32, // left_left_value or right_right_value
    pub this_expiration: u64,     // left_expiration or right_expiration
    #[clvm(rest)]
    pub this_data: XchandlesDataValue, // left_data or right_data
}

impl XchandlesRestOfSlot {
    pub fn new(
        this_counter: u64,
        this_this_value: Bytes32,
        this_expiration: u64,
        this_data: XchandlesDataValue,
    ) -> Self {
        Self {
            this_counter,
            this_this_value,
            this_expiration,
            this_data,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRegisterActionSolution<PP, PS, CMP, CMS, S> {
    pub handle_hash: Bytes32,
    pub neighbors: SlotNeigborsInfo,
    pub cat_maker_puzzle_and_solution: PuzzleAndSolution<CMP, CMS>,
    pub pricing_puzzle_and_solution: PuzzleAndSolution<PP, PS>,
    pub left_rest_of_slot: XchandlesRestOfSlot,
    pub right_rest_of_slot: XchandlesRestOfSlot,
    pub data_puzzle_hashes: XchandlesNewDataPuzzleHashes,
    #[clvm(rest)]
    pub other_precommit_data: XchandlesOtherPrecommitData<S>,
}

impl Mod for XchandlesRegisterActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_REGISTER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_REGISTER_PUZZLE_HASH
    }
}
