mod delegation_layer;
mod writer_layer;

use clvm_utils::TreeHash;
use hex_literal::hex;

pub use delegation_layer::*;
pub use writer_layer::*;

// bytes(ACS_MU).hex()
pub const DL_METADATA_UPDATER_PUZZLE: [u8; 1] = hex!(
    "
    0b
    "
);

pub const DL_METADATA_UPDATER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    57bfd1cb0adda3d94315053fda723f2028320faa8338225d99f629e3d46d43a9
    "
));

mod tests {
    use crate::{
        DELEGATION_LAYER_PUZZLE, DELEGATION_LAYER_PUZZLE_HASH, DL_METADATA_UPDATER_PUZZLE,
        DL_METADATA_UPDATER_PUZZLE_HASH, WRITER_FILTER_PUZZLE, WRITER_FILTER_PUZZLE_HASH,
    };

    // not exported, so I had to copy-paste
    // use chia_puzzles::assert_puzzle_hash;
    #[macro_export]
    macro_rules! assert_puzzle_hash {
        ($puzzle:ident => $puzzle_hash:ident) => {
            let mut a = clvmr::Allocator::new();
            let ptr = clvmr::serde::node_from_bytes(&mut a, &$puzzle).unwrap();
            let hash = clvm_utils::tree_hash(&mut a, ptr);
            assert_eq!($puzzle_hash, hash);
        };
    }

    #[test]
    fn test_puzzle_hashes() {
        assert_puzzle_hash!(DELEGATION_LAYER_PUZZLE => DELEGATION_LAYER_PUZZLE_HASH);
        assert_puzzle_hash!(WRITER_FILTER_PUZZLE => WRITER_FILTER_PUZZLE_HASH);
        assert_puzzle_hash!(DL_METADATA_UPDATER_PUZZLE => DL_METADATA_UPDATER_PUZZLE_HASH);
    }
}
