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

use crate::{
    Action, ActionLayer, ActionLayerArgs, DefaultFinalizer2ndCurryArgs, DelegatedStateAction,
    Finalizer, XchandlesExpireAction, XchandlesExponentialPremiumRenewPuzzleArgs,
    XchandlesExtendAction, XchandlesFactorPricingPuzzleArgs, XchandlesOracleAction,
    XchandlesRefundAction, XchandlesRegisterAction, XchandlesUpdateAction,
};

use super::{DefaultCatMakerArgs, XchandlesRegistry};

pub type XchandlesRegistryLayers = SingletonLayer<ActionLayer<XchandlesRegistryState>>;

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct XchandlesRegistryState {
    pub cat_maker_puzzle_hash: Bytes32,
    pub pricing_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub expired_handle_pricing_puzzle_hash: Bytes32,
}

impl XchandlesRegistryState {
    pub fn from(
        payment_cat_tail_hash_hash: Bytes32,
        base_price: u64,
        registration_period: u64,
    ) -> Self {
        Self {
            cat_maker_puzzle_hash: DefaultCatMakerArgs::curry_tree_hash(payment_cat_tail_hash_hash)
                .into(),
            pricing_puzzle_hash: XchandlesFactorPricingPuzzleArgs::curry_tree_hash(
                base_price,
                registration_period,
            )
            .into(),
            expired_handle_pricing_puzzle_hash:
                XchandlesExponentialPremiumRenewPuzzleArgs::curry_tree_hash(
                    base_price,
                    registration_period,
                    1000,
                )
                .into(),
        }
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy, ToClvm, FromClvm)]
#[clvm(list)]
pub struct XchandlesConstants {
    pub launcher_id: Bytes32,
    pub precommit_payout_puzzle_hash: Bytes32,
    pub relative_block_height: u32,
    pub price_singleton_launcher_id: Bytes32,
}

impl XchandlesConstants {
    pub fn new(
        launcher_id: Bytes32,
        precommit_payout_puzzle_hash: Bytes32,
        relative_block_height: u32,
        price_singleton_launcher_id: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            precommit_payout_puzzle_hash,
            relative_block_height,
            price_singleton_launcher_id,
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

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct XchandlesRegistryInfo {
    pub state: XchandlesRegistryState,

    pub constants: XchandlesConstants,
}

impl XchandlesRegistryInfo {
    pub fn new(state: XchandlesRegistryState, constants: XchandlesConstants) -> Self {
        Self { state, constants }
    }

    pub fn with_state(mut self, state: XchandlesRegistryState) -> Self {
        self.state = state;
        self
    }

    pub fn action_puzzle_hashes(constants: &XchandlesConstants) -> [Bytes32; 7] {
        [
            XchandlesExpireAction::from_constants(constants)
                .tree_hash()
                .into(),
            XchandlesExtendAction::from_constants(constants)
                .tree_hash()
                .into(),
            XchandlesOracleAction::from_constants(constants)
                .tree_hash()
                .into(),
            XchandlesRegisterAction::from_constants(constants)
                .tree_hash()
                .into(),
            XchandlesUpdateAction::from_constants(constants)
                .tree_hash()
                .into(),
            XchandlesRefundAction::from_constants(constants)
                .tree_hash()
                .into(),
            <DelegatedStateAction as Action<XchandlesRegistry>>::from_constants(constants)
                .tree_hash()
                .into(),
        ]
    }

    #[must_use]
    pub fn into_layers(self) -> XchandlesRegistryLayers {
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
        constants: XchandlesConstants,
    ) -> Result<Option<Self>, DriverError> {
        let Some(layers) = XchandlesRegistryLayers::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let action_puzzle_hashes = Self::action_puzzle_hashes(&constants);
        let merkle_root = MerkleTree::new(&action_puzzle_hashes).root();
        if layers.inner_puzzle.merkle_root != merkle_root {
            return Ok(None);
        }

        Ok(Some(Self::from_layers(layers, constants)))
    }

    pub fn from_layers(layers: XchandlesRegistryLayers, constants: XchandlesConstants) -> Self {
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
