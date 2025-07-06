use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
};
use chia_puzzles::CAT_PUZZLE_HASH;
use chia_wallet_sdk::driver::{DriverError, SpendContext};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::SpendContextExt;

pub const DEFAULT_CAT_MAKER_PUZZLE: [u8; 283] = hex!("ff02ffff01ff0bff16ffff0bff04ffff0bff04ff1aff0580ffff0bff04ffff0bff1effff0bff04ffff0bff04ff1affff0bffff0101ff058080ffff0bff04ffff0bff1effff0bff04ffff0bff04ff1aff0b80ffff0bff04ffff0bff1effff0bff04ffff0bff04ff1aff1780ffff0bff04ff1aff12808080ff12808080ff12808080ff12808080ffff04ffff01ff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff018080");

pub const DEFAULT_CAT_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0370e9c0343398cbe3487fb93d4aa24357005cdd67894e1cbae14772e778a75a
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultCatMakerArgs {
    pub cat_mod_hash: Bytes32,
    pub tail_hash_hash: Bytes32,
}

impl DefaultCatMakerArgs {
    pub fn new(tail_hash_hash: Bytes32) -> Self {
        Self {
            cat_mod_hash: CAT_PUZZLE_HASH.into(),
            tail_hash_hash,
        }
    }
}

impl DefaultCatMakerArgs {
    pub fn curry_tree_hash(tail_hash_hash: Bytes32) -> TreeHash {
        CurriedProgram {
            program: DEFAULT_CAT_MAKER_PUZZLE_HASH,
            args: DefaultCatMakerArgs::new(tail_hash_hash),
        }
        .tree_hash()
    }

    pub fn get_puzzle(
        ctx: &mut SpendContext,
        tail_hash_hash: Bytes32,
    ) -> Result<NodePtr, DriverError> {
        let cat_maker_puzzle = ctx.default_cat_maker_puzzle()?;

        ctx.alloc(&CurriedProgram {
            program: cat_maker_puzzle,
            args: DefaultCatMakerArgs::new(tail_hash_hash),
        })
    }
}
