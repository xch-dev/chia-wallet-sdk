use chia_protocol::{Bytes, Bytes32};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PasskeyMember {
    pub genesis_challenge: Bytes32,
    pub public_key: R1PublicKey,
}

impl PasskeyMember {
    pub fn new(genesis_challenge: Bytes32, public_key: R1PublicKey) -> Self {
        Self {
            genesis_challenge,
            public_key,
        }
    }
}

impl Mod for PasskeyMember {
    const MOD_REVEAL: &[u8] = &PASSKEY_MEMBER;
    const MOD_HASH: TreeHash = PASSKEY_MEMBER_HASH;
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct PasskeyMemberSolution {
    pub authenticator_data: Bytes,
    pub client_data_json: Bytes,
    pub challenge_index: usize,
    pub signature: R1Signature,
    pub coin_id: Bytes32,
}

pub const PASSKEY_MEMBER: [u8; 1424] = hex!(
    "
    ff02ffff01ff02ff3effff04ff02ffff04ff03ffff04ffff02ff2cffff04ff02
    ffff04ffff0bff17ff8202ffff0580ff80808080ff8080808080ffff04ffff01
    ffffffff02ffff03ffff21ffff09ff0bffff0dff178080ffff15ff0bffff0dff
    17808080ffff01ff0180ffff01ff04ffff02ff05ffff04ffff0cff17ff0bffff
    10ff0bffff01038080ff808080ffff02ff10ffff04ff02ffff04ff05ffff04ff
    ff10ff0bffff010380ffff04ff17ff8080808080808080ff0180ff02ffff03ff
    0bffff01ff02ffff03ff13ffff01ff04ffff02ff05ffff04ff23ff808080ffff
    02ff18ffff04ff02ffff04ff05ffff04ffff04ff33ff1b80ff808080808080ff
    ff01ff02ff18ffff04ff02ffff04ff05ffff04ff1bff808080808080ff0180ff
    ff01ff018080ff0180ffff0cffff01c0404142434445464748494a4b4c4d4e4f
    505152535455565758595a6162636465666768696a6b6c6d6e6f707172737475
    767778797a303132333435363738392d5fff05ffff10ff05ffff01018080ffff
    02ff3cffff04ff02ffff04ff03ffff04ffff06ffff14ffff0dff0580ffff0103
    8080ff8080808080ff02ff12ffff04ff02ffff04ff03ffff04ffff02ffff03ff
    0bffff01ff11ffff0103ff0b80ffff01ff018080ff0180ff8080808080ffffff
    02ff2affff04ff02ffff04ff03ffff04ffff0eff11ffff0cffff0183000000ff
    80ff0b8080ff8080808080ffff02ff2effff04ff02ffff04ff03ffff04ffff02
    ff10ffff04ff02ffff04ffff04ffff0102ffff04ffff04ffff0101ffff04ffff
    0102ffff04ffff04ffff0101ff1680ffff04ffff04ffff0104ffff04ffff04ff
    ff0101ff0280ffff04ffff0101ff80808080ff8080808080ffff04ffff04ffff
    0104ffff04ffff04ffff0101ffff04ff15ff808080ffff04ffff0101ff808080
    80ff80808080ffff04ff80ffff04ff0bff808080808080ff8080808080ff04ff
    4fffff04ffff19ffff16ff6fffff010480ff2780ffff04ffff19ffff16ff37ff
    ff010280ff1380ffff04ff1bff8080808080ffff02ff3affff04ff02ffff04ff
    ff04ffff04ff09ff8080ffff04ff0bff808080ffff04ffff14ffff02ffff03ff
    ff15ffff0cff0bffff0102ffff010380ffff0181ff80ffff01ff0cff0bffff01
    02ffff010380ffff01ff10ffff0cff0bffff0102ffff010380ffff0182010080
    80ff0180ffff014080ffff04ffff14ffff02ffff03ffff15ffff0cff0bffff01
    01ffff010280ffff0181ff80ffff01ff0cff0bffff0101ffff010280ffff01ff
    10ffff0cff0bffff0101ffff010280ffff018201008080ff0180ffff011080ff
    ff04ffff14ffff02ffff03ffff15ffff0cff0bff80ffff010180ffff0181ff80
    ffff01ff0cff0bff80ffff010180ffff01ff10ffff0cff0bff80ffff010180ff
    ff018201008080ff0180ffff010480ff80808080808080ffff0cffff02ffff04
    ffff04ffff010eff8080ffff02ff18ffff04ff02ffff04ffff04ffff0102ffff
    04ffff04ffff0101ff1480ffff04ffff04ffff0104ffff04ffff04ffff0101ff
    0280ffff04ffff0101ff80808080ff80808080ffff04ff0bff808080808080ff
    8080ff80ffff11ffff0dffff02ffff04ffff04ffff010eff8080ffff02ff18ff
    ff04ff02ffff04ffff04ffff0102ffff04ffff04ffff0101ff1480ffff04ffff
    04ffff0104ffff04ffff04ffff0101ff0280ffff04ffff0101ff80808080ff80
    808080ffff04ff0bff808080808080ff808080ff298080ff02ffff03ffff09ff
    ff0cff8200bdff82017dffff10ff82017dffff0dffff0effff018d226368616c
    6c656e6765223a22ff0bffff012280808080ffff0effff018d226368616c6c65
    6e6765223a22ff0bffff01228080ffff01ff04ffff04ffff0146ffff04ff8205
    fdff808080ffff841c3a8f00ff15ffff0bff5dffff0bff8200bd8080ff8202fd
    8080ffff01ff088080ff0180ff018080
    "
);

pub const PASSKEY_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "2877c080c18a408111ec86b108da56dd667f968ce38f87623ca084934127059c"
));
