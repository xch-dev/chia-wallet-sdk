use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    CoinProof, LineageProof,
    cat::{CatArgs, CatSolution},
};
use chia_sdk_types::puzzles::{PrecommitLayerSolution, PrecommitSpendMode};
use clvm_traits::ToClvm;
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, PrecommitLayer, Spend, SpendContext};

/// A CAT that commits to a value and a payment before a registration in an action-layer app.
///
/// Registries such as [`CatalogRegistry`](crate::CatalogRegistry) and
/// [`XchandlesRegistry`](crate::XchandlesRegistry) use a two-step registration: the user first
/// creates a precommit coin, then the registry spends it once it is old enough. Since the
/// value (for example a handle and a secret) is only revealed at registration, the registration
/// can't be front-run.
///
/// The inner puzzle is a [`PrecommitLayer`]. A spend:
///
/// - asserts that the coin is at least `relative_block_height` blocks old,
/// - requires a message from the controller singleton containing the [`PrecommitSpendMode`], and
/// - sends the full amount to `payout_puzzle_hash` ([`PrecommitSpendMode::REGISTER`]) or
///   `refund_puzzle_hash` ([`PrecommitSpendMode::REFUND`]).
///
/// The value is curried into the puzzle but isn't used by it, so only its hash needs to be known
/// to compute the puzzle hash.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_puzzle_types::LineageProof;
/// use chia_sdk_driver::{PrecommitCoin, SpendContext};
/// use clvm_utils::ToTreeHash;
///
/// let ctx = &mut SpendContext::new();
///
/// let asset_id = Bytes32::new([1; 32]);
/// let controller_singleton_struct_hash = Bytes32::new([2; 32]);
/// let payout_puzzle_hash = Bytes32::new([3; 32]);
/// let refund_puzzle_hash = Bytes32::new([4; 32]);
/// let value = Bytes32::new([5; 32]);
///
/// let precommit = PrecommitCoin::new(
///     ctx,
///     Bytes32::new([6; 32]),
///     LineageProof {
///         parent_parent_coin_info: Bytes32::default(),
///         parent_inner_puzzle_hash: Bytes32::default(),
///         parent_amount: 100,
///     },
///     asset_id,
///     controller_singleton_struct_hash,
///     4,
///     payout_puzzle_hash,
///     refund_puzzle_hash,
///     value,
///     100,
/// )?;
///
/// assert_eq!(
///     precommit.coin.puzzle_hash,
///     PrecommitCoin::<Bytes32>::puzzle_hash(
///         asset_id,
///         controller_singleton_struct_hash,
///         4,
///         payout_puzzle_hash,
///         refund_puzzle_hash,
///         value.tree_hash(),
///     )
///     .into()
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct PrecommitCoin<V> {
    /// The precommit CAT coin.
    pub coin: Coin,
    /// The asset id of the CAT used for the payment.
    pub asset_id: Bytes32,
    /// The CAT lineage proof of the coin.
    pub proof: LineageProof,
    /// The inner puzzle hash of the CAT, which is the [`PrecommitLayer`] puzzle hash.
    pub inner_puzzle_hash: Bytes32,

    /// The singleton struct hash of the controller singleton (usually the registry).
    pub controller_singleton_struct_hash: Bytes32,
    /// The number of blocks that must pass after the coin is created before it can be spent.
    pub relative_block_height: u32,
    /// Where the payment goes when the precommitment is used for a registration.
    pub payout_puzzle_hash: Bytes32,
    /// Where the payment goes when the precommitment is refunded.
    pub refund_puzzle_hash: Bytes32,
    /// The committed value.
    pub value: V,
}

impl<V> PrecommitCoin<V> {
    /// Creates the precommit coin with the given parent, CAT lineage proof, puzzle parameters, and amount.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        proof: LineageProof,
        asset_id: Bytes32,
        controller_singleton_struct_hash: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value: V,
        precommit_amount: u64,
    ) -> Result<Self, DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let value_ptr = ctx.alloc(&value)?;
        let value_hash = ctx.tree_hash(value_ptr);

        let inner_puzzle_hash = PrecommitLayer::<V>::puzzle_hash(
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            value_hash,
        );

        Ok(Self {
            coin: Coin::new(
                parent_coin_id,
                CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash).into(),
                precommit_amount,
            ),
            proof,
            asset_id,
            inner_puzzle_hash: inner_puzzle_hash.into(),
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            value,
        })
    }

    /// Returns the full CAT puzzle hash of a precommit coin with the given parameters.
    pub fn puzzle_hash(
        asset_id: Bytes32,
        controller_singleton_struct_hash: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value_hash: TreeHash,
    ) -> TreeHash {
        CatArgs::curry_tree_hash(
            asset_id,
            PrecommitLayer::<V>::puzzle_hash(
                controller_singleton_struct_hash,
                relative_block_height,
                payout_puzzle_hash,
                refund_puzzle_hash,
                value_hash,
            ),
        )
    }

    /// Allocates the [`PrecommitLayer`] inner puzzle.
    pub fn inner_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>
    where
        V: Clone + ToClvm<Allocator>,
    {
        PrecommitLayer::<V>::new(
            self.controller_singleton_struct_hash,
            self.relative_block_height,
            self.payout_puzzle_hash,
            self.refund_puzzle_hash,
            self.value.clone(),
        )
        .construct_puzzle(ctx)
    }

    /// Allocates the full CAT puzzle.
    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>
    where
        V: Clone + ToClvm<Allocator>,
    {
        let inner_puzzle = self.inner_puzzle(ctx)?;

        CatLayer::<NodePtr>::new(self.asset_id, inner_puzzle).construct_puzzle(ctx)
    }

    /// Allocates the CAT solution for spending this coin by itself (not in a ring with other CATs).
    ///
    /// `singleton_inner_puzzle_hash` is the controller singleton's inner puzzle hash at the time
    /// of the spend.
    pub fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        mode: PrecommitSpendMode,
        singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<NodePtr, DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let layers = CatLayer::<NodePtr>::new(self.asset_id, self.inner_puzzle(ctx)?);

        let inner_puzzle_solution = ctx.alloc(&PrecommitLayerSolution {
            mode,
            my_amount: self.coin.amount,
            singleton_inner_puzzle_hash,
        })?;

        layers.construct_solution(
            ctx,
            CatSolution {
                inner_puzzle_solution,
                lineage_proof: Some(self.proof),
                prev_coin_id: self.coin.coin_id(),
                this_coin_info: self.coin,
                next_coin_proof: CoinProof {
                    parent_coin_info: self.coin.parent_coin_info,
                    inner_puzzle_hash: self.inner_puzzle_hash,
                    amount: self.coin.amount,
                },
                prev_subtotal: 0,
                extra_delta: 0,
            },
        )
    }

    /// Spends the precommit coin with the given mode.
    ///
    /// The controller singleton, whose current inner puzzle hash is `spender_inner_puzzle_hash`,
    /// must send this coin a message with the mode in the same block. Registry actions such as
    /// [`CatalogRegisterAction`](crate::CatalogRegisterAction) call this for you.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        mode: PrecommitSpendMode,
        spender_inner_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, mode, spender_inner_puzzle_hash)?;

        ctx.spend(self.coin, Spend::new(puzzle, solution))
    }
}
