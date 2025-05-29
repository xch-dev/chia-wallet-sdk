use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct OptionContractArgs<I> {
    pub mod_hash: Bytes32,
    pub underlying_coin_id: Bytes32,
    pub underlying_delegated_puzzle_hash: Bytes32,
    pub inner_puzzle: I,
}

impl<I> OptionContractArgs<I> {
    pub fn new(
        underlying_coin_id: Bytes32,
        underlying_delegated_puzzle_hash: Bytes32,
        inner_puzzle: I,
    ) -> Self {
        Self {
            mod_hash: OPTION_CONTRACT_PUZZLE_HASH.into(),
            underlying_coin_id,
            underlying_delegated_puzzle_hash,
            inner_puzzle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct OptionContractSolution<I> {
    pub inner_solution: I,
}

impl<I> OptionContractSolution<I> {
    pub fn new(inner_solution: I) -> Self {
        Self { inner_solution }
    }
}

impl<I> Mod for OptionContractArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&OPTION_CONTRACT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        OPTION_CONTRACT_PUZZLE_HASH
    }
}

pub const OPTION_CONTRACT_PUZZLE: [u8; 862] = hex!(
    "
    ff02ffff01ff02ff1effff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04
    ffff02ff2fff5f80ffff01ff80ff8080808080808080ffff04ffff01ffffff33
    42ff02ff02ffff03ff05ffff01ff0bff72ffff02ff16ffff04ff02ffff04ff09
    ffff04ffff02ff1cffff04ff02ffff04ff0dff80808080ff808080808080ffff
    016280ff0180ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631
    c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b0837
    21e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd
    2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e18
    79b7152a6e7298a91ce119a63400ade7c5ff0bff52ffff02ff16ffff04ff02ff
    ff04ff05ffff04ffff02ff1cffff04ff02ffff04ff07ff80808080ff80808080
    8080ffff0bff14ffff0bff14ff62ff0580ffff0bff14ff0bff428080ff02ffff
    03ff2fffff01ff02ffff03ffff09ff818fff1080ffff01ff02ffff03ffff09ff
    8202cfffff01818f80ffff01ff04ff4fffff02ff1effff04ff02ffff04ff05ff
    ff04ff0bffff04ff17ffff04ff6fffff04ffff0101ffff04ff81bfff80808080
    808080808080ffff01ff04ffff04ff10ffff04ffff02ff1affff04ff02ffff04
    ff05ffff04ffff0bffff0101ff0580ffff04ffff0bffff0101ff0b80ffff04ff
    ff0bffff0101ff1780ffff04ff82014fff8080808080808080ff8201cf8080ff
    ff02ff1effff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff6fffff04
    ff5fffff04ff81bfff8080808080808080808080ff0180ffff01ff02ffff03ff
    ff02ffff03ffff09ff818fff1880ffff01ff02ffff03ffff22ffff09ff82014f
    ffff011780ffff09ff8205cfff0b8080ffff01ff0101ff8080ff0180ff8080ff
    0180ffff01ff02ffff03ffff09ff8202cfff1780ffff01ff04ff4fffff02ff1e
    ffff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff6fffff04ff5fffff
    01ff01808080808080808080ffff01ff088080ff0180ffff01ff04ff4fffff02
    ff1effff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ff6fffff04ff5f
    ffff04ff81bfff8080808080808080808080ff018080ff0180ffff01ff02ffff
    03ffff09ff5fff81bf80ff80ffff01ff088080ff018080ff0180ff018080
    "
);

pub const OPTION_CONTRACT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "5a084d1786fc0fe43c30bc5fc0233cc1a791cfde3a25580a9ca4883878f0ba63"
));
