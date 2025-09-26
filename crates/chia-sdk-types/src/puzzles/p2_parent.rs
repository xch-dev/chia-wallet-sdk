use std::borrow::Cow;

use chia_bls::PublicKey;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const P2_PARENT_PUZZLE: [u8; 242] = hex!(
    "
    ff02ffff01ff04ffff04ff08ffff04ffff02ff0affff04ff02ffff04ff0bffff
    04ffff02ff05ffff02ff0effff04ff02ffff04ff17ff8080808080ffff04ff2f
    ff808080808080ff808080ffff02ff17ff5f8080ffff04ffff01ffff4720ffff
    02ffff03ffff22ffff09ffff0dff0580ff0c80ffff09ffff0dff0b80ff0c80ff
    ff15ff17ffff0181ff8080ffff01ff0bff05ff0bff1780ffff01ff088080ff01
    80ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff0effff04ff02ff
    ff04ff09ff80808080ffff02ff0effff04ff02ffff04ff0dff8080808080ffff
    01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_PARENT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b10ce2d0b18dcf8c21ddfaf55d9b9f0adcbf1e0beb55b1a8b9cad9bbff4e5f22
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2ParentArgs<M> {
    pub morpher: M,
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct P2ParentSolution<PIP, PS> {
    pub parent_parent_id: Bytes32,
    pub parent_inner_puzzle: PIP,
    pub parent_amount: u64,
    pub parent_solution: PS,
}

impl<M> Mod for P2ParentArgs<M> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_PARENT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_PARENT_PUZZLE_HASH
    }
}
