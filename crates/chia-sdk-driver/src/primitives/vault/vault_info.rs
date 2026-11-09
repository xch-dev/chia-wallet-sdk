use chia_protocol::Bytes32;
use clvm_utils::TreeHash;

use crate::SingletonInfo;

/// Information needed to construct the puzzle hash of a [`Vault`](crate::Vault).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultInfo {
    /// The coin id of the launcher coin that created this vault's singleton.
    pub launcher_id: Bytes32,
    /// The tree hash of the vault's custody puzzle, which is a top level MIPS puzzle
    /// (see [`mips_puzzle_hash`](crate::mips_puzzle_hash)). This is the singleton's inner puzzle hash.
    pub custody_hash: TreeHash,
}

impl VaultInfo {
    /// Creates a new [`VaultInfo`].
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
