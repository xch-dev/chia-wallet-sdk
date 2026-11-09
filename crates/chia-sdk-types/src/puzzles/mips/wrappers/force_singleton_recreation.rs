use std::borrow::Cow;

use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

// TODO: Import these from `chia-puzzles` after they are published.
/// The serialized CLVM bytecode of the MIPS `force_singleton_recreation` restriction. See
/// [`ForceSingletonRecreationMod`].
pub const FORCE_SINGLETON_RECREATION: [u8; 243] = hex!(
    "
    ff02ffff01ff02ff1effff04ff02ffff04ffff02ff16ffff04ff02ffff04ff05
    ff80808080ffff04ff05ff8080808080ffff04ffff01ffff4933ffff02ffff03
    ffff09ff09ff0c80ffff01ff02ffff03ffff18ff2dffff010180ffff01ff04ff
    13ff2d80ffff010b80ff0180ffff01ff02ffff03ffff09ff09ff0880ffff01ff
    04ff15ff1b80ffff010b80ff018080ff0180ffff02ffff03ff05ffff01ff02ff
    0affff04ff02ffff04ff09ffff04ffff02ff16ffff04ff02ffff04ff0dff8080
    8080ff8080808080ffff01ff01ff808080ff0180ff02ffff03ffff09ff09ff0d
    80ffff010bffff01ff088080ff0180ff018080
    "
);
/// The tree hash of [`FORCE_SINGLETON_RECREATION`].
pub const FORCE_SINGLETON_RECREATION_HASH: [u8; 32] =
    hex!("63e04374ced7e7f9ab0f46e9bad1b4e82bf3edf671ea355cd60113c7a947ee1e");

/// The MIPS `force_singleton_recreation` restriction, which requires a singleton to be recreated
/// with the same amount.
///
/// It requires an odd `CREATE_COIN` whose amount matches the amount in an `ASSERT_MY_AMOUNT`
/// condition, or `1` if there isn't one. This prevents the singleton from being melted. It has no
/// curried arguments.
///
/// Like the other restrictions that take a list of conditions, it can be used as a member
/// validator in [`RestrictionsArgs`](crate::puzzles::RestrictionsArgs) or as a delegated puzzle
/// wrapper with [`AddDelegatedPuzzleWrapper`](crate::puzzles::AddDelegatedPuzzleWrapper).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceSingletonRecreationMod;

impl Mod for ForceSingletonRecreationMod {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FORCE_SINGLETON_RECREATION)
    }

    fn mod_hash() -> TreeHash {
        FORCE_SINGLETON_RECREATION_HASH.into()
    }
}
