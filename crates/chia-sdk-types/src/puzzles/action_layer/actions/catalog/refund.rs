use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::SlotNeigborsInfo, Mod};

pub const CATALOG_REFUND_PUZZLE: [u8; 922] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff4fffff02ff2effff04ff02ffff04ff81afff
    8080808080ffff01ff04ff17ffff02ff16ffff04ff02ffff04ff0bffff04ffff
    02ff2effff04ff02ffff04ffff04ff8202efff821fef80ff80808080ffff04ff
    ff22ffff09ff77ff8217ef80ffff09ff4fff578080ffff04ffff04ffff04ff28
    ffff04ffff0effff0124ffff02ff2effff04ff02ffff04ffff04ff8202efff82
    05ef80ff8080808080ff808080ffff04ffff04ff38ffff04ffff0113ffff04ff
    80ffff04ffff02ff81afffff04ffff02ff2affff04ff02ffff04ff05ffff04ff
    820befffff04ffff0bffff0102ff8202efffff0bffff0101ffff02ff2effff04
    ff02ffff04ffff04ff8205efffff04ff4fff82016f8080ff808080808080ff80
    8080808080ff82016f8080ffff04ff8217efff808080808080ff808080ff8080
    808080808080ffff01ff088080ff0180ffff04ffff01ffffff33ff3e42ff02ff
    ff02ffff03ff05ffff01ff0bff81fcffff02ff3affff04ff02ffff04ff09ffff
    04ffff02ff2cffff04ff02ffff04ff0dff80808080ff808080808080ffff0181
    dc80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5
    d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878
    a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4
    b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b715
    2a6e7298a91ce119a63400ade7c5ffffff04ff10ffff04ffff02ff2affff04ff
    02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff
    04ffff04ff05ff8080ff8080808080ffff0bff81bcffff02ff3affff04ff02ff
    ff04ff05ffff04ffff02ff2cffff04ff02ffff04ff07ff80808080ff80808080
    8080ff0bff14ffff0bff14ff81dcff0580ffff0bff14ff0bff819c8080ffff02
    ffff03ff17ffff01ff04ffff02ff3effff04ff02ffff04ff05ffff04ff0bff80
    80808080ffff04ffff02ff12ffff04ff02ffff04ff05ffff04ff0bff80808080
    80ff2f8080ffff012f80ff0180ffff02ffff03ffff07ff0580ffff01ff0bffff
    0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02
    ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff38ff
    ff04ffff0112ffff04ff80ffff04ffff02ff2affff04ff02ffff04ff05ffff04
    ffff0bffff0101ff0b80ff8080808080ff8080808080ff018080
    "
);

pub const CATALOG_REFUND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3d4aefac7d53b8d36802d5e03aa4a12301fc7eadab60a497311fe7995c2ebf32
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
pub struct CatalogRefundActionSolution<P, S> {
    pub precommited_cat_maker_hash: Bytes32,
    pub precommited_cat_maker_reveal: P,
    pub precommited_cat_maker_solution: S,
    pub tail_hash: Bytes32,
    pub initial_nft_owner_ph: Bytes32,
    pub refund_puzzle_hash_hash: Bytes32,
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
