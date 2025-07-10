use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::{
    puzzles::{
        AnyMetadataUpdater, CatNftMetadata, DefaultCatMakerArgs, PrecommitLayer1stCurryArgs,
        PrecommitLayer2ndCurryArgs, PrecommitLayerSolution, PRECOMMIT_LAYER_PUZZLE_HASH,
    },
    Conditions, Mod,
};
use clvm_traits::{clvm_quote, clvm_tuple, ClvmEncoder, FromClvm, ToClvm, ToClvmError};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone)]
#[must_use]
pub struct PrecommitLayer<V> {
    pub controller_singleton_struct_hash: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
    pub refund_puzzle_hash: Bytes32,
    pub value: V,
}

impl<V> PrecommitLayer<V> {
    pub fn new(
        controller_singleton_struct_hash: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value: V,
    ) -> Self {
        Self {
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            value,
        }
    }

    pub fn first_curry_hash(
        controller_singleton_struct_hash: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> TreeHash {
        PrecommitLayer1stCurryArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
        }
        .curry_tree_hash()
    }

    pub fn puzzle_hash(
        controller_singleton_struct_hash: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value_hash: TreeHash,
    ) -> TreeHash {
        CurriedProgram {
            program: Self::first_curry_hash(
                controller_singleton_struct_hash,
                relative_block_height,
                payout_puzzle_hash,
            ),
            args: PrecommitLayer2ndCurryArgs {
                refund_puzzle_hash,
                value: value_hash,
            },
        }
        .tree_hash()
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>
    where
        V: Clone + ToClvm<Allocator>,
    {
        let prog_1st_curry = ctx.curry(PrecommitLayer1stCurryArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: self.controller_singleton_struct_hash,
            relative_block_height: self.relative_block_height,
            payout_puzzle_hash: self.payout_puzzle_hash,
        })?;

        Ok(CurriedProgram {
            program: prog_1st_curry,
            args: PrecommitLayer2ndCurryArgs {
                refund_puzzle_hash: self.refund_puzzle_hash,
                value: self.value.clone(),
            },
        }
        .to_clvm(ctx)?)
    }

    pub fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: PrecommitLayerSolution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl<V> Layer for PrecommitLayer<V>
where
    V: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
{
    type Solution = PrecommitLayerSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle_2nd_curry) = puzzle.as_curried() else {
            return Ok(None);
        };

        let Some(curried) = CurriedProgram::<NodePtr, NodePtr>::parse_puzzle(allocator, puzzle)?
        else {
            return Ok(None);
        };
        let puzzle_1st_curry = Puzzle::parse(allocator, curried.program);
        let Some(puzzle_1st_curry) = puzzle_1st_curry.as_curried() else {
            return Ok(None);
        };

        if puzzle_1st_curry.mod_hash != PRECOMMIT_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args_2nd_curry =
            PrecommitLayer2ndCurryArgs::<V>::from_clvm(allocator, puzzle_2nd_curry.args)?;
        let args_1st_curry =
            PrecommitLayer1stCurryArgs::from_clvm(allocator, puzzle_1st_curry.args)?;

