mod members;
mod memos;
mod restrictions;
mod spend;

pub use members::*;
pub use memos::*;
pub use restrictions::*;
pub use spend::*;

use bindy::Result;
use chia_protocol::Coin;
use chia_sdk_driver::{self as sdk, VaultInfo};
use chia_sdk_types::{puzzles::AddDelegatedPuzzleWrapper, Mod};
use clvm_utils::TreeHash;

use crate::{Program, Proof};

#[derive(Clone)]
pub struct Vault {
    pub coin: Coin,
    pub proof: Proof,
    pub info: VaultInfo,
}

impl Vault {
    pub fn child(&self, custody_hash: TreeHash, amount: u64) -> Result<Self> {
        Ok(sdk::Vault::from(self.clone())
            .child(custody_hash, amount)
            .into())
    }
}

impl From<sdk::Vault> for Vault {
    fn from(value: sdk::Vault) -> Self {
        Vault {
            coin: value.coin,
            proof: value.proof.into(),
            info: value.info,
        }
    }
}

impl From<Vault> for sdk::Vault {
    fn from(value: Vault) -> Self {
        sdk::Vault::new(value.coin, value.proof.into(), value.info)
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
