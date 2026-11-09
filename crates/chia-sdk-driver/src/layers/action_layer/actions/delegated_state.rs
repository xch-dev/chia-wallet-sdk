use chia_protocol::{Bytes32, Coin};
use chia_sdk_types::{
    Conditions,
    puzzles::{DelegatedStateActionArgs, DelegatedStateActionSolution},
};
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    CatalogRegistry, CatalogRegistryConstants, DriverError, SingletonAction, Spend, SpendContext,
    XchandlesConstants, XchandlesRegistry, XchandlesRegistryReceivedMessagePrefix,
};

/// An action that replaces the whole state of an action layer singleton with a state sent by
/// another singleton.
///
/// The `CATalog` and `XCHandles` registries use it to let their price singleton, such as a
/// [`StateScheduler`](crate::StateScheduler), update registration prices. The other singleton
/// must send a message with [`XchandlesRegistryReceivedMessagePrefix::update_state`] of the new
/// state's tree hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegatedStateAction {
    /// The launcher id of the singleton allowed to set the state.
    pub other_launcher_id: Bytes32,
}

impl ToTreeHash for DelegatedStateAction {
    fn tree_hash(&self) -> TreeHash {
        DelegatedStateActionArgs::curry_tree_hash(self.other_launcher_id)
    }
}

impl SingletonAction<CatalogRegistry> for DelegatedStateAction {
    fn from_constants(constants: &CatalogRegistryConstants) -> Self {
        Self {
            other_launcher_id: constants.price_singleton_launcher_id,
        }
    }
}

impl SingletonAction<XchandlesRegistry> for DelegatedStateAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            other_launcher_id: constants.price_singleton_launcher_id,
        }
    }
}

impl DelegatedStateAction {
    /// Computes the puzzle hash of this action for the given price singleton.
    pub fn curry_tree_hash(price_singleton_launcher_id: Bytes32) -> TreeHash {
        DelegatedStateActionArgs::curry_tree_hash(price_singleton_launcher_id)
    }

    /// Constructs the action puzzle.
    ///
    /// # Errors
    ///
    /// Returns an error if the puzzle can't be allocated.
    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(DelegatedStateActionArgs::new(self.other_launcher_id))
    }

    /// Creates the action spend that sets the state of the singleton coin `my_coin` to
    /// `new_state`.
    ///
    /// Returns the conditions that the other singleton must output, which send the update
    /// message to `my_coin`'s puzzle hash, and the action [`Spend`]. Unlike most actions, the spend
    /// isn't added to a registry, so the caller has to insert it, for example with
    /// [`CatalogRegistry::insert_action_spend`].
    /// `other_singleton_inner_puzzle_hash` is the other singleton's current inner puzzle hash.
    ///
    /// # Errors
    ///
    /// Returns an error if the state or puzzle can't be allocated.
    pub fn spend<S>(
        self,
        ctx: &mut SpendContext,
        my_coin: Coin,
        new_state: S,
        other_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Spend), DriverError>
    where
        S: ToClvm<Allocator>,
    {
        let state = new_state.to_clvm(ctx)?;
        let my_solution = DelegatedStateActionSolution::<NodePtr> {
            new_state: state,
            other_singleton_inner_puzzle_hash,
        }
        .to_clvm(ctx)?;
        let my_puzzle = self.construct_puzzle(ctx)?;

        let conds = Conditions::new().send_message(
            18,
            XchandlesRegistryReceivedMessagePrefix::update_state(ctx.tree_hash(state)).into(),
            vec![ctx.alloc(&my_coin.puzzle_hash)?],
        );
        Ok((conds, Spend::new(my_puzzle, my_solution)))
    }
}
