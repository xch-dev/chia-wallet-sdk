use std::borrow::Cow;

use chia_puzzles::{
    NFT_METADATA_UPDATER_DEFAULT, NFT_METADATA_UPDATER_DEFAULT_HASH, SETTLEMENT_PAYMENT,
    SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER, SINGLETON_LAUNCHER_HASH,
};
use clvm_utils::TreeHash;

use crate::Mod;

/// The default NFT metadata updater puzzle, which has no curried arguments.
///
/// Its solution is `(key . new_url)`. If `key` is `"u"` (data URIs), `"mu"` (metadata URIs), or
/// `"lu"` (license URIs) and that key is in the NFT's metadata, the new URL is added to the front
/// of its list. Otherwise, the metadata is unchanged. It never changes the metadata updater puzzle
/// hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftMetadataUpdater;

impl Mod for NftMetadataUpdater {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&NFT_METADATA_UPDATER_DEFAULT)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(NFT_METADATA_UPDATER_DEFAULT_HASH)
    }
}

/// The singleton launcher puzzle, which has no curried arguments.
///
/// A singleton is launched by creating a coin with this puzzle and then spending it. The
/// launcher's coin id becomes the singleton's launcher id. Its solution is
/// `(singleton_full_puzzle_hash amount key_value_list)`, and it creates the singleton coin along
/// with a coin announcement of the tree hash of its solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingletonLauncher;

impl Mod for SingletonLauncher {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SINGLETON_LAUNCHER)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(SINGLETON_LAUNCHER_HASH)
    }
}

/// The settlement payments puzzle used by offers, which has no curried arguments.
///
/// Anyone can spend a coin with this puzzle. Its solution is a list of notarized payments, and
/// for each one, it creates the payments and a puzzle announcement of the notarized payment's
/// tree hash. The other side of an offer asserts those announcements with
/// [`payment_assertion`](crate::payment_assertion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementPayment;

impl Mod for SettlementPayment {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SETTLEMENT_PAYMENT)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(SETTLEMENT_PAYMENT_HASH)
    }
}
