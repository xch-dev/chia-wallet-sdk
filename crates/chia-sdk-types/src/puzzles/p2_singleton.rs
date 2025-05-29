use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2SingletonArgs {
    pub singleton_mod_hash: Bytes32,
    pub launcher_id: Bytes32,
    pub launcher_puzzle_hash: Bytes32,
}

impl Mod for P2SingletonArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_SINGLETON_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_SINGLETON_PUZZLE_HASH
    }
}

impl P2SingletonArgs {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            launcher_id,
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
        }
    }

    pub fn curry_tree_hash(launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: P2_SINGLETON_PUZZLE_HASH,
            args: Self::new(launcher_id),
        }
        .tree_hash()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2SingletonSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub my_id: Bytes32,
}

pub const P2_SINGLETON_PUZZLE: [u8; 403] = hex!(
    "
    ff02ffff01ff04ffff04ff18ffff04ffff0bffff02ff2effff04ff02ffff04ff
    05ffff04ff2fffff04ffff02ff3effff04ff02ffff04ffff04ff05ffff04ff0b
    ff178080ff80808080ff808080808080ff5f80ff808080ffff04ffff04ff2cff
    ff01ff248080ffff04ffff04ff10ffff04ff5fff808080ff80808080ffff04ff
    ff01ffffff463fff02ff3c04ffff01ff0102ffff02ffff03ff05ffff01ff02ff
    16ffff04ff02ffff04ff0dffff04ffff0bff3affff0bff12ff3c80ffff0bff3a
    ffff0bff3affff0bff12ff2a80ff0980ffff0bff3aff0bffff0bff12ff808080
    8080ff8080808080ffff010b80ff0180ffff0bff3affff0bff12ff1480ffff0b
    ff3affff0bff3affff0bff12ff2a80ff0580ffff0bff3affff02ff16ffff04ff
    02ffff04ff07ffff04ffff0bff12ff1280ff8080808080ffff0bff12ff808080
    8080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02
    ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ff
    ff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_SINGLETON_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "40f828d8dd55603f4ff9fbf6b73271e904e69406982f4fbefae2c8dcceaf9834"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_SINGLETON_PUZZLE => P2_SINGLETON_PUZZLE_HASH);
        Ok(())
    }
}
