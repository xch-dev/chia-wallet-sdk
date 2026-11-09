use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

/// The serialized CLVM bytecode of the `DataLayer` delegation layer (CHIP-0035). See
/// [`DelegationLayerArgs`].
pub const DELEGATION_LAYER_PUZZLE: [u8; 1027] = hex!(
    "
    ff02ffff01ff02ff12ffff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff2fffff04ff5fff
    ff04ffff02ff81bfff82017f80ffff04ffff02ff16ffff04ff02ffff04ff81bfff80808080ff8080
    8080808080808080ffff04ffff01ffffff3381f3ff02ffffa04bf5122f344554c53bde2ebb8cd2b7
    e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e8
    78a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f680
    6923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff
    ffff02ffff03ffff09ff82017fff1780ffff0181bfffff01ff02ffff03ffff09ff2fffff02ff1eff
    ff04ff02ffff04ffff0bffff0101ff82017f80ffff04ff5fff808080808080ffff01ff02ff1affff
    04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff81bfffff04ffff04ff2fffff04ff0bff8080
    80ff8080808080808080ffff01ff088080ff018080ff0180ff02ffff03ff2fffff01ff02ffff03ff
    ff09ff818fff1880ffff01ff02ff1affff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff6f
    ffff04ff81cfff8080808080808080ffff01ff04ffff02ffff03ffff02ffff03ffff09ff818fffff
    0181e880ffff01ff22ffff09ff820acfff8080ffff09ff8214cfffff01a057bfd1cb0adda3d94315
    053fda723f2028320faa8338225d99f629e3d46d43a98080ffff01ff010180ff0180ffff014fffff
    01ff088080ff0180ffff02ff1affff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff6fffff
    04ff5fff80808080808080808080ff0180ffff01ff04ffff04ff10ffff04ffff0bff5cffff0bff14
    ffff0bff14ff6cff0580ffff0bff14ffff0bff7cffff0bff14ffff0bff14ff6cffff0bffff0101ff
    058080ffff0bff14ffff0bff7cffff0bff14ffff0bff14ff6cffff0bffff0101ff0b8080ffff0bff
    14ffff0bff7cffff0bff14ffff0bff14ff6cffff0bffff0101ff178080ffff0bff14ffff0bff7cff
    ff0bff14ffff0bff14ff6cffff0bffff0101ff819f8080ffff0bff14ff6cff4c808080ff4c808080
    ff4c808080ff4c808080ff4c808080ffff04ffff0101ffff04ff81dfff8080808080ff808080ff01
    80ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff8080
    8080ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff
    02ffff03ff1bffff01ff02ff1effff04ff02ffff04ffff02ffff03ffff18ffff0101ff1380ffff01
    ff0bffff0102ff2bff0580ffff01ff0bffff0102ff05ff2b8080ff0180ffff04ffff04ffff17ff13
    ffff0181ff80ff3b80ff8080808080ffff010580ff0180ff018080
    "
);

/// The tree hash of [`DELEGATION_LAYER_PUZZLE`].
pub const DELEGATION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    c3b249466cb15c51e5abb5c54ef5077c1624ae2e6a0f8f7a3fa197a943a5d62e
    "
));

/// Curried arguments for the `DataLayer` delegation layer (CHIP-0035), which lets a store's owner
/// delegate limited control to other puzzles.
///
/// It's the inner puzzle of a `DataLayer` store's NFT state layer. The solution reveals a puzzle
/// and its solution:
///
/// - If the puzzle's hash is `owner_puzzle_hash`, its conditions are returned unchanged, so the
///   owner has full control.
/// - Otherwise, the solution must include a [`MerkleProof`] that the puzzle hash is a leaf of the
///   merkle tree with `merkle_root`. Such a delegated puzzle can update the metadata (as long as
///   the metadata updater stays
///   [`DL_METADATA_UPDATER_PUZZLE_HASH`](crate::puzzles::DL_METADATA_UPDATER_PUZZLE_HASH) and it
///   outputs no extra conditions) and can change the merkle root with an
///   [`UpdateDatastoreMerkleRoot`](crate::conditions::UpdateDatastoreMerkleRoot) condition. The
///   layer then recreates itself with amount 1, keeping the owner and using the new merkle root if
///   one was given, hinted with the memos from that condition (or the launcher id by default).
///
/// Delegated puzzles are usually wrapped in [`WriterLayerArgs`](crate::puzzles::WriterLayerArgs)
/// so that they can't change the merkle root.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::Bytes32;
/// # use chia_sdk_types::{Mod, puzzles::DelegationLayerArgs};
/// let launcher_id = Bytes32::new([1; 32]);
/// let owner_puzzle_hash = Bytes32::new([2; 32]);
/// let merkle_root = Bytes32::new([3; 32]);
///
/// let args = DelegationLayerArgs::new(launcher_id, owner_puzzle_hash, merkle_root);
/// assert_eq!(
///     Mod::curry_tree_hash(&args),
///     DelegationLayerArgs::curry_tree_hash(launcher_id, owner_puzzle_hash, merkle_root)
/// );
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DelegationLayerArgs {
    /// The mod hash of the delegation layer, which is [`DELEGATION_LAYER_PUZZLE_HASH`].
    pub mod_hash: Bytes32,
    /// The launcher id of the store, used as the default hint when the layer recreates itself.
    pub launcher_id: Bytes32,
    /// The puzzle hash of the owner, which has full control over the store.
    pub owner_puzzle_hash: Bytes32,
    /// The root of the merkle tree of delegated puzzle hashes.
    pub merkle_root: Bytes32,
}

impl DelegationLayerArgs {
    /// Creates new [`DelegationLayerArgs`], using [`DELEGATION_LAYER_PUZZLE_HASH`] as the mod hash.
    pub fn new(launcher_id: Bytes32, owner_puzzle_hash: Bytes32, merkle_root: Bytes32) -> Self {
        Self {
            mod_hash: DELEGATION_LAYER_PUZZLE_HASH.into(),
            launcher_id,
            owner_puzzle_hash,
            merkle_root,
        }
    }
}

impl DelegationLayerArgs {
    /// Calculates the puzzle hash of the delegation layer with the given arguments.
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        owner_puzzle_hash: Bytes32,
        merkle_root: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: DELEGATION_LAYER_PUZZLE_HASH,
            args: DelegationLayerArgs {
                mod_hash: DELEGATION_LAYER_PUZZLE_HASH.into(),
                launcher_id,
                owner_puzzle_hash,
                merkle_root,
            },
        }
        .tree_hash()
    }
}

/// The solution for the `DataLayer` delegation layer. See [`DelegationLayerArgs`].
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct DelegationLayerSolution<P, S> {
    /// The proof that the puzzle hash is in the merkle tree, or [`None`] if the owner is spending.
    pub merkle_proof: Option<MerkleProof>,
    /// The owner's puzzle or a delegated puzzle.
    pub puzzle_reveal: P,
    /// The solution to run the puzzle with.
    pub puzzle_solution: S,
}

impl Mod for DelegationLayerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DELEGATION_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DELEGATION_LAYER_PUZZLE_HASH
    }
}
