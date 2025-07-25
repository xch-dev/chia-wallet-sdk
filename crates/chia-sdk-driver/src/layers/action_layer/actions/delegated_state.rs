use chia_protocol::{Bytes32, Coin};
use chia_sdk_types::{
    puzzles::{DelegatedStateActionArgs, DelegatedStateActionSolution},
    Conditions,
};
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    CatalogRegistry, CatalogRegistryConstants, DriverError, SingletonAction, Spend, SpendContext,
    XchandlesConstants, XchandlesRegistry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegatedStateAction {
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
    pub fn curry_tree_hash(price_singleton_launcher_id: Bytes32) -> TreeHash {
        DelegatedStateActionArgs::curry_tree_hash(price_singleton_launcher_id)
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(DelegatedStateActionArgs::new(self.other_launcher_id))
    }

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

        let message: Bytes32 = ctx.tree_hash(state).into();
        let conds = Conditions::new().send_message(
            18,
            message.into(),
            vec![ctx.alloc(&my_coin.puzzle_hash)?],
        );
        Ok((conds, Spend::new(my_puzzle, my_solution)))
    }
}
