use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH,
    NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{
    ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError, clvm_tuple,
};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{ANY_METADATA_UPDATER_HASH, CatalogOtherPrecommitData},
};

/// Constants used by the `CATalog` register action to compute the puzzle hash of the
/// registration NFT it mints.
///
/// Encoded as `(((launcher_hash . singleton_mod_hash) . (state_layer_mod_hash . metadata_updater_hash_hash)) . ((nft_ownership_layer_mod_hash . transfer_program_mod_hash) . (royalty_puzzle_hash_hash . trade_price_percentage)))`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftPack {
    /// Puzzle hash of the singleton launcher.
    pub launcher_hash: Bytes32,
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Mod hash of the NFT state layer.
    pub state_layer_mod_hash: Bytes32,
    /// Tree hash of the NFT metadata updater's puzzle hash.
    pub metadata_updater_hash_hash: Bytes32,
    /// Mod hash of the NFT ownership layer.
    pub nft_ownership_layer_mod_hash: Bytes32,
    /// Mod hash of the NFT royalty transfer program.
    pub transfer_program_mod_hash: Bytes32,
    /// Tree hash of the puzzle hash that receives NFT royalties.
    pub royalty_puzzle_hash_hash: Bytes32,
    /// Royalty, in basis points.
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
    /// Creates the pack with the standard NFT puzzles and [`ANY_METADATA_UPDATER_HASH`] as
    /// the metadata updater.
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

/// Serialized `CATalog` register action.
///
/// `CATalog` is a registry of CATs where the first party to reveal a CAT's TAIL (through
/// a precommit coin) owns its entry. Registering inserts a slot for the new asset id
/// between two neighboring slots (left and right asset ids must bracket it), consumes a
/// precommit coin whose amount equals the registration price, and mints a registration
/// NFT through a [uniqueness prelauncher](crate::puzzles::UNIQUENESS_PRELAUNCHER_PUZZLE)
/// whose value is the asset id. The NFT starts with nil metadata and is owned by the
/// precommitted initial owner puzzle hash. The action announces `"r"` followed by the tree
/// hash of `(tail_hash . initial_nft_owner_ph)`, and leaves the state unchanged.
pub const CATALOG_REGISTER_PUZZLE: [u8; 1456] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff0aff820bffff82057f80ffff0aff
    820affff820bff8080ffff09ffff02ff04ffff04ff04ff8209ff8080ff82015f
    8080ffff01ff02ffff01ff04ff81bfffff04ffff04ffff0133ffff04ff02ffff
    01ff80808080ffff04ffff02ff7dffff04ffff04ff09ffff04ff15ff5d80
    80ffff04ff0bffff04ffff30ffff30ff82017fff02ff8080ff43ffff010180ff
    822fff80808080ffff04ffff04ffff0146ffff04ff82017fff808080ffff04ff
    ff04ffff013effff04ffff0effff0172ffff02ff09ffff04ff09ffff04ff8217
    ffff822fff80808080ff808080ffff04ffff04ffff0142ffff04ffff0112ffff
    04ff80ffff04ffff02ff15ffff04ff5dffff04ff5fffff04ffff0bffff0101ff
    ff02ff09ffff04ff09ffff04ff8204ffffff04ff820affffff04ff820effff82
    15ff808080808080ff8080808080ff8080808080ffff04ffff04ffff0142ffff
    04ffff0112ffff04ff80ffff04ffff02ff15ffff04ff5dffff04ff5fffff04ff
    ff0bffff0101ffff02ff09ffff04ff09ffff04ff8209ffffff04ff8215ffffff
    04ff820affff821dff808080808080ff8080808080ff8080808080ffff04ffff
    02ff2dffff04ffff04ff15ff5d80ffff04ff5fffff02ff09ffff04ff09ffff04
    ff80ffff04ff8217ffffff04ff820affff8215ff8080808080808080ffff04ff
    ff02ff2dffff04ffff04ff15ff5d80ffff04ff5fffff02ff09ffff04ff09ffff
    04ffff10ff8204ffffff010180ffff04ff820affffff04ff820effff8217ff80
    80808080808080ffff04ffff02ff2dffff04ffff04ff15ff5d80ffff04ff5fff
    ff02ff09ffff04ff09ffff04ffff10ff8209ffffff010180ffff04ff8215ffff
    ff04ff8217ffff821dff8080808080808080ffff04ffff04ffff0142ffff04ff
    ff0113ffff04ffff0101ffff04ffff02ff8213ffffff04ffff02ff15ffff04ff
    5dffff04ff2fffff04ff823fffffff04ffff0bffff0102ff8217ffffff0bffff
    0101ffff02ff09ffff04ff09ffff04ff822fffffff04ff8202bfff821bff8080
    80808080ff808080808080ff821bff8080ffff04ff8203bfff808080808080ff
    808080808080808080808080ffff04ffff02ff0affff04ff2effff04ff0bffff
    04ffff0bffff0101ff820bff80ff8080808080ff018080ffff01ff088080ff01
    80ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff
    02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0b
    ffff0101ff038080ff0180ffff04ffff01ff0bffff0102ffff0bffff01820102
    80ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bff
    ff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04ff
    ff01ff04ffff0133ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0b
    ffff0101ff0780ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080
    808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182
    010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff
    0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff
    01ff0bffff018201018080ff0180ffff01ff02ffff01ff04ffff0140ffff04ff
    ff30ff17ffff02ff15ffff04ff1dffff04ff63ffff04ff02ffff04ffff02ff15
    ffff04ff1dffff04ff53ffff04ffff0bffff0101ff5380ffff04ffff0bffff01
    0180ffff04ff73ffff04ffff02ff15ffff04ff1dffff04ff4bffff04ffff0bff
    ff0101ff4b80ffff04ffff0bffff010180ffff04ffff02ff15ffff04ff1dffff
    04ff6bffff04ff02ffff04ff5bffff04ffff0bffff0101ff7b80ff8080808080
    8080ffff04ff1fff8080808080808080ff8080808080808080ff808080808080
    ffff010180ff808080ffff04ffff02ff04ffff04ff04ffff04ff31ffff04ff0b
    ff2180808080ff01808080808080ff018080
    "
);

