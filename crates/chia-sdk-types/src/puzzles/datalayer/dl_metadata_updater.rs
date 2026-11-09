use clvm_utils::TreeHash;
use hex_literal::hex;

/// The serialized CLVM bytecode of the `DataLayer` metadata updater, which is just `11`.
///
/// The NFT state layer runs metadata updaters with
/// `(current_metadata current_updater_puzzle_hash updater_solution)`, so this program returns
/// the updater solution as is. This means the spender chooses the new metadata, metadata updater
/// puzzle hash, and conditions directly.
pub const DL_METADATA_UPDATER_PUZZLE: [u8; 1] = hex!(
    "
    0b
    "
);

/// The tree hash of [`DL_METADATA_UPDATER_PUZZLE`].
pub const DL_METADATA_UPDATER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    57bfd1cb0adda3d94315053fda723f2028320faa8338225d99f629e3d46d43a9
    "
));

#[cfg(test)]
mod tests {
    use crate::{
        assert_puzzle_hash,
        puzzles::{
            DELEGATION_LAYER_PUZZLE, DELEGATION_LAYER_PUZZLE_HASH, WRITER_LAYER_PUZZLE,
            WRITER_LAYER_PUZZLE_HASH,
        },
    };

    use super::*;

    #[test]
    fn test_puzzle_hashes() -> anyhow::Result<()> {
        assert_puzzle_hash!(DELEGATION_LAYER_PUZZLE => DELEGATION_LAYER_PUZZLE_HASH);
        assert_puzzle_hash!(WRITER_LAYER_PUZZLE => WRITER_LAYER_PUZZLE_HASH);
        assert_puzzle_hash!(DL_METADATA_UPDATER_PUZZLE => DL_METADATA_UPDATER_PUZZLE_HASH);
        Ok(())
    }
}
