mod members;
mod restrictions;
mod spend;

use chia_sdk_types::{AddDelegatedPuzzleWrapper, Mod};
pub use members::*;
pub use restrictions::*;
pub use spend::*;

use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_driver as sdk;
use clvm_utils::TreeHash;

use crate::{Coin, LineageProof, Program};

#[derive(Clone)]
pub struct Vault {
    pub coin: Coin,
    pub launcher_id: Bytes32,
    pub proof: LineageProof,
    pub custody_hash: TreeHash,
}

impl Vault {
    pub fn child(&self, custody_hash: TreeHash) -> Result<Self> {
        Ok(sdk::Vault::from(self.clone()).child(custody_hash).into())
    }
}

impl From<sdk::Vault> for Vault {
    fn from(value: sdk::Vault) -> Self {
        Vault {
            coin: value.coin.into(),
            launcher_id: value.launcher_id,
            proof: value.proof.into(),
            custody_hash: value.custody_hash,
        }
    }
}

impl From<Vault> for sdk::Vault {
    fn from(value: Vault) -> Self {
        sdk::Vault {
            coin: value.coin.into(),
            launcher_id: value.launcher_id,
            proof: value.proof.into(),
            custody_hash: value.custody_hash,
        }
    }
}

#[derive(Clone)]
pub struct VaultMint {
    pub vault: Vault,
    pub parent_conditions: Vec<Program>,
}

pub fn wrapped_delegated_puzzle_hash(
    restrictions: Vec<Restriction>,
    delegated_puzzle_hash: TreeHash,
) -> Result<TreeHash> {
    let mut delegated_puzzle_hash = delegated_puzzle_hash;

    for restriction in restrictions.into_iter().rev() {
        if !matches!(restriction.kind, RestrictionKind::DelegatedPuzzleWrapper) {
            continue;
        }

        delegated_puzzle_hash =
            AddDelegatedPuzzleWrapper::new(restriction.puzzle_hash, delegated_puzzle_hash)
                .curry_tree_hash();
    }

    Ok(delegated_puzzle_hash)
}
