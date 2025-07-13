use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{SlotNeigborsInfo, XchandlesDataValue},
    Mod,
};

pub const XCHANDLES_EXTEND_PUZZLE: [u8; 964] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff81afffff02ff2effff04ff02ffff04
    ff8202dfff8080808080ffff09ff82016fffff02ff2effff04ff02ffff04ff81
    9fff808080808080ffff01ff04ff2fffff04ffff02ff3effff04ff02ffff04ff
    17ffff04ffff02ff2effff04ff02ffff04ffff04ffff04ffff0bffff0101ff82
    0b5f80ff820bdf80ffff04ff82055fff820fdf8080ff80808080ff8080808080
    ffff04ffff04ff3cffff04ffff0effff0165ffff02ff2effff04ff02ffff04ff
    ff04ffff05ffff02ff819fff82015f8080ff820b5f80ff8080808080ff808080
    ffff04ffff04ff10ffff04ff82055fff808080ffff04ffff04ff14ffff04ff82
    025fff808080ffff04ffff02ff16ffff04ff02ffff04ff17ffff04ffff02ff2e
    ffff04ff02ffff04ffff04ffff04ffff0bffff0101ff820b5f80ff820bdf80ff
    ff04ffff10ff82055fffff06ffff02ff819fff82015f808080ff820fdf8080ff
    80808080ff8080808080ffff04ffff04ff18ffff04ffff0bffff02ff8202dfff
    ff04ff05ff8205df8080ffff02ff2effff04ff02ffff04ffff04ffff02ff2eff
    ff04ff02ffff04ffff04ff820b5fff82055f80ff80808080ffff04ffff04ff0b
    ffff04ffff05ffff02ff819fff82015f8080ffff04ffff04ff0bff8080ff8080
    8080ff808080ff8080808080ff808080ff8080808080808080ffff01ff088080
    ff0180ffff04ffff01ffffff553fff51ff333effff42ff02ffffa04bf5122f34
    4554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a1
    84f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a1
    2871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a1
    02a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7
    c5ffff04ff2cffff04ffff0bff81baffff0bff2affff0bff2aff81daff0580ff
    ff0bff2affff0bff81faffff0bff2affff0bff2aff81daffff0bffff0101ff0b
    8080ffff0bff2aff81daff819a808080ff819a808080ffff04ff80ffff04ffff
    04ff05ff8080ff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff01
    02ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ff
    ff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff12ffff
    04ffff0112ffff04ff80ffff04ffff0bff81baffff0bff2affff0bff2aff81da
    ff0580ffff0bff2affff0bff81faffff0bff2affff0bff2aff81daffff0bffff
    0101ff0b8080ffff0bff2aff81daff819a808080ff819a808080ff8080808080
    ff018080
    "
);

pub const XCHANDLES_EXTEND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    caa665c939f3de5d90dd22b00d092ba7c794300bf994b9ddcea536fa77843e08
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExtendActionArgs {
    pub offer_mod_hash: Bytes32,
    pub payout_puzzle_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExtendActionSolution<PP, PS, CMP, CMS> {
    pub pricing_puzzle_reveal: PP,
    pub pricing_solution: PS,
    pub cat_maker_puzzle_reveal: CMP,
    pub cat_maker_solution: CMS,
    pub neighbors: SlotNeigborsInfo,
    #[clvm(rest)]
    pub rest: XchandlesDataValue,
}

impl Mod for XchandlesExtendActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXTEND_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXTEND_PUZZLE_HASH
    }
}
