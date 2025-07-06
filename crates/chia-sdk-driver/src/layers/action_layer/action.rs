pub trait Registry {
    type State;
    type Constants;
}

pub trait Action<R: Registry> {
    fn from_constants(constants: &R::Constants) -> Self;

    // you may also add:

    // fn curry_tree_hash(constants: &R::Constants) -> TreeHash;

    // fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>;

    // fn spend(
    //     self,
    //     ctx: &mut SpendContext,
    //     registry: &Self::Registry,
    //     params: &Self::SpendParams,
    // ) -> Result<(Option<Conditions>, Spend, Self::SpendReturnParams), DriverError>;

    // and a function to return the slots this action creates
}
