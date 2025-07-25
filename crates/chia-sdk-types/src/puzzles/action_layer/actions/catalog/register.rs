use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH,
    NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{puzzles::ANY_METADATA_UPDATER_HASH, Mod};

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NftPack {
    pub launcher_hash: Bytes32,
    pub singleton_mod_hash: Bytes32,
    pub state_layer_mod_hash: Bytes32,
    pub metadata_updater_hash_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub transfer_program_mod_hash: Bytes32,
    pub royalty_puzzle_hash_hash: Bytes32,
    pub trade_price_percentage: u16,
}

impl NftPack {
    pub fn new(royalty_puzzle_hash_hash: Bytes32, trade_price_percentage: u16) -> Self {
        let meta_updater_hash: Bytes32 = ANY_METADATA_UPDATER_HASH.into();

        Self {
            launcher_hash: SINGLETON_LAUNCHER_HASH.into(),
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            metadata_updater_hash_hash: meta_updater_hash.tree_hash().into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            transfer_program_mod_hash:
                NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH.into(),
            royalty_puzzle_hash_hash,
            trade_price_percentage,
        }
    }
}

pub const CATALOG_REGISTER_PUZZLE: [u8; 1578] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff0aff8205bfff822fbf80ffff0aff82bfbf
    ff8205bf80ffff09ffff02ff2effff04ff02ffff04ff82013fff80808080ff82
    015f8080ffff01ff04ff5fffff02ff32ffff04ff02ffff04ff05ffff04ff8301
    ffbfffff04ff820bbfffff04ffff02ff3affff04ff02ffff04ff0bffff04ffff
    0bffff0101ff8205bf80ff8080808080ffff04ffff04ffff04ff28ffff04ff83
    01ffbfff808080ffff04ffff04ff24ffff04ffff0effff0172ffff02ff2effff
    04ff02ffff04ffff04ff8205bfff820bbf80ff8080808080ff808080ffff04ff
    ff02ff3effff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff
    04ff822fbfffff04ff825fbfff82bfbf8080ff80808080ff8080808080ffff04
    ffff02ff3effff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ff
    ff04ff82bfbfffff04ff822fbfff83017fbf8080ff80808080ff8080808080ff
    ff04ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff
    04ffff04ff8205bfffff04ff822fbfff82bfbf8080ff80808080ff8080808080
    ffff04ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ff
    ff04ffff04ff822fbfffff04ff825fbfff8205bf8080ff80808080ff80808080
    80ffff04ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02
    ffff04ffff04ff82bfbfffff04ff8205bfff83017fbf8080ff80808080ff8080
    808080ffff04ffff04ff34ffff04ffff0113ffff04ffff0101ffff04ffff02ff
    82013fffff04ffff02ff3affff04ff02ffff04ff17ffff04ff8217bfffff04ff
    ff0bffff0102ff8205bfffff0bffff0101ffff02ff2effff04ff02ffff04ffff
    04ff820bbfffff04ff82015fff8202bf8080ff808080808080ff808080808080
    ff8202bf8080ffff04ff8201dfff808080808080ff808080808080808080ff80
    8080808080808080ffff01ff088080ff0180ffff04ffff01ffffff40ff4633ff
    ff3e42ff02ff02ffff03ff05ffff01ff0bff81e2ffff02ff26ffff04ff02ffff
    04ff09ffff04ffff02ff3cffff04ff02ffff04ff0dff80808080ff8080808080
    80ffff0181c280ff0180ffffffffffa04bf5122f344554c53bde2ebb8cd2b7e3
    d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99
    a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eae
    a194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f
    3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ffff04ff38ffff04
    ff2fffff01ff80808080ffff04ffff02ff36ffff04ff02ffff04ff05ffff04ff
    17ffff04ffff30ffff30ff0bff2fff8080ff09ffff010180ff808080808080ff
    5f8080ffff04ff38ffff04ffff02ff3affff04ff02ffff04ff05ffff04ffff0b
    ffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080
    808080ff0bff81a2ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff3cff
    ff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff2cffff0bff2c
    ff81c2ff0580ffff0bff2cff0bff81828080ff04ff10ffff04ffff30ff17ffff
    02ff3affff04ff02ffff04ff15ffff04ffff02ff2effff04ff02ffff04ffff04
    ff15ffff04ff17ff098080ff80808080ffff04ffff02ff3affff04ff02ffff04
    ff2dffff04ffff0bffff0101ff2d80ffff04ff8182ffff04ff5dffff04ffff02
    ff3affff04ff02ffff04ff81bdffff04ffff0bffff0101ff81bd80ffff04ff81
    82ffff04ffff02ff3affff04ff02ffff04ff82017dffff04ffff02ff2effff04
    ff02ffff04ffff04ff15ffff04ff17ff098080ff80808080ffff04ff8202fdff
    ff04ffff0bffff0101ff8205fd80ff80808080808080ffff04ff0bff80808080
    80808080ff8080808080808080ff808080808080ffff010180ff808080ffff02
    ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff
    09ff80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0b
    ffff0101ff058080ff0180ff04ff34ffff04ffff0112ffff04ff80ffff04ffff
    02ff3affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080
    ff8080808080ff018080
    "
);

pub const CATALOG_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    028d83ae6f75c1a1fa40ebc68efb7c983257d0cc3fc7161a3418f63cca934e20
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct CatalogRegisterActionArgs {
    pub nft_pack: NftPack,
    pub uniqueness_prelauncher_1st_curry_hash: Bytes32,
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogRegisterActionSolution<P, S> {
    pub cat_maker_reveal: P,
    pub cat_maker_solution: S,
    pub tail_hash: Bytes32,
    pub initial_nft_owner_ph: Bytes32,
    pub refund_puzzle_hash_hash: Bytes32,
    pub left_tail_hash: Bytes32,
    pub left_left_tail_hash: Bytes32,
    pub right_tail_hash: Bytes32,
    pub right_right_tail_hash: Bytes32,
    #[clvm(rest)]
    pub my_id: Bytes32,
}

impl Mod for CatalogRegisterActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&CATALOG_REGISTER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        CATALOG_REGISTER_PUZZLE_HASH
    }
}
