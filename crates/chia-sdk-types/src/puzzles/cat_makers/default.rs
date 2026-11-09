use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::CAT_PUZZLE_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized default CAT maker puzzle, compiled from Rue.
///
/// A CAT maker takes an inner puzzle hash (prepended by the caller to a user-supplied
/// solution) and returns the full puzzle hash of a CAT with that inner puzzle. Action layer
/// registries store the hash of a CAT maker in their state so payments can be made in a specific
/// asset, and partial offers (CHIP-0052) curry one in to compute the puzzle hash of the offered
/// asset. This one wraps the inner puzzle directly in the CAT layer, with no layers in between.
pub const DEFAULT_CAT_MAKER_PUZZLE: [u8; 217] = hex!(
    // Rue
    "
    ff02ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bff
    ff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff
    02ffff04ffff0bffff0101ff0580ffff04ff0bffff04ff17ff808080808080ff
    ff0bffff010180808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102
    ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff018201
    0180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01
    0180808080ffff01ff0bffff018201018080ff0180ff018080
    "
);

/// Tree hash of [`DEFAULT_CAT_MAKER_PUZZLE`].
pub const DEFAULT_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d6693f7aacfeb7e6901cd71a00f5e4b318d73234fbffd5b4d84da2c1371b3181
    "
));

/// Curried arguments of the default CAT maker ([`DEFAULT_CAT_MAKER_PUZZLE`]).
///
/// The CAT maker takes no solution beyond the inner puzzle hash.
///
/// # Example
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_puzzle_types::cat::CatArgs;
/// use chia_sdk_types::{
///     run_puzzle,
///     puzzles::{DEFAULT_CAT_MAKER_PUZZLE, DefaultCatMakerArgs},
/// };
/// use clvm_traits::{FromClvm, ToClvm};
/// use clvm_utils::{CurriedProgram, ToTreeHash};
/// use clvmr::{Allocator, serde::node_from_bytes};
///
/// let asset_id = Bytes32::new([1; 32]);
/// let inner_puzzle_hash = Bytes32::new([2; 32]);
///
/// // The CAT maker is curried with the tree hash of the asset id.
/// let args = DefaultCatMakerArgs::new(asset_id.tree_hash().into());
///
/// let mut allocator = Allocator::new();
/// let cat_maker_mod = node_from_bytes(&mut allocator, &DEFAULT_CAT_MAKER_PUZZLE)?;
/// let cat_maker = CurriedProgram { program: cat_maker_mod, args }.to_clvm(&mut allocator)?;
///
/// // Running it with an inner puzzle hash returns the CAT's full puzzle hash.
/// let solution = [inner_puzzle_hash].to_clvm(&mut allocator)?;
/// let output = run_puzzle(&mut allocator, cat_maker, solution)?;
/// let cat_puzzle_hash = Bytes32::from_clvm(&allocator, output)?;
///
/// assert_eq!(
///     cat_puzzle_hash,
///     Bytes32::from(CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash.into())),
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultCatMakerArgs {
    /// Mod hash of the CAT outer puzzle.
    pub cat_mod_hash: Bytes32,
    /// Tree hash of the asset id (TAIL hash) as an atom, `sha256(1 ++ asset_id)`.
    pub tail_hash_hash: Bytes32,
}

impl DefaultCatMakerArgs {
    /// Creates the arguments for the CAT v2 mod hash. `tail_hash_hash` is the tree hash of
    /// the asset id.
    pub fn new(tail_hash_hash: Bytes32) -> Self {
        Self {
            cat_mod_hash: CAT_PUZZLE_HASH.into(),
            tail_hash_hash,
        }
    }
}

impl Mod for DefaultCatMakerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DEFAULT_CAT_MAKER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DEFAULT_CAT_MAKER_PUZZLE_HASH
    }
}
