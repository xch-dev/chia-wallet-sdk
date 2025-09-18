use std::borrow::Cow;

use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::XchandlesSlotValue, Mod};

pub const XCHANDLES_REFUND_PUZZLE: [u8; 1075] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff81afffff02ff2effff04ff02ffff04
    ff4fff8080808080ffff09ff8205efffff02ff2effff04ff02ffff04ff8202ef
    ff8080808080ffff02ffff03ff8303ffefffff01ff09ff8309ffefffff0bffff
    0101ff8217ef8080ffff01ff010180ff018080ffff01ff04ff17ffff02ff16ff
    ff04ff02ffff04ff0bffff04ffff02ff2effff04ff02ffff04ff8303ffefff80
    808080ffff04ffff02ffff03ffff22ffff09ff81afff5780ffff09ff8217efff
    825bef80ffff21ffff09ff8205efff81b780ffff09ff8205efff81f78080ffff
    09ff8302ffefffff05ffff02ff8202efff820bef80808080ffff01830bffefff
    8080ff0180ffff04ffff02ff4fffff04ffff0bff52ffff0bff3cffff0bff3cff
    62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ff83017fef80
    ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ffff02
    ff2effff04ff02ffff04ffff04ffff04ffff04ff81afff82016f80ffff04ff82
    05efff820bef8080ffff04ffff04ff8217efff822fef80ffff04ff825fefff82
    bfef808080ff808080808080ffff0bff3cff62ff42808080ff42808080ff4280
    8080ff82016f8080ffff04ff8302ffefff808080808080808080ffff01ff0880
    80ff0180ffff04ffff01ffffff5533ff3eff4202ffffffffa04bf5122f344554
    c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f3
    2623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871
    fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8
    d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff
    04ff18ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cff
    ff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3c
    ff62ff42808080ff42808080ffff04ff80ffff04ffff04ff05ff8080ff808080
    8080ffff04ffff04ff2cffff04ffff0113ffff04ff80ffff04ff2fffff04ff5f
    ff808080808080ffff04ffff04ff14ffff04ffff0effff0124ff2f80ff808080
    ffff02ffff03ff17ffff01ff04ffff04ff10ffff04ff17ff808080ffff04ffff
    02ff3effff04ff02ffff04ff05ffff04ff0bff8080808080ffff04ffff02ff1a
    ffff04ff02ffff04ff05ffff04ff0bff8080808080ff80808080ff8080ff0180
    8080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff
    02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080
    ffff01ff0bffff0101ff058080ff0180ff04ff2cffff04ffff0112ffff04ff80
    ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff
    72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff
    42808080ff42808080ff8080808080ff018080
    "
);

pub const XCHANDLES_REFUND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    c1469c124abadf18b0deee827c57f5189bc81d0f59aa07e2290676d0000b20a1
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRefundActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRefundActionSolution<CMP, CMS, PP, PS, S> {
    pub precommited_cat_maker_reveal: CMP,
    pub precommited_cat_maker_hash: Bytes32,
    pub precommited_cat_maker_solution: CMS,
    pub precommited_pricing_puzzle_reveal: PP,
    pub precommited_pricing_puzzle_hash: Bytes32,
    pub precommited_pricing_puzzle_solution: PS,
    pub handle: String,
    pub secret: S,
    pub precommited_owner_launcher_id: Bytes32,
    pub precommited_resolved_data: Bytes,
    pub refund_puzzle_hash_hash: Bytes32,
    pub precommit_amount: u64,
    #[clvm(rest)]
    pub slot_value: Option<XchandlesSlotValue>,
}

impl Mod for XchandlesRefundActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_REFUND_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_REFUND_PUZZLE_HASH
    }
}
