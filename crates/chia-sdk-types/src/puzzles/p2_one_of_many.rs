use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{MerkleProof, Mod};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2OneOfManyArgs {
    pub merkle_root: Bytes32,
}

impl P2OneOfManyArgs {
    pub fn new(merkle_root: Bytes32) -> Self {
        Self { merkle_root }
    }
}

impl Mod for P2OneOfManyArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_ONE_OF_MANY_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_ONE_OF_MANY_PUZZLE_HASH
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2OneOfManySolution<P, S> {
    pub merkle_proof: MerkleProof,
    pub puzzle: P,
    pub solution: S,
}

impl<P, S> P2OneOfManySolution<P, S> {
    pub fn new(merkle_proof: MerkleProof, puzzle: P, solution: S) -> Self {
        Self {
            merkle_proof,
            puzzle,
            solution,
        }
    }
}

pub const P2_ONE_OF_MANY_PUZZLE: [u8; 280] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff05ffff02ff06ffff04ff02ffff04ffff0bff
    ff0101ffff02ff04ffff04ff02ffff04ff17ff8080808080ffff04ff0bff8080
    80808080ffff01ff02ff17ff2f80ffff01ff088080ff0180ffff04ffff01ffff
    02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff04ffff04ff02ffff04
    ff09ff80808080ffff02ff04ffff04ff02ffff04ff0dff8080808080ffff01ff
    0bffff0101ff058080ff0180ff02ffff03ff1bffff01ff02ff06ffff04ff02ff
    ff04ffff02ffff03ffff18ffff0101ff1380ffff01ff0bffff0102ff2bff0580
    ffff01ff0bffff0102ff05ff2b8080ff0180ffff04ffff04ffff17ff13ffff01
    81ff80ff3b80ff8080808080ffff010580ff0180ff018080
    "
);

pub const P2_ONE_OF_MANY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "46b29fd87fbeb6737600c4543931222a6c1ed3db6fa5601a3ca284a9f4efe780"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_ONE_OF_MANY_PUZZLE => P2_ONE_OF_MANY_PUZZLE_HASH);
        Ok(())
    }
}
