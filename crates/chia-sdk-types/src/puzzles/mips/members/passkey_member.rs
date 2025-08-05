use std::borrow::Cow;

use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::{PASSKEY_MEMBER, PASSKEY_MEMBER_HASH};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PasskeyMember {
    pub public_key: R1PublicKey,
}

impl PasskeyMember {
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for PasskeyMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PASSKEY_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        PASSKEY_MEMBER_HASH.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PasskeyMemberSolution {
    pub authenticator_data: Bytes,
    pub client_data_json: Bytes,
    pub challenge_index: usize,
    pub signature: R1Signature,
    pub coin_id: Bytes32,
}
