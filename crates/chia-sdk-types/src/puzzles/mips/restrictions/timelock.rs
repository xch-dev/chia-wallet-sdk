use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Timelock {
    pub seconds: u64,
}

impl Timelock {
    pub fn new(seconds: u64) -> Self {
        Self { seconds }
    }
}

impl Mod for Timelock {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&TIMELOCK_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        TIMELOCK_PUZZLE_HASH
    }
}

pub const TIMELOCK_PUZZLE: [u8; 137] = hex!(
    "
    ff02ffff01ff02ff06ffff04ff02ffff04ff05ffff04ff0bff8080808080ffff
    04ffff01ff50ff02ffff03ffff02ffff03ffff09ff23ff0480ffff01ff02ffff
    03ffff09ff53ff0580ffff01ff0101ff8080ff0180ff8080ff0180ffff010bff
    ff01ff04ff13ffff02ff06ffff04ff02ffff04ff05ffff04ff1bff8080808080
    8080ff0180ff018080
    "
);

pub const TIMELOCK_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "a6f96d8ecf9bd29e8c41822d231408823707b587bc0d372e5db4ac9733cbea3c"
));
