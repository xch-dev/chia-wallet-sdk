use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::{
    MerkleTree,
    puzzles::{ActionLayerArgs, DefaultFinalizer2ndCurryArgs},
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{
    ActionLayer, CatalogRefundAction, CatalogRegisterAction, DelegatedStateAction, DriverError,
    Finalizer, Layer, Puzzle, SingletonAction, SingletonLayer,
};

use super::CatalogRegistry;

/// The layers of a [`CatalogRegistry`] puzzle: a singleton around an [`ActionLayer`].
pub type CatalogRegistryLayers = SingletonLayer<ActionLayer<CatalogRegistryState>>;

/// The mutable state of a [`CatalogRegistry`], stored in its action layer.
///
/// The price singleton can change the state with a [`DelegatedStateAction`].
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct CatalogRegistryState {
    /// The puzzle hash of the [`CatMaker`](crate::CatMaker) for the CAT that registrations are paid in.
    pub cat_maker_puzzle_hash: Bytes32,
    /// The amount of the payment CAT that a registration costs.
    #[clvm(rest)]
    pub registration_price: u64,
}

/// The values that are fixed for the lifetime of a [`CatalogRegistry`] and curried into its actions.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct CatalogRegistryConstants {
    /// The launcher id of the registry singleton.
    pub launcher_id: Bytes32,
    /// The royalty puzzle hash of the NFTs minted for registered CATs.
    pub royalty_address: Bytes32,
    /// The royalty of the minted NFTs, in basis points.
    pub royalty_basis_points: u16,
    /// Where the payments of [`PrecommitCoin`](crate::PrecommitCoin)s go when they are used for a registration.
    pub precommit_payout_puzzle_hash: Bytes32,
    /// The number of blocks a precommit coin must wait before it can be used.
    pub relative_block_height: u32,
    /// The launcher id of the singleton that may update the state with a [`DelegatedStateAction`].
    pub price_singleton_launcher_id: Bytes32,
}

impl CatalogRegistryConstants {
    /// Returns the constants of the official `CATalog` deployment on mainnet or testnet11.
    ///
    /// # Panics
    ///
    /// Always panics for now, because the official constants are not yet available.
    pub fn get(testnet11: bool) -> Self {
        let _ = testnet11;
        // CatalogRegistryConstants {
        //     launcher_id: Bytes32::default(),
        //     royalty_address: Bytes32::from(hex!(
        //         "764e9d674d2fa441f0f6f8fc5e749a17dde345ebe4a33536afd3ef417a3f8c90"
        //     )),
        //     royalty_basis_points: 100,
        //     precommit_payout_puzzle_hash: Bytes32::from(hex!(
        //         "764e9d674d2fa441f0f6f8fc5e749a17dde345ebe4a33536afd3ef417a3f8c90"
        //     )),
        //     relative_block_height: 4,
        //     price_singleton_launcher_id: Bytes32::default(),
        // }
        todo!("CATalog constants not yet available.");
    }

    /// Returns a copy of the constants with a different price singleton launcher id.
    pub fn with_price_singleton(mut self, price_singleton_launcher_id: Bytes32) -> Self {
        self.price_singleton_launcher_id = price_singleton_launcher_id;
        self
    }

    /// Returns a copy of the constants with a different registry launcher id.
    pub fn with_launcher_id(mut self, launcher_id: Bytes32) -> Self {
        self.launcher_id = launcher_id;
        self
    }
}

/// The information needed to construct the puzzle of a [`CatalogRegistry`].
///
/// The registry's actions are [`CatalogRegisterAction`], [`CatalogRefundAction`], and
/// [`DelegatedStateAction`]. It uses the default finalizer, which recreates the singleton with
/// the new state.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{
///     CatMaker, CatalogRegistryConstants, CatalogRegistryInfo, CatalogRegistryState, Layer,
///     SpendContext,
/// };
/// use clvm_utils::ToTreeHash;
///
/// let ctx = &mut SpendContext::new();
///
/// let payment_asset_id = Bytes32::new([1; 32]);
/// let state = CatalogRegistryState {
///     cat_maker_puzzle_hash: CatMaker::Default {
///         tail_hash_hash: payment_asset_id.tree_hash(),
///     }
///     .curry_tree_hash()
///     .into(),
///     registration_price: 100,
/// };
/// let constants = CatalogRegistryConstants {
///     launcher_id: Bytes32::new([2; 32]),
///     royalty_address: Bytes32::new([3; 32]),
///     royalty_basis_points: 100,
///     precommit_payout_puzzle_hash: Bytes32::new([4; 32]),
///     relative_block_height: 4,
///     price_singleton_launcher_id: Bytes32::new([5; 32]),
/// };
/// let info = CatalogRegistryInfo::new(state, constants);
///
/// let puzzle = info.into_layers().construct_puzzle(ctx)?;
/// assert_eq!(ctx.tree_hash(puzzle), info.puzzle_hash());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct CatalogRegistryInfo {
    /// The current state of the registry.
    pub state: CatalogRegistryState,

    /// The constants of the registry.
    pub constants: CatalogRegistryConstants,
}

impl CatalogRegistryInfo {
    /// Creates a new [`CatalogRegistryInfo`].
    pub fn new(state: CatalogRegistryState, constants: CatalogRegistryConstants) -> Self {
        Self { state, constants }
    }

    /// Returns a copy of the info with a different state.
    pub fn with_state(mut self, state: CatalogRegistryState) -> Self {
        self.state = state;
        self
    }

    /// Returns the puzzle hashes of the registry's actions, in the order used for the Merkle tree.
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

    /// Returns the layers of the registry puzzle.
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

    /// Parses a registry puzzle with the given constants.
    ///
    /// Returns [`None`] if the puzzle isn't an action-layer singleton or its actions don't match the constants.
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

    /// Creates the info from parsed layers, without checking that they match the constants.
    pub fn from_layers(
        layers: &CatalogRegistryLayers,
        constants: CatalogRegistryConstants,
    ) -> Self {
        Self {
            state: layers.inner_puzzle.state,
            constants,
        }
    }

    /// Returns the full singleton puzzle hash of the registry.
    pub fn puzzle_hash(&self) -> TreeHash {
        SingletonArgs::curry_tree_hash(self.constants.launcher_id, self.inner_puzzle_hash())
    }

    /// Returns the inner (action layer) puzzle hash of the registry.
    pub fn inner_puzzle_hash(&self) -> TreeHash {
        ActionLayerArgs::curry_tree_hash(
            DefaultFinalizer2ndCurryArgs::curry_tree_hash(self.constants.launcher_id),
            MerkleTree::new(&Self::action_puzzle_hashes(&self.constants)).root(),
            self.state.tree_hash(),
        )
    }
}
