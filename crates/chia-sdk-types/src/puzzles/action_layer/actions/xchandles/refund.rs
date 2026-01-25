use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{PuzzleHashPuzzleAndSolution, XchandlesHandleSlotValue, XchandlesOtherPrecommitData},
    Mod,
};

pub const XCHANDLES_REFUND_PUZZLE: [u8; 1057] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff819fffff02ff2effff04ff02ffff04
    ff82015fff8080808080ffff09ff4fffff02ff2effff04ff02ffff04ff81afff
    8080808080ffff02ffff03ff8202ffffff01ff09ff8208ffffff0bffff0101ff
    81bf8080ffff01ff010180ff018080ffff01ff04ff17ffff02ff16ffff04ff02
    ffff04ff0bffff04ffff02ff2effff04ff02ffff04ff8202ffff80808080ffff
    04ffff02ffff03ffff22ffff09ff819fff5780ffff09ff81bfff8205ef80ffff
    21ffff09ff4fff81b780ffff09ff4fff81f78080ffff09ff82017fffff05ffff
    02ff81afff81ef80808080ffff01820affff8080ff0180ffff04ffff02ff8201
    5fffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0b
    ff72ffff0bff3cffff0bff3cff62ff820bff80ffff0bff3cffff0bff72ffff0b
    ff3cffff0bff3cff62ffff0bffff0101ffff02ff2effff04ff02ffff04ffff04
    ffff04ffff04ff819fff8201df80ffff04ff4fff81ef8080ffff04ffff04ff81
    bfff820fff80ffff04ff8209ffff820dff808080ff808080808080ffff0bff3c
    ff62ff42808080ff42808080ff42808080ff8201df8080ffff04ff82017fff80
    8080808080808080ffff01ff088080ff0180ffff04ffff01ffffff5533ff3eff
    4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7
    cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a1
    6d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b2
    3759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a
    6e7298a91ce119a63400ade7c5ff04ff18ffff04ffff0bff52ffff0bff3cffff
    0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff
    0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ffff04ff80ff
    ff04ffff04ff05ff8080ff8080808080ffff04ffff04ff2cffff04ffff0113ff
    ff04ff80ffff04ff2fffff04ff5fff808080808080ffff04ffff04ff14ffff04
    ffff0effff0124ff2f80ff808080ffff02ffff03ff17ffff01ff04ffff04ff10
    ffff04ff17ff808080ffff04ffff02ff3effff04ff02ffff04ff05ffff04ff0b
    ff8080808080ffff04ffff02ff1affff04ff02ffff04ff05ffff04ff0bff8080
    808080ff80808080ff8080ff01808080ffff02ffff03ffff07ff0580ffff01ff
    0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff
    04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04
    ff2cffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff3cffff0bff3c
    ff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff
    0101ff0b8080ffff0bff3cff62ff42808080ff42808080ff8080808080ff0180
    80
    "
);

pub const XCHANDLES_REFUND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    02fd6f79996cb4a5adade14b7e0ef6239f49f1c657b68094c09583b2200b89e0
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRefundActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub handle_slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRefundActionSolution<CMP, CMS, PP, PS, S> {
    pub precommited_pricing_puzzle_and_solution: PuzzleHashPuzzleAndSolution<PP, PS>,
    pub precommited_cat_maker_and_solution: PuzzleHashPuzzleAndSolution<CMP, CMS>,
    pub handle: String,
    pub precommit_amount: u64,
    pub slot_value: Option<XchandlesHandleSlotValue>,
    #[clvm(rest)]
    pub other_precommit_data: XchandlesOtherPrecommitData<S>,
}

impl Mod for XchandlesRefundActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_REFUND_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_REFUND_PUZZLE_HASH
    }
}
