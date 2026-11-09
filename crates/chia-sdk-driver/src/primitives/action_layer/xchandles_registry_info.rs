use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::{
    MerkleTree, Mod,
    puzzles::{
        ActionLayerArgs, DefaultCatMakerArgs, DefaultFinalizer2ndCurryArgs,
        XchandlesFactorPricingPuzzleArgs,
    },
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{
    ActionLayer, DelegatedStateAction, DriverError, Finalizer, Layer, Puzzle, SingletonAction,
    SingletonLayer, XchandlesExecuteUpdateAction, XchandlesExpireAction,
    XchandlesExpirePricingPuzzle, XchandlesExtendAction, XchandlesInitiateUpdateAction,
    XchandlesOracleAction, XchandlesRefundAction, XchandlesRegisterAction,
};

use super::XchandlesRegistry;

/// The layers of an [`XchandlesRegistry`] puzzle: a singleton around an [`ActionLayer`].
pub type XchandlesRegistryLayers = SingletonLayer<ActionLayer<XchandlesRegistryState>>;

/// The mutable state of an [`XchandlesRegistry`], stored in its action layer.
///
/// The price singleton can change the state with a [`DelegatedStateAction`].
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct XchandlesRegistryState {
    /// The puzzle hash of the [`CatMaker`](crate::CatMaker) for the CAT that registrations are paid in.
    pub cat_maker_puzzle_hash: Bytes32,
    /// The puzzle hash of the pricing puzzle for registering and extending handles.
    pub pricing_puzzle_hash: Bytes32,
    /// The puzzle hash of the pricing puzzle for re-registering expired handles.
    #[clvm(rest)]
    pub expired_handle_pricing_puzzle_hash: Bytes32,
}

impl XchandlesRegistryState {
    /// Creates the state for the default CAT maker and pricing puzzles.
    ///
    /// `payment_cat_tail_hash_hash` is the tree hash of the payment CAT's asset id. The pricing
    /// puzzle is the factor pricing puzzle with `base_price` and `registration_period` (in seconds),
    /// and expired handles use [`XchandlesExpirePricingPuzzle`] with the same parameters.
    pub fn from(
        payment_cat_tail_hash_hash: Bytes32,
        base_price: u64,
        registration_period: u64,
    ) -> Self {
        Self {
            cat_maker_puzzle_hash: DefaultCatMakerArgs::new(payment_cat_tail_hash_hash)
                .curry_tree_hash()
                .into(),
            pricing_puzzle_hash: XchandlesFactorPricingPuzzleArgs {
                base_price,
                registration_period,
            }
            .curry_tree_hash()
            .into(),
            expired_handle_pricing_puzzle_hash: XchandlesExpirePricingPuzzle::curry_tree_hash(
                base_price,
                registration_period,
            )
            .into(),
        }
    }
}

/// The values that are fixed for the lifetime of an [`XchandlesRegistry`] and curried into its actions.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy, ToClvm, FromClvm)]
#[clvm(list)]
pub struct XchandlesConstants {
    /// The launcher id of the registry singleton.
    pub launcher_id: Bytes32,
    /// Where the payments of [`PrecommitCoin`](crate::PrecommitCoin)s go when they are used.
    pub precommit_payout_puzzle_hash: Bytes32,
    /// The number of blocks a precommit coin must wait before it can be used.
    pub relative_block_height: u32,
    /// The launcher id of the singleton that may update the state with a [`DelegatedStateAction`].
    pub price_singleton_launcher_id: Bytes32,
}

impl XchandlesConstants {
    /// Creates a new [`XchandlesConstants`].
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

/// The information needed to construct the puzzle of an [`XchandlesRegistry`].
///
/// The registry's actions are the expire, extend, oracle, register, initiate update, execute
/// update, and refund actions (such as [`XchandlesRegisterAction`]), plus a
/// [`DelegatedStateAction`]. It uses the default finalizer, which recreates the singleton with the
/// new state.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{
///     Layer, SpendContext, XchandlesConstants, XchandlesRegistryInfo, XchandlesRegistryState,
/// };
/// use clvm_utils::ToTreeHash;
///
/// let ctx = &mut SpendContext::new();
///
/// let payment_asset_id = Bytes32::new([1; 32]);
/// let state = XchandlesRegistryState::from(
///     payment_asset_id.tree_hash().into(),
///     100,
///     366 * 24 * 60 * 60,
/// );
/// let constants = XchandlesConstants::new(
///     Bytes32::new([2; 32]),
///     Bytes32::new([3; 32]),
///     4,
///     Bytes32::new([4; 32]),
/// );
/// let info = XchandlesRegistryInfo::new(state, constants);
///
/// let puzzle = info.into_layers().construct_puzzle(ctx)?;
/// assert_eq!(ctx.tree_hash(puzzle), info.puzzle_hash());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct XchandlesRegistryInfo {
    /// The current state of the registry.
    pub state: XchandlesRegistryState,

    /// The constants of the registry.
    pub constants: XchandlesConstants,
}

impl XchandlesRegistryInfo {
    /// Creates a new [`XchandlesRegistryInfo`].
    pub fn new(state: XchandlesRegistryState, constants: XchandlesConstants) -> Self {
        Self { state, constants }
    }

    /// Returns a copy of the info with a different state.
    pub fn with_state(mut self, state: XchandlesRegistryState) -> Self {
        self.state = state;
        self
    }

    /// Returns the puzzle hashes of the registry's actions, in the order used for the Merkle tree.
    pub fn action_puzzle_hashes(constants: &XchandlesConstants) -> [Bytes32; 8] {
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
            XchandlesInitiateUpdateAction::from_constants(constants)
                .tree_hash()
                .into(),
            XchandlesExecuteUpdateAction::from_constants(constants)
                .tree_hash()
                .into(),
            XchandlesRefundAction::from_constants(constants)
                .tree_hash()
                .into(),
            <DelegatedStateAction as SingletonAction<XchandlesRegistry>>::from_constants(constants)
                .tree_hash()
                .into(),
        ]
    }

    /// Returns the layers of the registry puzzle.
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

    /// Parses a registry puzzle with the given constants.
    ///
    /// Returns [`None`] if the puzzle isn't an action-layer singleton or its actions don't match the constants.
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

        Ok(Some(Self::from_layers(&layers, constants)))
    }

    /// Creates the info from parsed layers, without checking that they match the constants.
    pub fn from_layers(layers: &XchandlesRegistryLayers, constants: XchandlesConstants) -> Self {
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
