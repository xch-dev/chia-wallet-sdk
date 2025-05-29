use std::borrow::Cow;

use chia_protocol::{Bytes, Bytes32};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PasskeyMemberPuzzleAssert {
    pub public_key: R1PublicKey,
}

impl PasskeyMemberPuzzleAssert {
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for PasskeyMemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PASSKEY_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        PASSKEY_MEMBER_PUZZLE_ASSERT_HASH
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PasskeyMemberPuzzleAssertSolution {
    pub authenticator_data: Bytes,
    pub client_data_json: Bytes,
    pub challenge_index: usize,
    pub signature: R1Signature,
    pub puzzle_hash: Bytes32,
}

pub const PASSKEY_MEMBER_PUZZLE_ASSERT: [u8; 1418] = hex!(
    "
    ff02ffff01ff02ff3effff04ff02ffff04ff03ffff04ffff02ff2cffff04ff02
    ffff04ffff0bff0bff82017f80ff80808080ff8080808080ffff04ffff01ffff
    ffff02ffff03ffff21ffff09ff0bffff0dff178080ffff15ff0bffff0dff1780
    8080ffff01ff0180ffff01ff04ffff02ff05ffff04ffff0cff17ff0bffff10ff
    0bffff01038080ff808080ffff02ff10ffff04ff02ffff04ff05ffff04ffff10
    ff0bffff010380ffff04ff17ff8080808080808080ff0180ff02ffff03ff0bff
    ff01ff02ffff03ff13ffff01ff04ffff02ff05ffff04ff23ff808080ffff02ff
    18ffff04ff02ffff04ff05ffff04ffff04ff33ff1b80ff808080808080ffff01
    ff02ff18ffff04ff02ffff04ff05ffff04ff1bff808080808080ff0180ffff01
    ff018080ff0180ffff0cffff01c0404142434445464748494a4b4c4d4e4f5051
    52535455565758595a6162636465666768696a6b6c6d6e6f7071727374757677
    78797a303132333435363738392d5fff05ffff10ff05ffff01018080ffff02ff
    3cffff04ff02ffff04ff03ffff04ffff06ffff14ffff0dff0580ffff01038080
    ff8080808080ff02ff12ffff04ff02ffff04ff03ffff04ffff02ffff03ff0bff
    ff01ff11ffff0103ff0b80ffff01ff018080ff0180ff8080808080ffffff02ff
    2affff04ff02ffff04ff03ffff04ffff0eff11ffff0cffff0183000000ff80ff
    0b8080ff8080808080ffff02ff2effff04ff02ffff04ff03ffff04ffff02ff10
    ffff04ff02ffff04ffff04ffff0102ffff04ffff04ffff0101ffff04ffff0102
    ffff04ffff04ffff0101ff1680ffff04ffff04ffff0104ffff04ffff04ffff01
    01ff0280ffff04ffff0101ff80808080ff8080808080ffff04ffff04ffff0104
    ffff04ffff04ffff0101ffff04ff15ff808080ffff04ffff0101ff80808080ff
    80808080ffff04ff80ffff04ff0bff808080808080ff8080808080ff04ff4fff
    ff04ffff19ffff16ff6fffff010480ff2780ffff04ffff19ffff16ff37ffff01
    0280ff1380ffff04ff1bff8080808080ffff02ff3affff04ff02ffff04ffff04
    ffff04ff09ff8080ffff04ff0bff808080ffff04ffff14ffff02ffff03ffff15
    ffff0cff0bffff0102ffff010380ffff0181ff80ffff01ff0cff0bffff0102ff
    ff010380ffff01ff10ffff0cff0bffff0102ffff010380ffff018201008080ff
    0180ffff014080ffff04ffff14ffff02ffff03ffff15ffff0cff0bffff0101ff
    ff010280ffff0181ff80ffff01ff0cff0bffff0101ffff010280ffff01ff10ff
    ff0cff0bffff0101ffff010280ffff018201008080ff0180ffff011080ffff04
    ffff14ffff02ffff03ffff15ffff0cff0bff80ffff010180ffff0181ff80ffff
    01ff0cff0bff80ffff010180ffff01ff10ffff0cff0bff80ffff010180ffff01
    8201008080ff0180ffff010480ff80808080808080ffff0cffff02ffff04ffff
    04ffff010eff8080ffff02ff18ffff04ff02ffff04ffff04ffff0102ffff04ff
    ff04ffff0101ff1480ffff04ffff04ffff0104ffff04ffff04ffff0101ff0280
    ffff04ffff0101ff80808080ff80808080ffff04ff0bff808080808080ff8080
    ff80ffff11ffff0dffff02ffff04ffff04ffff010eff8080ffff02ff18ffff04
    ff02ffff04ffff04ffff0102ffff04ffff04ffff0101ff1480ffff04ffff04ff
    ff0104ffff04ffff04ffff0101ff0280ffff04ffff0101ff80808080ff808080
    80ffff04ff0bff808080808080ff808080ff298080ff02ffff03ffff09ffff0c
    ff5dff8200bdffff10ff8200bdffff0dffff0effff018d226368616c6c656e67
    65223a22ff0bffff012280808080ffff0effff018d226368616c6c656e676522
    3a22ff0bffff01228080ffff01ff04ffff04ffff0148ffff04ff8202fdff8080
    80ffff841c3a8f00ff09ffff0bff2dffff0bff5d8080ff82017d8080ffff01ff
    088080ff0180ff018080
    "
);

pub const PASSKEY_MEMBER_PUZZLE_ASSERT_HASH: TreeHash = TreeHash::new(hex!(
    "e6db5ba2eeded13c47216512a7a4662b95121c145580db6312cb711aaadcec32"
));
