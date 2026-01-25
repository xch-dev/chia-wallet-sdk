use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{PuzzleAndSolution, SlotNeigborsInfo, XchandlesDataValue},
    Mod,
};

pub const XCHANDLES_EXTEND_PUZZLE: [u8; 959] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff81afffff02ff2effff04ff02ffff04
    ff8202ffff8080808080ffff09ff82016fffff02ff2effff04ff02ffff04ff81
    9fff808080808080ffff01ff04ff2fffff04ffff02ff3effff04ff02ffff04ff
    17ffff04ffff02ff2effff04ff02ffff04ffff04ffff04ffff0bffff0101ff82
    05df80ff81bf80ffff04ff8202dfff82017f8080ff80808080ff8080808080ff
    ff04ffff04ff3cffff04ffff0effff0165ffff02ff2effff04ff02ffff04ffff
    04ffff05ffff02ff819fff81df8080ff8205df80ff8080808080ff808080ffff
    04ffff04ff10ffff04ff8202dfff808080ffff04ffff04ff14ffff04ff82015f
    ff808080ffff04ffff02ff16ffff04ff02ffff04ff17ffff04ffff02ff2effff
    04ff02ffff04ffff04ffff04ffff0bffff0101ff8205df80ff81bf80ffff04ff
    ff10ff8202dfffff06ffff02ff819fff81df808080ff82017f8080ff80808080
    ff8080808080ffff04ffff04ff18ffff04ffff0bffff02ff8202ffffff04ff05
    ff8203ff8080ffff02ff2effff04ff02ffff04ffff04ffff02ff2effff04ff02
    ffff04ffff04ff8205dfff8202df80ff80808080ffff04ffff04ff0bffff04ff
    ff05ffff02ff819fff81df8080ffff04ffff04ff0bff8080ff80808080ff8080
    80ff8080808080ff808080ff8080808080808080ffff01ff088080ff0180ffff
    04ffff01ffffff553fff51ff333effff42ff02ffffa04bf5122f344554c53bde
    2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d1
    1a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210
    fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63
    fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff
    2cffff04ffff0bff81baffff0bff2affff0bff2aff81daff0580ffff0bff2aff
    ff0bff81faffff0bff2affff0bff2aff81daffff0bffff0101ff0b8080ffff0b
    ff2aff81daff819a808080ff819a808080ffff04ff80ffff04ffff04ff05ff80
    80ff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff
    2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff
    8080808080ffff01ff0bffff0101ff058080ff0180ff04ff12ffff04ffff0112
    ffff04ff80ffff04ffff0bff81baffff0bff2affff0bff2aff81daff0580ffff
    0bff2affff0bff81faffff0bff2affff0bff2aff81daffff0bffff0101ff0b80
    80ffff0bff2aff81daff819a808080ff819a808080ff8080808080ff018080
    "
);

pub const XCHANDLES_EXTEND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    479da4ab9f1072c233008912d6b88d9a2474f1c52b07e73176b5ba316e26bfcd
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
