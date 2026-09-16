use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{PuzzleAndSolution, SlotNeigborsInfo, XchandlesDataValue},
};

pub const XCHANDLES_EXTEND_PUZZLE: [u8; 887] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff03ffff22ffff09ffff02ff09ffff04ff09ff
    820bff8080ff82015f80ffff09ffff02ff09ffff04ff09ff82013f8080ff8202
    df8080ffff01ff04ff5fffff04ffff04ffff0142ffff04ffff0112ffff04ff80
    ffff04ffff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff
    0102ffff0bffff0182010180ff2f80ffff0bffff0102ffff02ff15ffff04ff15
    ffff04ffff0bffff0101ffff02ff09ffff04ff09ffff04ff82017fffff04ffff
    04ffff0bffff0101ff820bbf80ff8202ff80ffff04ff8205bfff8205ff808080
    808080ff80808080ffff0bffff010180808080ff8080808080ffff04ffff04ff
    ff013effff04ffff0effff0165ffff02ff09ffff04ff09ffff04ff04ff820bbf
    80808080ff808080ffff04ffff04ffff0155ffff04ff8205bfff808080ffff04
    ffff04ffff0151ffff04ff8202bfff808080ffff04ffff02ff1dffff04ff15ff
    ff04ff2fffff02ff09ffff04ff09ffff04ffff10ff82017fffff010180ffff04
    ffff04ffff0bffff0101ff820bbf80ff8202ff80ffff04ffff10ff8205bfff06
    80ff8205ff8080808080808080ffff04ffff04ffff013fffff04ffff0bffff02
    ff820bffffff04ff0bff820fff8080ffff02ff09ffff04ff09ffff04ffff02ff
    09ffff04ff09ffff04ff820bbfff8205bf808080ffff04ffff04ff17ffff04ff
    04ffff04ffff04ff17ff8080ff80808080ff808080808080ff808080ff808080
    8080808080ffff01ff088080ff0180ffff04ffff02ff819fff81df80ff018080
    ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02
    ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bff
    ff0101ff038080ff0180ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102
    ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff018201
    0180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01
    0180808080ffff01ff0bffff018201018080ff0180ffff01ff04ffff0133ffff
    04ffff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102
    ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ffff
    04ffff0bffff0101ff0780ff80808080ffff0bffff010180808080ffff04ff80
    ffff04ffff04ff05ff8080ff80808080808080ff018080

    "
);

pub const XCHANDLES_EXTEND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    490522c6759f840a455ff5ca5a00f3eb83db2dfd3c4590553d7ff67d210bc353
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExtendActionArgs {
    pub offer_mod_hash: Bytes32,
    pub payout_puzzle_hash: Bytes32,
    pub handle_slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExtendActionSolution<PP, PS, CMP, CMS> {
    pub pricing_puzzle_and_solution: PuzzleAndSolution<PP, PS>,
    pub counter: u64,
    pub neighbors: SlotNeigborsInfo,
    pub rest: XchandlesDataValue,
    #[clvm(rest)]
    pub cat_maker_and_solution: PuzzleAndSolution<CMP, CMS>,
}

impl Mod for XchandlesExtendActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXTEND_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXTEND_PUZZLE_HASH
    }
}
