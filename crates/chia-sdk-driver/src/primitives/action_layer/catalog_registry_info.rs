use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::{
    MerkleTree,
    puzzles::{ActionLayerArgs, DefaultFinalizer2ndCurryArgs},
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;
use hex_literal::hex;

use crate::{
    ActionLayer, CatalogRefundAction, CatalogRegisterAction, DelegatedStateAction, DriverError,
    Finalizer, Layer, Puzzle, SingletonAction, SingletonLayer,
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
        // Launcher IDs stay unset until a timestamp-based CATalog deployment exists.
        // Royalty / relative-height defaults remain usable for a fresh launch.
        let _ = testnet11;
        CatalogRegistryConstants {
            launcher_id: Bytes32::default(),
            royalty_address: Bytes32::from(hex!(
                "764e9d674d2fa441f0f6f8fc5e749a17dde345ebe4a33536afd3ef417a3f8c90"
            )),
            royalty_basis_points: 100,
            precommit_payout_puzzle_hash: Bytes32::from(hex!(
                "764e9d674d2fa441f0f6f8fc5e749a17dde345ebe4a33536afd3ef417a3f8c90"
            )),
            relative_block_height: 4,
            price_singleton_launcher_id: Bytes32::default(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_constants_have_no_deployed_scheduler_ids() {
        for testnet11 in [false, true] {
            let constants = CatalogRegistryConstants::get(testnet11);
            assert_eq!(constants.launcher_id, Bytes32::default());
            assert_eq!(constants.price_singleton_launcher_id, Bytes32::default());
            assert_eq!(constants.relative_block_height, 4);
        }
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
            <DelegatedStateAction as SingletonAction<CatalogRegistry>>::from_constants(constants)
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

        Ok(Some(Self::from_layers(&layers, constants)))
    }

    pub fn from_layers(
        layers: &CatalogRegistryLayers,
        constants: CatalogRegistryConstants,
    ) -> Self {
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