/// Tree hash of [`CATALOG_REGISTER_PUZZLE`].
pub const CATALOG_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    ef66622bbd9ac5922df81baac96082e16f51ae0a8a69e40e93b13a44fff1f8cf
    "
));

/// Curried arguments of the `CATalog` register action ([`CATALOG_REGISTER_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct CatalogRegisterActionArgs {
    /// Constants used to compute the registration NFT's puzzle hash.
    pub nft_pack: NftPack,
    /// Hash of the uniqueness prelauncher after its first curry
    /// ([`UniquenessPrelauncher1stCurryArgs`](crate::puzzles::UniquenessPrelauncher1stCurryArgs)).
    pub uniqueness_prelauncher_1st_curry_hash: Bytes32,
    /// Hash of the precommit layer after its first curry
    /// ([`PrecommitLayer1stCurryArgs`](crate::puzzles::PrecommitLayer1stCurryArgs)).
    pub precommit_1st_curry_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for the registry's slots
    /// ([`Slot1stCurryArgs`](crate::puzzles::Slot1stCurryArgs)).
    pub slot_1st_curry_hash: Bytes32,
}

/// A puzzle reveal and its solution, encoded as `(puzzle . solution)`.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct PuzzleAndSolution<P, S> {
    /// Puzzle reveal.
    pub puzzle: P,
    /// Solution to the puzzle.
    #[clvm(rest)]
    pub solution: S,
}

impl<P, S> PuzzleAndSolution<P, S> {
    /// Creates the value from its parts.
    pub fn new(puzzle: P, solution: S) -> Self {
        Self { puzzle, solution }
    }
}

/// Information about one of the two slots a new `CATalog` entry is inserted between,
/// encoded as `(this_counter this_tail_hash . this_this_tail_hash)`.
#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogDoubleTailHashData {
    /// Counter of the neighbor slot.
    pub this_counter: u64, // this slot's counters
    /// Asset id of the neighbor slot (the left or right asset id).
    pub this_tail_hash: Bytes32, // left_tail_hash or right_tail_hash
    /// Asset id of the neighbor's own outer neighbor (the left neighbor's left neighbor, or
    /// the right neighbor's right neighbor).
    #[clvm(rest)]
    pub this_this_tail_hash: Bytes32, // left_left_tail_hash or right_right_tail_hash
}

impl CatalogDoubleTailHashData {
    /// Creates the value from its parts.
    pub fn new(this_counter: u64, this_tail_hash: Bytes32, this_this_tail_hash: Bytes32) -> Self {
        Self {
            this_counter,
            this_tail_hash,
            this_this_tail_hash,
        }
    }
}

/// Solution to the `CATalog` register action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogRegisterActionSolution<P, S> {
    /// Coin id of the `CATalog` singleton coin being spent, which is the parent of the
    /// uniqueness prelauncher.
    pub my_id: Bytes32,
    /// Slot to the left of the new entry.
    pub left_data: CatalogDoubleTailHashData,
    /// Slot to the right of the new entry.
    pub right_data: CatalogDoubleTailHashData,
    /// CAT maker puzzle and solution of the precommit coin. The puzzle must match the CAT
    /// maker puzzle hash in the registry's state.
    pub precommitted_cat_maker_data: PuzzleAndSolution<P, S>,
    /// Rest of the data the precommit coin committed to.
    #[clvm(rest)]
    pub other_precommit_data: CatalogOtherPrecommitData,
}

impl Mod for CatalogRegisterActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&CATALOG_REGISTER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        CATALOG_REGISTER_PUZZLE_HASH
    }
}
