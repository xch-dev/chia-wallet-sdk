use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const SLOT_PUZZLE: [u8; 456] = hex!("ff02ffff01ff04ffff04ff08ffff04ffff30ff2fffff02ff1effff04ff02ffff04ff05ffff04ff5fff8080808080ffff010180ff808080ffff04ffff04ff14ffff04ffff0112ffff04ff80ffff04ffff02ff1effff04ff02ffff04ff05ffff04ff81bfff8080808080ff8080808080ff808080ffff04ffff01ffff47ff4302ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff80808080ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff0bff2affff0bff1cffff0bff1cff32ff0980ffff0bff1cffff0bff3affff0bff1cffff0bff1cff32ffff02ff16ffff04ff02ffff04ff05ff8080808080ffff0bff1cffff0bff3affff0bff1cffff0bff1cff32ff0b80ffff0bff1cff32ff22808080ff22808080ff22808080ff018080");

pub const SLOT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    66460af4bd504bc5e26f05698530a46fceb764b354727faf620e3a49065fa513
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot1stCurryArgs {
    pub singleton_struct: SingletonStruct,
    pub nonce: u64,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot2ndCurryArgs {
    pub value_hash: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotSolution {
    pub parent_parent_info: Bytes32,
    pub parent_inner_puzzle_hash: Bytes32,
    pub spender_inner_puzzle_hash: Bytes32,
}

impl Mod for Slot1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SLOT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        SLOT_PUZZLE_HASH
    }
}
