mod delegation_layer;
mod oracle_layer;
mod writer_layer;

pub use delegation_layer::*;
pub use oracle_layer::*;
pub use writer_layer::*;

#[cfg(test)]
mod tests {
    use chia_sdk_types::puzzles::DL_METADATA_UPDATER_PUZZLE;
    use clvm_traits::{clvm_list, ToClvm};
    use clvm_utils::tree_hash;
    use clvmr::serde::node_from_bytes;
    use hex_literal::hex;
    use rstest::rstest;

    use crate::SpendContext;

    // tests that DL metadata updater indeed returns the third argument
    #[rstest]
    #[case::string(&hex!("8379616b"))] // run -d '"yak"'
    #[case::atom(&hex!("ff018379616b"))] // run -d '(mod () "yak"))'
    #[case::one_item_list(&hex!("ff01ff0180"))] // run -d '(mod () (list 1)))'
    #[case::multiple_item_list(&hex!("ff01ff01ff02ff0380"))] // run -d '(mod () (list 1 2 3)))'
    #[case::lists_within_list(&hex!("ff01ff01ffff02ff0380ffff04ff0580ffff060780"))] // run -d '(mod () (list 1 (list 2 3) (list 4 5) (c 6 7))))'
    fn test_dl_metadata_updater_puzzle(#[case] third_arg: &'static [u8]) -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let third_arg_ptr = node_from_bytes(&mut ctx, third_arg)?;
        let solution_ptr = clvm_list![(), (), third_arg_ptr].to_clvm(&mut ctx)?;

        let puzzle_ptr = node_from_bytes(&mut ctx, &DL_METADATA_UPDATER_PUZZLE)?;
        let output = ctx.run(puzzle_ptr, solution_ptr)?;

        assert_eq!(tree_hash(&ctx, output), tree_hash(&ctx, third_arg_ptr),);

        Ok(())
    }
}
