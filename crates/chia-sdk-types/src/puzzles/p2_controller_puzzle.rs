use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Curried arguments for the CHIP-0037 controller puzzle.
///
/// A coin locked by this puzzle can only be spent when a coin whose puzzle
/// hash equals `controller_puzzle_hash` sends a `RECEIVE_MESSAGE` whose body
/// is the tree hash of the chosen `delegated_puzzle`. This collapses many
/// coin-spending signatures into one (e.g. one EIP-712 prompt unlocks many
/// downstream coins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2ControllerPuzzleArgs {
    pub controller_puzzle_hash: Bytes32,
}

impl P2ControllerPuzzleArgs {
    pub fn new(controller_puzzle_hash: Bytes32) -> Self {
        Self {
            controller_puzzle_hash,
        }
    }
}

impl Mod for P2ControllerPuzzleArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_CONTROLLER_PUZZLE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_CONTROLLER_PUZZLE_PUZZLE_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2ControllerPuzzleSolution<P, S> {
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl<P, S> P2ControllerPuzzleSolution<P, S> {
    pub fn new(delegated_puzzle: P, delegated_solution: S) -> Self {
        Self {
            delegated_puzzle,
            delegated_solution,
        }
    }
}

/// CLVM bytecode for the CHIP-0037 `p2_controller_puzzle` puzzle.
///
/// Source: <https://github.com/Chia-Network/chips/blob/main/assets/chip-0037/clsp/p2_controller_puzzle.clsp>
pub const P2_CONTROLLER_PUZZLE_PUZZLE: [u8; 151] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ffff0117ffff04ffff02ff06ffff04ff02
    ffff04ff0bff80808080ffff04ff05ff8080808080ffff02ff0bff178080ffff
    04ffff01ff43ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ff
    ff04ff02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff04ff0dff8080
    808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_CONTROLLER_PUZZLE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "d5415713619e318bfa7820e06e2b163beef32d82294a5a7fcf9c3c69b0949c88"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_CONTROLLER_PUZZLE_PUZZLE => P2_CONTROLLER_PUZZLE_PUZZLE_HASH);

        Ok(())
    }
}
