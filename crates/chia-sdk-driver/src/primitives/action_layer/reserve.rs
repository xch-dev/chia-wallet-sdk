use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{LineageProof, cat::CatArgs, singleton::SingletonSolution};
use chia_sdk_types::{
    conditions::CreateCoin,
    puzzles::{P2DelegatedBySingletonLayerArgs, P2DelegatedBySingletonLayerSolution},
    run_puzzle,
};
use clvm_traits::{FromClvm, ToClvm, clvm_quote, clvm_tuple, match_tuple};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{
    ActionLayer, Cat, CatInfo, CatSpend, DriverError, Layer, P2DelegatedBySingletonLayer, Spend,
    SpendContext,
};

/// A CAT that holds funds on behalf of a controller singleton.
///
/// The inner puzzle is a [`P2DelegatedBySingletonLayer`]: the reserve runs any delegated puzzle
/// whose tree hash the controller singleton sends it as a message in the same block. The
/// [`RewardDistributor`](crate::RewardDistributor) keeps its rewards in a reserve and uses the
/// reserve finalizer to spend it, so the reserve's amount always matches the distributor state.
///
/// The `nonce` is curried into the inner puzzle so a controller can have several distinct reserves.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_puzzle_types::{LineageProof, singleton::SingletonStruct};
/// use chia_sdk_driver::Reserve;
/// use clvm_utils::ToTreeHash;
///
/// let asset_id = Bytes32::new([1; 32]);
/// let controller_singleton_struct_hash: Bytes32 =
///     SingletonStruct::new(Bytes32::new([2; 32])).tree_hash().into();
///
/// let reserve = Reserve::new(
///     Bytes32::new([3; 32]),
///     LineageProof {
///         parent_parent_coin_info: Bytes32::default(),
///         parent_inner_puzzle_hash: Bytes32::default(),
///         parent_amount: 1000,
///     },
///     asset_id,
///     controller_singleton_struct_hash,
///     0,
///     1000,
/// );
/// assert_eq!(
///     reserve.coin.puzzle_hash,
///     Reserve::puzzle_hash(asset_id, controller_singleton_struct_hash, 0).into()
/// );
///
/// // Spending the reserve recreates it with a new amount.
/// let child = reserve.child(750);
/// assert_eq!(child.coin.parent_coin_info, reserve.coin.coin_id());
/// assert_eq!(child.coin.puzzle_hash, reserve.coin.puzzle_hash);
/// assert_eq!(child.coin.amount, 750);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Reserve {
    /// The reserve CAT coin.
    pub coin: Coin,
    /// The asset id of the reserve CAT.
    pub asset_id: Bytes32,
    /// The CAT lineage proof of the coin.
    pub proof: LineageProof,
    /// The inner puzzle hash of the CAT, which is the [`P2DelegatedBySingletonLayer`] puzzle hash.
    pub inner_puzzle_hash: Bytes32,

    /// The singleton struct hash of the controller singleton.
    pub controller_singleton_struct_hash: Bytes32,
    /// A nonce that distinguishes multiple reserves of the same controller.
    pub nonce: u64,
}

/// A state type that determines how much a controller's reserves should hold.
///
/// Used by [`Reserve::delegated_puzzle_for_finalizer_controller`] to compute the amount of the
/// recreated reserve after the controller's actions have run.
pub trait Reserveful {
    /// Returns the amount that the reserve with the given index should hold in this state.
    fn reserve_amount(&self, index: u64) -> u64;
}

