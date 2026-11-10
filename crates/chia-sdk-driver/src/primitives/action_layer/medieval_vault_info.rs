use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::P2MOfNDelegateDirectArgs;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::{MOfNLayer, SingletonInfo, SingletonLayer};

type MedievalVaultLayers = SingletonLayer<MOfNLayer>;

/// The information needed to construct the puzzle of a [`MedievalVault`](crate::MedievalVault).
///
/// The inner puzzle is an m-of-n BLS multisig ([`P2MOfNDelegateDirectArgs`]): any `m` of the keys in
/// `public_key_list` can sign a delegated puzzle that the vault then runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MedievalVaultInfo {
    /// The launcher id of the vault singleton.
    pub launcher_id: Bytes32,

    /// The number of signatures required to spend the vault.
    pub m: usize,
    /// The public keys that may sign, in the order they are curried into the puzzle.
    pub public_key_list: Vec<PublicKey>,
}

impl MedievalVaultInfo {
    /// Creates a new [`MedievalVaultInfo`].
    pub fn new(launcher_id: Bytes32, m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self {
            launcher_id,
            m,
            public_key_list,
        }
    }

    /// Creates a [`MedievalVaultInfo`] from a [`MedievalVaultHint`].
    pub fn from_hint(hint: MedievalVaultHint) -> Self {
        Self {
            launcher_id: hint.my_launcher_id,
            m: hint.m,
            public_key_list: hint.public_key_list,
        }
    }

    /// Returns the singleton and m-of-n layers of the vault puzzle.
    pub fn into_layers(&self) -> MedievalVaultLayers {
        SingletonLayer::new(
            self.launcher_id,
            MOfNLayer::new(self.m, self.public_key_list.clone()),
        )
    }

    /// Returns the [`MedievalVaultHint`] that describes this vault.
    pub fn to_hint(&self) -> MedievalVaultHint {
        MedievalVaultHint {
            my_launcher_id: self.launcher_id,
            m: self.m,
            public_key_list: self.public_key_list.clone(),
        }
    }
}

impl SingletonInfo for MedievalVaultInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        P2MOfNDelegateDirectArgs::curry_tree_hash(self.m, self.public_key_list.clone())
    }
}

/// Describes a vault's launcher id and multisig configuration so it can be found and parsed.
///
/// The hint is stored in the launcher solution's key-value list when the vault is launched, and in
/// the memos of the vault's `CREATE_COIN` condition whenever its keys change.
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct MedievalVaultHint {
    /// The launcher id of the vault. It comes first so it also acts as the coin's hint.
    pub my_launcher_id: Bytes32,
    /// The number of signatures required to spend the vault.
    pub m: usize,
    /// The public keys that may sign.
    #[clvm(rest)]
    pub public_key_list: Vec<PublicKey>,
}
