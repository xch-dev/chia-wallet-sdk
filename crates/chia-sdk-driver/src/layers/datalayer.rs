mod delegation_layer;
mod oracle_layer;
mod writer_layer;

use clvm_utils::TreeHash;
use hex_literal::hex;

pub use delegation_layer::*;
pub use oracle_layer::*;
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

#[cfg(test)]
mod tests {
    use clvm_traits::{clvm_list, ToClvm};
    use clvm_utils::tree_hash;
    use clvmr::serde::node_from_bytes;
    use rstest::rstest;

    use crate::SpendContext;

    use super::*;

    // not exported, so I had to copy-paste
    // use chia_puzzles::assert_puzzle_hash;
    #[macro_export]
    macro_rules! assert_puzzle_hash {
        ($puzzle:ident => $puzzle_hash:ident) => {
            let mut a = clvmr::Allocator::new();
            let ptr = clvmr::serde::node_from_bytes(&mut a, &$puzzle)?;
            let hash = clvm_utils::tree_hash(&mut a, ptr);
            assert_eq!($puzzle_hash, hash);
        };
    }

    #[test]
    fn test_puzzle_hashes() -> anyhow::Result<()> {
        assert_puzzle_hash!(DELEGATION_LAYER_PUZZLE => DELEGATION_LAYER_PUZZLE_HASH);
        assert_puzzle_hash!(WRITER_FILTER_PUZZLE => WRITER_FILTER_PUZZLE_HASH);
        assert_puzzle_hash!(DL_METADATA_UPDATER_PUZZLE => DL_METADATA_UPDATER_PUZZLE_HASH);

        Ok(())
    }

    // tests that DL metadata updater indeed returns the third argument
    #[rstest]
    #[case::string(&hex!("8379616b"))] // run -d '"yak"'
    #[case::atom(&hex!("ff018379616b"))] // run -d '(mod () "yak"))'
    #[case::one_item_list(&hex!("ff01ff0180"))] // run -d '(mod () (list 1)))'
    #[case::multiple_item_list(&hex!("ff01ff01ff02ff0380"))] // run -d '(mod () (list 1 2 3)))'
    #[case::lists_within_list(&hex!("ff01ff01ffff02ff0380ffff04ff0580ffff060780"))] // run -d '(mod () (list 1 (list 2 3) (list 4 5) (c 6 7))))'
    fn test_dl_metadata_updater_puzzle(#[case] third_arg: &'static [u8]) -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let third_arg_ptr = node_from_bytes(&mut ctx.allocator, third_arg)?;
        let solution_ptr = clvm_list![(), (), third_arg_ptr].to_clvm(&mut ctx.allocator)?;

        let puzzle_ptr = node_from_bytes(&mut ctx.allocator, &DL_METADATA_UPDATER_PUZZLE)?;
        let output = ctx.run(puzzle_ptr, solution_ptr)?;

        assert_eq!(
            tree_hash(&ctx.allocator, output),
            tree_hash(&ctx.allocator, third_arg_ptr),
        );

        Ok(())
    }
}
