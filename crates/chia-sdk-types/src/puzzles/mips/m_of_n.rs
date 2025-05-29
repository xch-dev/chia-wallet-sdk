use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct MofNArgs {
    pub required: usize,
    pub merkle_root: Bytes32,
}

impl MofNArgs {
    pub fn new(required: usize, merkle_root: Bytes32) -> Self {
        Self {
            required,
            merkle_root,
        }
    }
}

impl Mod for MofNArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&M_OF_N_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        M_OF_N_PUZZLE_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNSolution<P> {
    pub proofs: P,
}

impl<P> MofNSolution<P> {
    pub fn new(proofs: P) -> Self {
        Self { proofs }
    }
}

pub const M_OF_N_PUZZLE: [u8; 622] = hex!(
    "
    ff02ffff01ff02ff16ffff04ff02ffff04ff05ffff04ff0bffff04ffff02ff0c
    ffff04ff02ffff04ff2fffff04ff17ff8080808080ff808080808080ffff04ff
    ff01ffffff02ffff03ffff07ff0580ffff01ff02ff0cffff04ff02ffff04ff05
    ffff04ff0bff8080808080ffff01ff04ff05ffff01ff80ff80808080ff0180ff
    02ffff03ff09ffff01ff04ffff0bffff0102ffff05ffff02ff08ffff04ff02ff
    ff04ff09ffff04ff0bff808080808080ffff05ffff02ff08ffff04ff02ffff04
    ff0dffff04ff0bff80808080808080ffff04ffff02ff0affff04ff02ffff04ff
    ff05ffff06ffff02ff08ffff04ff02ffff04ff09ffff04ff0bff808080808080
    80ffff04ffff05ffff06ffff02ff08ffff04ff02ffff04ff0dffff04ff0bff80
    808080808080ff8080808080ffff04ffff10ffff05ffff06ffff06ffff02ff08
    ffff04ff02ffff04ff09ffff04ff0bff8080808080808080ffff05ffff06ffff
    06ffff02ff08ffff04ff02ffff04ff0dffff04ff0bff808080808080808080ff
    80808080ffff01ff04ffff0bffff0101ffff02ff1effff04ff02ffff04ff15ff
    8080808080ffff04ffff02ff15ffff04ff0bff1d8080ffff01ff0180808080ff
    0180ffff02ffff03ff05ffff01ff04ff09ffff02ff0affff04ff02ffff04ff0d
    ffff04ff0bff808080808080ffff010b80ff0180ffff02ffff03ffff22ffff09
    ff05ff81b780ffff09ff0bff278080ffff0157ffff01ff088080ff0180ff02ff
    ff03ffff07ff0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09
    ff80808080ffff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bff
    ff0101ff058080ff0180ff018080
    "
);

pub const M_OF_N_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "de27deb2ebc7f1e1b77e1d38cc2f9d90fbd54d4b13dd4e6fa1f659177e36ed4f"
));
