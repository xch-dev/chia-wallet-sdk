use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

pub const DELEGATION_LAYER_PUZZLE: [u8; 1027] = hex!(
    "
    ff02ffff01ff02ff12ffff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff2fffff04ff5fff
    ff04ffff02ff81bfff82017f80ffff04ffff02ff16ffff04ff02ffff04ff81bfff80808080ff8080
    8080808080808080ffff04ffff01ffffff3381f3ff02ffffa04bf5122f344554c53bde2ebb8cd2b7
    e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e8
    78a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f680
    6923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff
    ffff02ffff03ffff09ff82017fff1780ffff0181bfffff01ff02ffff03ffff09ff2fffff02ff1eff
    ff04ff02ffff04ffff0bffff0101ff82017f80ffff04ff5fff808080808080ffff01ff02ff1affff
    04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff81bfffff04ffff04ff2fffff04ff0bff8080
    80ff8080808080808080ffff01ff088080ff018080ff0180ff02ffff03ff2fffff01ff02ffff03ff
    ff09ff818fff1880ffff01ff02ff1affff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff6f
    ffff04ff81cfff8080808080808080ffff01ff04ffff02ffff03ffff02ffff03ffff09ff818fffff
    0181e880ffff01ff22ffff09ff820acfff8080ffff09ff8214cfffff01a057bfd1cb0adda3d94315
    053fda723f2028320faa8338225d99f629e3d46d43a98080ffff01ff010180ff0180ffff014fffff
    01ff088080ff0180ffff02ff1affff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff6fffff
    04ff5fff80808080808080808080ff0180ffff01ff04ffff04ff10ffff04ffff0bff5cffff0bff14
    ffff0bff14ff6cff0580ffff0bff14ffff0bff7cffff0bff14ffff0bff14ff6cffff0bffff0101ff
    058080ffff0bff14ffff0bff7cffff0bff14ffff0bff14ff6cffff0bffff0101ff0b8080ffff0bff
    14ffff0bff7cffff0bff14ffff0bff14ff6cffff0bffff0101ff178080ffff0bff14ffff0bff7cff
    ff0bff14ffff0bff14ff6cffff0bffff0101ff819f8080ffff0bff14ff6cff4c808080ff4c808080
    ff4c808080ff4c808080ff4c808080ffff04ffff0101ffff04ff81dfff8080808080ff808080ff01
    80ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff8080
    8080ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff
    02ffff03ff1bffff01ff02ff1effff04ff02ffff04ffff02ffff03ffff18ffff0101ff1380ffff01
    ff0bffff0102ff2bff0580ffff01ff0bffff0102ff05ff2b8080ff0180ffff04ffff04ffff17ff13
    ffff0181ff80ff3b80ff8080808080ffff010580ff0180ff018080
    "
);

pub const DELEGATION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    c3b249466cb15c51e5abb5c54ef5077c1624ae2e6a0f8f7a3fa197a943a5d62e
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DelegationLayerArgs {
    pub mod_hash: Bytes32,
    pub launcher_id: Bytes32,
    pub owner_puzzle_hash: Bytes32,
    pub merkle_root: Bytes32,
}

impl DelegationLayerArgs {
    pub fn new(launcher_id: Bytes32, owner_puzzle_hash: Bytes32, merkle_root: Bytes32) -> Self {
        Self {
            mod_hash: DELEGATION_LAYER_PUZZLE_HASH.into(),
            launcher_id,
            owner_puzzle_hash,
            merkle_root,
        }
    }
}

impl DelegationLayerArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        owner_puzzle_hash: Bytes32,
        merkle_root: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: DELEGATION_LAYER_PUZZLE_HASH,
            args: DelegationLayerArgs {
                mod_hash: DELEGATION_LAYER_PUZZLE_HASH.into(),
                launcher_id,
                owner_puzzle_hash,
                merkle_root,
            },
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct DelegationLayerSolution<P, S> {
    pub merkle_proof: Option<MerkleProof>,
    pub puzzle_reveal: P,
    pub puzzle_solution: S,
}

impl Mod for DelegationLayerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DELEGATION_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DELEGATION_LAYER_PUZZLE_HASH
    }
}
