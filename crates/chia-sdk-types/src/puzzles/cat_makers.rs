mod default;
mod revocable;
mod xch;

pub use default::*;
pub use revocable::*;
pub use xch::*;

#[cfg(test)]
mod tests {
    use crate::assert_puzzle_hash;

    use super::*;

    #[test]
    fn test_puzzle_hashes() -> anyhow::Result<()> {
        assert_puzzle_hash!(DEFAULT_CAT_MAKER_PUZZLE => DEFAULT_CAT_MAKER_PUZZLE_HASH);
        assert_puzzle_hash!(REVOCABLE_CAT_MAKER_PUZZLE => REVOCABLE_CAT_MAKER_PUZZLE_HASH);
        assert_puzzle_hash!(XCH_CAT_MAKER_PUZZLE => XCH_CAT_MAKER_PUZZLE_HASH);
        Ok(())
    }
}
