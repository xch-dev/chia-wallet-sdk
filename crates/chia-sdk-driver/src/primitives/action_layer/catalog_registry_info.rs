use chia::{
    clvm_utils::{ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::singleton::SingletonArgs,
};
use chia_wallet_sdk::{
    driver::{DriverError, Layer, Puzzle, SingletonLayer},
    types::MerkleTree,
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::Allocator;
use hex_literal::hex;

use crate::{
    Action, ActionLayer, ActionLayerArgs, CatalogRefundAction, CatalogRegisterAction,
    DefaultFinalizer2ndCurryArgs, DelegatedStateAction, Finalizer,
};

use super::CatalogRegistry;

pub type CatalogRegistryLayers = SingletonLayer<ActionLayer<CatalogRegistryState>>;

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct CatalogRegistryState {
    pub cat_maker_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub registration_price: u64,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct CatalogRegistryConstants {
    pub launcher_id: Bytes32,
    pub royalty_address: Bytes32,
    pub royalty_basis_points: u16,
    pub precommit_payout_puzzle_hash: Bytes32,
    pub relative_block_height: u32,
    pub price_singleton_launcher_id: Bytes32,
}

impl CatalogRegistryConstants {
    pub fn get(testnet11: bool) -> Self {
        if testnet11 {
            return CatalogRegistryConstants {
                launcher_id: Bytes32::from(hex!(
                    "0b705afb0d848794311970de0cb98722468fad6c8f687337735ab9e5286d7704"
                )),
                royalty_address: Bytes32::from(hex!(
                    "b3aea098428b2b5e6d57cf3bff6ee82e3950dec338b17df6d8ee20944787def5"
                )),
                royalty_basis_points: 100,
                precommit_payout_puzzle_hash: Bytes32::from(hex!(
                    "b3aea098428b2b5e6d57cf3bff6ee82e3950dec338b17df6d8ee20944787def5"
                )),
                relative_block_height: 4,
                price_singleton_launcher_id: Bytes32::from(hex!(
                    "45dff01375d9bd681d36a3a186ab3d0c86eb809d7f85fff950f0b37f068ec664"
                )),
            };
        }

        todo!("oops - catalog constants for mainnet are not yet available");
    }

    pub fn with_price_singleton(mut self, price_singleton_launcher_id: Bytes32) -> Self {
        self.price_singleton_launcher_id = price_singleton_launcher_id;
        self
    }

    pub fn with_launcher_id(mut self, launcher_id: Bytes32) -> Self {
        self.launcher_id = launcher_id;
        self
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct CatalogRegistryInfo {
    pub state: CatalogRegistryState,

    pub constants: CatalogRegistryConstants,
}

impl CatalogRegistryInfo {
    pub fn new(state: CatalogRegistryState, constants: CatalogRegistryConstants) -> Self {
        Self { state, constants }
    }

    pub fn with_state(mut self, state: CatalogRegistryState) -> Self {
        self.state = state;
        self
    }

    pub fn action_puzzle_hashes(constants: &CatalogRegistryConstants) -> [Bytes32; 3] {
        [
            CatalogRegisterAction::from_constants(constants)
                .tree_hash()
                .into(),
            CatalogRefundAction::from_constants(constants)
                .tree_hash()
                .into(),
            <DelegatedStateAction as Action<CatalogRegistry>>::from_constants(constants)
                .tree_hash()
                .into(),
        ]
    }

    #[must_use]
    pub fn into_layers(self) -> CatalogRegistryLayers {
        SingletonLayer::new(
            self.constants.launcher_id,
            ActionLayer::from_action_puzzle_hashes(
                &Self::action_puzzle_hashes(&self.constants),
                self.state,
                Finalizer::Default {
                    hint: self.constants.launcher_id,
                },
            ),
        )
    }

    pub fn parse(
        allocator: &mut Allocator,
        puzzle: Puzzle,
        constants: CatalogRegistryConstants,
    ) -> Result<Option<Self>, DriverError> {
        let Some(layers) = CatalogRegistryLayers::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let action_puzzle_hashes = Self::action_puzzle_hashes(&constants);
        let merkle_root = MerkleTree::new(&action_puzzle_hashes).root();
        if layers.inner_puzzle.merkle_root != merkle_root {
            return Ok(None);
        }

        Ok(Some(Self::from_layers(layers, constants)))
    }

    pub fn from_layers(layers: CatalogRegistryLayers, constants: CatalogRegistryConstants) -> Self {
        Self {
            state: layers.inner_puzzle.state,
            constants,
        }
    }

    pub fn puzzle_hash(&self) -> TreeHash {
        SingletonArgs::curry_tree_hash(self.constants.launcher_id, self.inner_puzzle_hash())
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        ActionLayerArgs::curry_tree_hash(
            DefaultFinalizer2ndCurryArgs::curry_tree_hash(self.constants.launcher_id),
            MerkleTree::new(&Self::action_puzzle_hashes(&self.constants)).root(),
            self.state.tree_hash(),
        )
    }
}