        Ok(Some(Self {
            controller_singleton_struct_hash: args_1st_curry.singleton_struct_hash,
            relative_block_height: args_1st_curry.relative_block_height,
            payout_puzzle_hash: args_1st_curry.payout_puzzle_hash,
            refund_puzzle_hash: args_2nd_curry.refund_puzzle_hash,
            value: args_2nd_curry.value,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        PrecommitLayerSolution::from_clvm(allocator, solution).map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        self.construct_puzzle(ctx)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        self.construct_solution(ctx, solution)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogPrecommitValue<T = NodePtr, S = ()>
where
    S: ToTreeHash,
{
    pub tail_reveal: T,
    pub initial_inner_puzzle_hash: Bytes32,
    pub cat_maker_hash: Bytes32,
    pub cat_maker_solution: S,
}

impl<T> CatalogPrecommitValue<T> {
    pub fn with_default_cat_maker(
        payment_asset_tail_hash_hash: TreeHash,
        initial_inner_puzzle_hash: Bytes32,
        tail_reveal: T,
    ) -> Self {
        Self {
            tail_reveal,
            initial_inner_puzzle_hash,
            cat_maker_hash: DefaultCatMakerArgs::new(payment_asset_tail_hash_hash.into())
                .curry_tree_hash()
                .into(),
            cat_maker_solution: (),
        }
    }

    pub fn initial_inner_puzzle(
        ctx: &mut SpendContext,
        owner_inner_puzzle_hash: Bytes32,
        initial_metadata: &CatNftMetadata,
    ) -> Result<NodePtr, DriverError> {
        let mut conds = Conditions::new().create_coin(
            owner_inner_puzzle_hash,
            1,
            ctx.hint(owner_inner_puzzle_hash)?,
        );
        let updater_solution = ctx.alloc(&initial_metadata)?;
        conds = conds.update_nft_metadata(ctx.alloc_mod::<AnyMetadataUpdater>()?, updater_solution);
        conds = conds.remark(ctx.alloc(&"MEOW".to_string())?);

        ctx.alloc(&clvm_quote!(conds))
    }
}

// On-chain, the CATalog precommit value is just (TAIL . HASH)
impl<N, E: ClvmEncoder<Node = N>, T, S> ToClvm<E> for CatalogPrecommitValue<T, S>
where
    S: ToTreeHash,
    T: ToClvm<E> + Clone,
{
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let hash: Bytes32 = clvm_tuple!(
            self.initial_inner_puzzle_hash,
            clvm_tuple!(self.cat_maker_hash, self.cat_maker_solution.tree_hash())
        )
        .tree_hash()
        .into();

        clvm_tuple!(self.tail_reveal.clone(), hash).to_clvm(encoder)
    }
}

// value is:
// (c
//   (c (c cat_maker_reveal cat_maker_solution) (c pricing_puzzle_reveal pricing_solution))
//   (c (c secret handle) (c owner_launcher_id resolved_data)))
// )
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesPrecommitValue<CS = (), PS = TreeHash, S = Bytes32>
where
    CS: ToTreeHash,
    PS: ToTreeHash,
    S: ToTreeHash,
{
    pub cat_maker_hash: Bytes32,
    pub cat_maker_solution: CS,
    pub pricing_puzzle_hash: Bytes32,
    pub pricing_solution: PS,
    pub handle: String,
    pub secret: S,
    pub owner_launcher_id: Bytes32,
    pub resolved_data: Bytes,
}

impl<CS, PS, S> XchandlesPrecommitValue<CS, PS, S>
where
    CS: ToTreeHash,
    PS: ToTreeHash,
    S: ToTreeHash,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cat_maker_hash: Bytes32,
        cat_maker_solution: CS,
        pricing_puzzle_hash: Bytes32,
        pricing_solution: PS,
        handle: String,
        secret: S,
        owner_launcher_id: Bytes32,
        resolved_data: Bytes,
    ) -> Self {
        Self {
            cat_maker_hash,
            cat_maker_solution,
            pricing_puzzle_hash,
            pricing_solution,
            handle,
            secret,
            owner_launcher_id,
            resolved_data,
        }
    }
}

impl XchandlesPrecommitValue<(), TreeHash, Bytes32> {
    pub fn for_normal_registration<PS>(
        payment_tail_hash_hash: TreeHash,
        pricing_puzzle_hash: TreeHash,
        pricing_puzzle_solution: &PS,
        handle: String,
        secret: Bytes32,
        owner_launcher_id: Bytes32,
        resolved_data: Bytes,
    ) -> Self
    where
        PS: ToTreeHash,
    {
        Self::new(
            DefaultCatMakerArgs::new(payment_tail_hash_hash.into())
                .curry_tree_hash()
                .into(),
            (),
            pricing_puzzle_hash.into(),
            pricing_puzzle_solution.tree_hash(),
            handle,
            secret,
            owner_launcher_id,
            resolved_data,
        )
    }
}

// On-chain, the precommit value is just a hash of the data it stores
impl<N, E: ClvmEncoder<Node = N>, CS, PS, S> ToClvm<E> for XchandlesPrecommitValue<CS, PS, S>
where
    CS: ToTreeHash,
    PS: ToTreeHash,
    S: ToTreeHash,
{
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let data_hash: Bytes32 = clvm_tuple!(
            clvm_tuple!(
                clvm_tuple!(self.cat_maker_hash, self.cat_maker_solution.tree_hash()),
                clvm_tuple!(self.pricing_puzzle_hash, self.pricing_solution.tree_hash())
            ),
            clvm_tuple!(
                clvm_tuple!(self.handle.tree_hash(), self.secret.tree_hash()),
                clvm_tuple!(self.owner_launcher_id, self.resolved_data.tree_hash())
            )
        )
        .tree_hash()
        .into();

        data_hash.to_clvm(encoder)
    }
}
