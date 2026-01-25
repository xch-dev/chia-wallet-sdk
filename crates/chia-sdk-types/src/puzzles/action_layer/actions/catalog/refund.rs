use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::SlotNeigborsInfo, Mod};

pub const CATALOG_REFUND_PUZZLE: [u8; 914] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff4fffff02ff2effff04ff02ffff04ff81afff
    8080808080ffff01ff04ff17ffff02ff16ffff04ff02ffff04ff0bffff04ffff
    02ff2effff04ff02ffff04ffff04ff819fff81ff80ff80808080ffff04ffff22
    ffff09ff77ff81bf80ffff09ff4fff578080ffff04ffff04ffff04ff28ffff04
    ffff0effff0124ffff02ff2effff04ff02ffff04ffff04ff819fff82015f80ff
    8080808080ff808080ffff04ffff04ff38ffff04ffff0113ffff04ff80ffff04
    ffff02ff81afffff04ffff02ff2affff04ff02ffff04ff05ffff04ff8201dfff
    ff04ffff0bffff0102ff819fffff0bffff0101ffff02ff2effff04ff02ffff04
    ffff04ff82015fffff04ff4fff81ef8080ff808080808080ff808080808080ff
    81ef8080ffff04ff81bfff808080808080ff808080ff8080808080808080ffff
    01ff088080ff0180ffff04ffff01ffffff33ff3e42ff02ffff02ffff03ff05ff
    ff01ff0bff81fcffff02ff3affff04ff02ffff04ff09ffff04ffff02ff2cffff
    04ff02ffff04ff0dff80808080ff808080808080ffff0181dc80ff0180ffffa0
    4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a
    a09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7
    b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f68069
    23f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119
    a63400ade7c5ffffff04ff10ffff04ffff02ff2affff04ff02ffff04ff05ffff
    04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff05ff80
    80ff8080808080ffff0bff81bcffff02ff3affff04ff02ffff04ff05ffff04ff
    ff02ff2cffff04ff02ffff04ff07ff80808080ff808080808080ff0bff14ffff
    0bff14ff81dcff0580ffff0bff14ff0bff819c8080ffff02ffff03ff17ffff01
    ff04ffff02ff3effff04ff02ffff04ff05ffff04ff0bff8080808080ffff04ff
    ff02ff12ffff04ff02ffff04ff05ffff04ff0bff8080808080ff2f8080ffff01
    2f80ff0180ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2eff
    ff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080
    808080ffff01ff0bffff0101ff058080ff0180ff04ff38ffff04ffff0112ffff
    04ff80ffff04ffff02ff2affff04ff02ffff04ff05ffff04ffff0bffff0101ff
    0b80ff8080808080ff8080808080ff018080
    "
);

pub const CATALOG_REFUND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    7073bdb00158fb9cfbac3b6121760bc052d1b099f28deebf61c60ea688b13319
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct CatalogRefundActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogPrecommitCatMakerDataWithHash<P, S> {
    pub precommited_cat_maker_hash: Bytes32,
    pub precommited_cat_maker_reveal: P,
    #[clvm(rest)]
    pub precommited_cat_maker_solution: S,
}

impl<P, S> CatalogPrecommitCatMakerDataWithHash<P, S> {
    pub fn new(
        precommited_cat_maker_hash: Bytes32,
        precommited_cat_maker_reveal: P,
        precommited_cat_maker_solution: S,
    ) -> Self {
        Self {
            precommited_cat_maker_hash,
            precommited_cat_maker_reveal,
            precommited_cat_maker_solution,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogOtherPrecommitData {
    pub tail_hash: Bytes32,
    pub initial_nft_owner_ph: Bytes32,
    #[clvm(rest)]
    pub refund_puzzle_hash_hash: Bytes32,
}

impl CatalogOtherPrecommitData {
    pub fn new(
        tail_hash: Bytes32,
        initial_nft_owner_ph: Bytes32,
        refund_puzzle_hash_hash: Bytes32,
    ) -> Self {
        Self {
            tail_hash,
            initial_nft_owner_ph,
            refund_puzzle_hash_hash,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogRefundActionSolution<P, S> {
    pub precommited_cat_maker_data: CatalogPrecommitCatMakerDataWithHash<P, S>,
    pub other_precommit_data: CatalogOtherPrecommitData,
    pub precommit_amount: u64,
    #[clvm(rest)]
    pub neighbors: Option<SlotNeigborsInfo>,
}

impl Mod for CatalogRefundActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&CATALOG_REFUND_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        CATALOG_REFUND_PUZZLE_HASH
    }
}
