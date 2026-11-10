use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{CAT_PUZZLE_HASH, REVOCATION_LAYER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized revocable CAT maker puzzle, compiled from Rue.
///
/// Like [`DEFAULT_CAT_MAKER_PUZZLE`](crate::puzzles::DEFAULT_CAT_MAKER_PUZZLE), but for
/// revocable CATs: it wraps the inner puzzle in the revocation layer, then in the CAT
/// layer.
pub const REVOCABLE_CAT_MAKER_PUZZLE: [u8; 295] = hex!(
    // Rue
    "
    ff02ffff01ff02ff04ffff04ff06ffff04ff09ffff04ffff0bffff0101ff0980
    ffff04ff0bffff04ffff02ff04ffff04ff06ffff04ff15ffff04ffff0bffff01
    01ff1580ffff04ff1dffff04ffff0bffff0101ff1780ff80808080808080ff80
    808080808080ffff04ffff04ffff01ff0bffff0102ffff0bffff0182010280ff
    ff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff01
    02ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff02ff
    ff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff
    0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff
    04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201018080ff
    018080ff018080
    "
);

/// Tree hash of [`REVOCABLE_CAT_MAKER_PUZZLE`].
pub const REVOCABLE_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    f1b280e985d067fa79a86eca600d8fe9ca62bf2eaa5346a6ddaf23c7773ee955
    "
));

/// Constants curried into the revocable CAT maker as a single argument.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RevocableCatMakerStruct {
    /// Mod hash of the CAT outer puzzle.
    pub cat_mod_hash: Bytes32,
    /// Mod hash of the revocation layer.
    pub revocation_layer_mod_hash: Bytes32,
    /// Tree hash of the revocation layer's hidden puzzle hash as an atom.
    #[clvm(rest)]
    pub hidden_puzzle_hash_hash: Bytes32,
}

impl RevocableCatMakerStruct {
    /// Creates the struct with the standard CAT v2 and revocation layer mod hashes.
    /// `hidden_puzzle_hash_hash` is the tree hash of the hidden puzzle hash.
    pub fn new(hidden_puzzle_hash_hash: TreeHash) -> Self {
        Self {
            cat_mod_hash: CAT_PUZZLE_HASH.into(),
            revocation_layer_mod_hash: REVOCATION_LAYER_HASH.into(),
            hidden_puzzle_hash_hash: hidden_puzzle_hash_hash.into(),
        }
    }
}

/// Curried arguments of the revocable CAT maker ([`REVOCABLE_CAT_MAKER_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RevocableCatMakerArgs {
    /// Mod hashes and hidden puzzle hash shared by all revocable CATs of this asset.
    pub mod_struct: RevocableCatMakerStruct,
    /// Tree hash of the asset id (TAIL hash) as an atom.
    pub tail_hash_hash: Bytes32,
}

impl RevocableCatMakerArgs {
    /// Creates the arguments from the tree hashes of the asset id and of the hidden puzzle
    /// hash.
    pub fn new(tail_hash_hash: TreeHash, hidden_puzzle_hash_hash: TreeHash) -> Self {
        Self {
            mod_struct: RevocableCatMakerStruct::new(hidden_puzzle_hash_hash),
            tail_hash_hash: tail_hash_hash.into(),
        }
    }
}

impl Mod for RevocableCatMakerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REVOCABLE_CAT_MAKER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REVOCABLE_CAT_MAKER_PUZZLE_HASH
    }
}
