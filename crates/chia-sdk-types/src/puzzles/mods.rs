use chia_puzzles::{
    NFT_METADATA_UPDATER_DEFAULT, NFT_METADATA_UPDATER_DEFAULT_HASH, SETTLEMENT_PAYMENT,
    SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER, SINGLETON_LAUNCHER_HASH,
};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftMetadataUpdater;

impl Mod for NftMetadataUpdater {
    const MOD_REVEAL: &[u8] = &NFT_METADATA_UPDATER_DEFAULT;
    const MOD_HASH: TreeHash = TreeHash::new(NFT_METADATA_UPDATER_DEFAULT_HASH);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingletonLauncher;

impl Mod for SingletonLauncher {
    const MOD_REVEAL: &[u8] = &SINGLETON_LAUNCHER;
    const MOD_HASH: TreeHash = TreeHash::new(SINGLETON_LAUNCHER_HASH);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementPayment;

impl Mod for SettlementPayment {
    const MOD_REVEAL: &[u8] = &SETTLEMENT_PAYMENT;
    const MOD_HASH: TreeHash = TreeHash::new(SETTLEMENT_PAYMENT_HASH);
}
