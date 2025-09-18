use chia_protocol::Bytes32;
use clvm_utils::TreeHash;

use crate::SingletonInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultInfo {
    pub launcher_id: Bytes32,
    pub custody_hash: TreeHash,
}

impl VaultInfo {
    pub fn new(launcher_id: Bytes32, custody_hash: TreeHash) -> Self {
        Self {
            launcher_id,
            custody_hash,
        }
    }
}

impl SingletonInfo for VaultInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        self.custody_hash
    }
}
