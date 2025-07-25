pub trait ActionSingleton {
    type State;
    type Constants;
}

pub trait SingletonAction<AS: ActionSingleton> {
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
