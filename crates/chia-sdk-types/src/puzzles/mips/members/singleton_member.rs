use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct SingletonMember {
    pub singleton_struct: SingletonStruct,
}

impl SingletonMember {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_struct: SingletonStruct::new(launcher_id),
        }
    }
}

impl Mod for SingletonMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SINGLETON_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        SINGLETON_MEMBER_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct SingletonMemberSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub singleton_amount: u64,
}

impl SingletonMemberSolution {
    pub fn new(singleton_inner_puzzle_hash: Bytes32, singleton_amount: u64) -> Self {
        Self {
            singleton_inner_puzzle_hash,
            singleton_amount,
        }
    }
}

pub const SINGLETON_MEMBER: [u8; 361] = hex!(
    "
    ff02ffff01ff04ffff04ff12ffff04ffff0117ffff04ff0bffff04ffff02ff2e
    ffff04ff02ffff04ff09ffff04ff17ffff04ffff02ff3effff04ff02ffff04ff
    05ff80808080ff808080808080ff8080808080ff8080ffff04ffff01ffffff02
    04ff0101ffff4302ffff02ffff03ff05ffff01ff02ff16ffff04ff02ffff04ff
    0dffff04ffff0bff1affff0bff14ff1880ffff0bff1affff0bff1affff0bff14
    ff1c80ff0980ffff0bff1aff0bffff0bff14ff8080808080ff8080808080ffff
    010b80ff0180ffff0bff1affff0bff14ff1080ffff0bff1affff0bff1affff0b
    ff14ff1c80ff0580ffff0bff1affff02ff16ffff04ff02ffff04ff07ffff04ff
    ff0bff14ff1480ff8080808080ffff0bff14ff8080808080ff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff80808080
    ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff018080
    "
);

pub const SINGLETON_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "6f1cebc5a6d3661ad87d3558146259ca580729b244b7662757f8d1c34a6a9ad9"
));
