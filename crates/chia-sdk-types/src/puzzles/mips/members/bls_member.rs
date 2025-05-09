use std::borrow::Cow;

use chia_bls::PublicKey;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsMember {
    pub public_key: PublicKey,
}

impl BlsMember {
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for BlsMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        BLS_MEMBER_HASH
    }
}

pub const BLS_MEMBER: [u8; 41] = hex!(
    "
    ff02ffff01ff04ffff04ff02ffff04ff05ffff04ff0bff80808080ff8080ffff
    04ffff0132ff018080
    "
);

pub const BLS_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "21a3ae8b3ce64d41ca98d6d8df8f465c9e1bfb19ab40284a5da8479ba7fade78"
));
