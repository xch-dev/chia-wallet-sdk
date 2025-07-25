use std::borrow::Cow;

use chia_bls::PublicKey;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const P2_M_OF_N_DELEGATE_DIRECT_PUZZLE: [u8; 453] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff05ffff02ff16ffff04ff02ffff04ff17ff80
    80808080ffff01ff02ff0cffff04ff02ffff04ffff02ff0affff04ff02ffff04
    ff17ffff04ff0bff8080808080ffff04ffff02ff1effff04ff02ffff04ff2fff
    80808080ffff04ff2fffff04ff5fff80808080808080ffff01ff088080ff0180
    ffff04ffff01ffff31ff02ffff03ff05ffff01ff04ffff04ff08ffff04ff09ff
    ff04ff0bff80808080ffff02ff0cffff04ff02ffff04ff0dffff04ff0bffff04
    ff17ffff04ff2fff8080808080808080ffff01ff02ff17ff2f8080ff0180ffff
    02ffff03ff05ffff01ff02ffff03ff09ffff01ff04ff13ffff02ff0affff04ff
    02ffff04ff0dffff04ff1bff808080808080ffff01ff02ff0affff04ff02ffff
    04ff0dffff04ff1bff808080808080ff0180ff8080ff0180ffff02ffff03ff05
    ffff01ff10ffff02ff16ffff04ff02ffff04ff0dff80808080ffff02ffff03ff
    09ffff01ff0101ff8080ff018080ff8080ff0180ff02ffff03ffff07ff0580ff
    ff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ffff02ff
    1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff01
    80ff018080
    "
);

pub const P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0f199d5263ac1a62b077c159404a71abd3f9691cc57520bf1d4c5cb501504457
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2MOfNDelegateDirectArgs {
    pub m: usize,
    pub public_key_list: Vec<PublicKey>,
}

impl P2MOfNDelegateDirectArgs {
    pub fn new(m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self { m, public_key_list }
    }

    pub fn curry_tree_hash(m: usize, public_key_list: Vec<PublicKey>) -> TreeHash {
        CurriedProgram {
            program: P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH,
            args: Self::new(m, public_key_list),
        }
        .tree_hash()
    }

    pub fn selectors_for_used_pubkeys(
        public_key_list: &[PublicKey],
        used_pubkeys: &[PublicKey],
    ) -> Vec<bool> {
        public_key_list
            .iter()
            .map(|pubkey| used_pubkeys.contains(pubkey))
            .collect()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct P2MOfNDelegateDirectSolution<P, S> {
    pub selectors: Vec<bool>,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl Mod for P2MOfNDelegateDirectArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_M_OF_N_DELEGATE_DIRECT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH
    }
}