impl Reserve {
    /// Creates the reserve coin with the given parent, CAT lineage proof, controller, nonce, and amount.
    pub fn new(
        parent_coin_id: Bytes32,
        proof: LineageProof,
        asset_id: Bytes32,
        controller_singleton_struct_hash: Bytes32,
        nonce: u64,
        amount: u64,
    ) -> Self {
        let inner_puzzle_hash = P2DelegatedBySingletonLayerArgs::curry_tree_hash(
            controller_singleton_struct_hash,
            nonce,
        );

        Self {
            coin: Coin::new(
                parent_coin_id,
                CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash).into(),
                amount,
            ),
            proof,
            asset_id,
            inner_puzzle_hash: inner_puzzle_hash.into(),
            controller_singleton_struct_hash,
            nonce,
        }
    }

    /// Returns the full CAT puzzle hash of a reserve with the given parameters.
    pub fn puzzle_hash(
        asset_id: Bytes32,
        controller_singleton_struct_hash: Bytes32,
        nonce: u64,
    ) -> TreeHash {
        CatArgs::curry_tree_hash(
            asset_id,
            P2DelegatedBySingletonLayerArgs::curry_tree_hash(
                controller_singleton_struct_hash,
                nonce,
            ),
        )
    }

    /// Allocates the [`P2DelegatedBySingletonLayer`] inner puzzle.
    pub fn construct_inner_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let layer =
            P2DelegatedBySingletonLayer::new(self.controller_singleton_struct_hash, self.nonce);

        layer.construct_puzzle(ctx)
    }

    /// Returns the reserve as a (non-revocable) [`Cat`].
    pub fn to_cat(&self) -> Cat {
        Cat::new(
            self.coin,
            Some(self.proof),
            CatInfo::new(self.asset_id, None, self.inner_puzzle_hash),
        )
    }

    /// Builds the inner spend that runs `delegated_puzzle` with `delegated_solution`.
    ///
    /// The controller singleton, whose current inner puzzle hash is
    /// `controller_singleton_inner_puzzle_hash`, must send the tree hash of `delegated_puzzle` to
    /// the reserve as a message in the same block.
    pub fn inner_spend(
        &self,
        ctx: &mut SpendContext,
        controller_singleton_inner_puzzle_hash: Bytes32,
        delegated_puzzle: NodePtr,
        delegated_solution: NodePtr,
    ) -> Result<Spend, DriverError> {
        P2DelegatedBySingletonLayer::new(self.controller_singleton_struct_hash, self.nonce)
            .construct_spend(
                ctx,
                P2DelegatedBySingletonLayerSolution {
                    singleton_inner_puzzle_hash: controller_singleton_inner_puzzle_hash,
                    delegated_puzzle,
                    delegated_solution,
                },
            )
    }

    /// Builds the delegated puzzle that a controller using the reserve finalizer expects.
    ///
    /// `controller_solution` is the controller's full singleton solution. Its actions are run on top
    /// of `controller_initial_state` to find the final state. Every `(-42 . condition)` they output
    /// adds `condition` to the reserve's conditions. The returned puzzle quotes a `CREATE_COIN`
    /// that recreates the reserve with [`Reserveful::reserve_amount`] of the final state (index 0),
    /// followed by those conditions, just like the reserve finalizer does.
    pub fn delegated_puzzle_for_finalizer_controller<S>(
        &self,
        ctx: &mut SpendContext,
        controller_initial_state: S,
        controller_solution: NodePtr,
    ) -> Result<NodePtr, DriverError>
    where
        S: ToClvm<Allocator> + FromClvm<Allocator> + Clone + Reserveful,
    {
        let controller_solution = ctx.extract::<SingletonSolution<NodePtr>>(controller_solution)?;
        let inner_solution =
            ActionLayer::<S, NodePtr>::parse_solution(ctx, controller_solution.inner_solution)?;

        let mut state = (NodePtr::NIL, controller_initial_state);
        let mut reserve_conditions = Vec::new();
        for raw_action in inner_solution.action_spends {
            let actual_solution = ctx.alloc(&clvm_tuple!(state, raw_action.solution))?;

            let output = run_puzzle(ctx, raw_action.puzzle, actual_solution)?;

            let (new_state, conditions) =
                ctx.extract::<match_tuple!((NodePtr, S), Vec<(i64, NodePtr)>)>(output)?;
            state = new_state;

            for (opcode, cond) in conditions.iter().rev() {
                if *opcode == -42 {
                    reserve_conditions.push(*cond);
                }
            }
        }

        // prepend CREATE_COIN, just like the reserve finalizer does
        // (list CREATE_COIN RESERVE_INNER_PUZZLE_HASH (f New_State) (list RESERVE_INNER_PUZZLE_HASH))
        let new_reserve_amount = state.1.reserve_amount(0);
        let cc = CreateCoin::new(
            self.inner_puzzle_hash,
            new_reserve_amount,
            ctx.hint(self.inner_puzzle_hash)?,
        );
        reserve_conditions.insert(0, ctx.alloc(&cc)?);

        let delegated_puzzle = ctx.alloc(&clvm_quote!(reserve_conditions))?;

        Ok(delegated_puzzle)
    }

    /// Builds the [`CatSpend`] of the reserve for a controller spend that uses the reserve finalizer.
    ///
    /// See [`Reserve::delegated_puzzle_for_finalizer_controller`] for how the delegated puzzle is computed.
    pub fn cat_spend_for_reserve_finalizer_controller<S>(
        &self,
        ctx: &mut SpendContext,
        controller_initial_state: S,
        controller_singleton_inner_puzzle_hash: Bytes32,
        controller_solution: NodePtr,
    ) -> Result<CatSpend, DriverError>
    where
        S: ToClvm<Allocator> + FromClvm<Allocator> + Clone + Reserveful,
    {
        let delegated_puzzle = self.delegated_puzzle_for_finalizer_controller(
            ctx,
            controller_initial_state,
            controller_solution,
        )?;

        Ok(CatSpend::new(
            self.to_cat(),
            self.inner_spend(
                ctx,
                controller_singleton_inner_puzzle_hash,
                delegated_puzzle,
                NodePtr::NIL,
            )?,
        ))
    }

    /// Returns the reserve coin created by spending this one, with the given amount.
    pub fn child(&self, child_amount: u64) -> Self {
        Self::new(
            self.coin.coin_id(),
            self.child_lineage_proof(),
            self.asset_id,
            self.controller_singleton_struct_hash,
            self.nonce,
            child_amount,
        )
    }

    /// Returns the lineage proof for a child of this reserve.
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.inner_puzzle_hash,
            parent_amount: self.coin.amount,
        }
    }
}
