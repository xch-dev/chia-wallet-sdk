/// A singleton whose inner puzzle is an [`ActionLayer`](crate::ActionLayer), such as
/// [`CatalogRegistry`](crate::CatalogRegistry), [`XchandlesRegistry`](crate::XchandlesRegistry),
/// or [`RewardDistributor`](crate::RewardDistributor).
pub trait ActionSingleton {
    /// The state stored in the action layer.
    type State;
    /// The constants the singleton's action puzzles are curried with.
    type Constants;
}

/// An action that can be run by the [`ActionSingleton`] `AS`.
///
/// Actions usually also implement [`ToTreeHash`](clvm_utils::ToTreeHash), which returns the
/// action's puzzle hash: a leaf of the action layer's merkle tree.
pub trait SingletonAction<AS: ActionSingleton> {
    /// Creates the action from the singleton's constants.
    fn from_constants(constants: &AS::Constants) -> Self;

    // you may also add:

    // fn curry_tree_hash(constants: &AS::Constants) -> TreeHash;

    // fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>;

    // fn spend(
    //     self,
    //     ctx: &mut SpendContext,
    //     action_singleton: &Self::ActionSingleton,
    //     params: &Self::SpendParams,
    // ) -> Result<(Option<Conditions>, Spend, Self::SpendReturnParams), DriverError>;

    // and a function to return the slots this action creates
}
