use chia::{bls::PublicKey, clvm_utils::TreeHash, protocol::Bytes32};
use chia_wallet_sdk::driver::SingletonLayer;
use clvm_traits::{FromClvm, ToClvm};

use crate::{MOfNLayer, P2MOfNDelegateDirectArgs};

type MedievalVaultLayers = SingletonLayer<MOfNLayer>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MedievalVaultInfo {
    pub launcher_id: Bytes32,

    pub m: usize,
    pub public_key_list: Vec<PublicKey>,
}

impl MedievalVaultInfo {
    pub fn new(launcher_id: Bytes32, m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self {
            launcher_id,
            m,
            public_key_list,
        }
    }

    pub fn from_hint(hint: MedievalVaultHint) -> Self {
        Self {
            launcher_id: hint.my_launcher_id,
            m: hint.m,
            public_key_list: hint.public_key_list,
        }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        P2MOfNDelegateDirectArgs::curry_tree_hash(self.m, self.public_key_list.clone())
    }

    pub fn into_layers(&self) -> MedievalVaultLayers {
        SingletonLayer::new(
            self.launcher_id,
            MOfNLayer::new(self.m, self.public_key_list.clone()),
        )
    }

    pub fn to_hint(&self) -> MedievalVaultHint {
        MedievalVaultHint {
            my_launcher_id: self.launcher_id,
            m: self.m,
            public_key_list: self.public_key_list.clone(),
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct MedievalVaultHint {
    pub my_launcher_id: Bytes32,
    pub m: usize,
    #[clvm(rest)]
    pub public_key_list: Vec<PublicKey>,
}
