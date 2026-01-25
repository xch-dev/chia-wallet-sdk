use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH,
    NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{
    clvm_tuple, ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError,
};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{
    puzzles::{CatalogOtherPrecommitData, ANY_METADATA_UPDATER_HASH},
    Mod,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for NftPack {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        #[allow(clippy::type_complexity)]
        let (
            (
                (launcher_hash, singleton_mod_hash),
                (state_layer_mod_hash, metadata_updater_hash_hash),
            ),
            (
                (nft_ownership_layer_mod_hash, transfer_program_mod_hash),
                (royalty_puzzle_hash_hash, trade_price_percentage),
            ),
        ): (
            ((Bytes32, Bytes32), (Bytes32, Bytes32)),
            ((Bytes32, Bytes32), (Bytes32, u16)),
        ) = FromClvm::from_clvm(decoder, node)?;

        Ok(Self {
            launcher_hash,
            singleton_mod_hash,
            state_layer_mod_hash,
            metadata_updater_hash_hash,
            nft_ownership_layer_mod_hash,
            transfer_program_mod_hash,
            royalty_puzzle_hash_hash,
            trade_price_percentage,
        })
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for NftPack {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let obj = clvm_tuple!(
            clvm_tuple!(
                clvm_tuple!(self.launcher_hash, self.singleton_mod_hash,),
                clvm_tuple!(self.state_layer_mod_hash, self.metadata_updater_hash_hash),
            ),
            clvm_tuple!(
                clvm_tuple!(
                    self.nft_ownership_layer_mod_hash,
                    self.transfer_program_mod_hash
                ),
                clvm_tuple!(self.royalty_puzzle_hash_hash, self.trade_price_percentage)
            )
        );

        obj.to_clvm(encoder)
    }
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

pub const CATALOG_REGISTER_PUZZLE: [u8; 1566] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff0aff82027fff8204ff80ffff0aff8209ff
    ff82027f80ffff09ffff02ff2effff04ff02ffff04ff82013fff80808080ff82
    015f8080ffff01ff04ff5fffff02ff32ffff04ff02ffff04ff05ffff04ff8207
    ffffff04ff82057fffff04ffff02ff3affff04ff02ffff04ff0bffff04ffff0b
    ffff0101ff82027f80ff8080808080ffff04ffff04ffff04ff28ffff04ff8207
    ffff808080ffff04ffff04ff24ffff04ffff0effff0172ffff02ff2effff04ff
    02ffff04ffff04ff82027fff82057f80ff8080808080ff808080ffff04ffff02
    ff3effff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ff
    8204ffffff04ff8206ffff8209ff8080ff80808080ff8080808080ffff04ffff
    02ff3effff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04
    ff8209ffffff04ff8204ffff820dff8080ff80808080ff8080808080ffff04ff
    ff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff
    04ff82027fffff04ff8204ffff8209ff8080ff80808080ff8080808080ffff04
    ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ff
    ff04ff8204ffffff04ff8206ffff82027f8080ff80808080ff8080808080ffff
    04ffff02ff2affff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04
    ffff04ff8209ffffff04ff82027fff820dff8080ff80808080ff8080808080ff
    ff04ffff04ff34ffff04ffff0113ffff04ffff0101ffff04ffff02ff82013fff
    ff04ffff02ff3affff04ff02ffff04ff17ffff04ff82077fffff04ffff0bffff
    0102ff82027fffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ff8205
    7fffff04ff82015fff8201bf8080ff808080808080ff808080808080ff8201bf
    8080ffff04ff8201dfff808080808080ff808080808080808080ff8080808080
    80808080ffff01ff088080ff0180ffff04ffff01ffffff40ff4633ffff3e42ff
    02ff02ffff03ff05ffff01ff0bff81e2ffff02ff26ffff04ff02ffff04ff09ff
    ff04ffff02ff3cffff04ff02ffff04ff0dff80808080ff808080808080ffff01
    81c280ff0180ffffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad6
    31c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b08
    3721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581c
    bd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e
    1879b7152a6e7298a91ce119a63400ade7c5ff04ffff04ff38ffff04ff2fffff
    01ff80808080ffff04ffff02ff36ffff04ff02ffff04ff05ffff04ff17ffff04
    ffff30ffff30ff0bff2fff8080ff21ffff010180ff808080808080ff5f8080ff
    ff04ff38ffff04ffff02ff3affff04ff02ffff04ff05ffff04ffff0bffff0101
    ff0b80ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ff
    0bff81a2ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff3cffff04ff02
    ffff04ff07ff80808080ff808080808080ffffff0bff2cffff0bff2cff81c2ff
    0580ffff0bff2cff0bff81828080ff04ff10ffff04ffff30ff17ffff02ff3aff
    ff04ff02ffff04ff31ffff04ffff02ff2effff04ff02ffff04ffff04ff31ffff
    04ff17ff218080ff80808080ffff04ffff02ff3affff04ff02ffff04ff29ffff
    04ffff0bffff0101ff2980ffff04ff8182ffff04ff39ffff04ffff02ff3affff
    04ff02ffff04ff25ffff04ffff0bffff0101ff2580ffff04ff8182ffff04ffff
    02ff3affff04ff02ffff04ff35ffff04ffff02ff2effff04ff02ffff04ffff04
    ff31ffff04ff17ff218080ff80808080ffff04ff2dffff04ffff0bffff0101ff
    3d80ff80808080808080ffff04ff0bff8080808080808080ff80808080808080
    80ff808080808080ffff010180ff808080ffff02ffff03ffff07ff0580ffff01
    ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2eff
    ff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff
    04ff34ffff04ffff0112ffff04ff80ffff04ffff02ff3affff04ff02ffff04ff
    05ffff04ffff0bffff0101ff0b80ff8080808080ff8080808080ff018080
    "
);

pub const CATALOG_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    ddc07891fc010a39139a5e868a420cfe32c1fe6d1b5a2587d4212626ff740d30
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
pub struct PuzzleAndSolution<P, S> {
    pub puzzle: P,
    #[clvm(rest)]
    pub solution: S,
}

impl<P, S> PuzzleAndSolution<P, S> {
    pub fn new(puzzle: P, solution: S) -> Self {
        Self { puzzle, solution }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogDoubleTailHashData {
    pub this_tail_hash: Bytes32, // left_tail_hash or right_tail_hash
    #[clvm(rest)]
    pub this_this_tail_hash: Bytes32, // left_left_tail_hash or right_right_tail_hash
}

impl CatalogDoubleTailHashData {
    pub fn new(this_tail_hash: Bytes32, this_this_tail_hash: Bytes32) -> Self {
        Self {
            this_tail_hash,
            this_this_tail_hash,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogRegisterActionSolution<P, S> {
    pub precommitted_cat_maker_data: PuzzleAndSolution<P, S>,
    pub other_precommit_data: CatalogOtherPrecommitData,
    pub left_data: CatalogDoubleTailHashData,
    pub right_data: CatalogDoubleTailHashData,
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
