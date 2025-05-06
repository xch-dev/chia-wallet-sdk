use std::borrow::Cow;

use chia_puzzles::{
    NFT_METADATA_UPDATER_DEFAULT, NFT_METADATA_UPDATER_DEFAULT_HASH, SETTLEMENT_PAYMENT,
    SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER, SINGLETON_LAUNCHER_HASH,
};
use clvm_utils::TreeHash;

use crate::Mod;

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
