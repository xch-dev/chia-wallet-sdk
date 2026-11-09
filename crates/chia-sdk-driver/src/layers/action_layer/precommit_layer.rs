use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::{
    Conditions, Mod,
    puzzles::{
        AnyMetadataUpdater, CatNftMetadata, DefaultCatMakerArgs, PRECOMMIT_LAYER_PUZZLE_HASH,
        PrecommitLayer1stCurryArgs, PrecommitLayer2ndCurryArgs, PrecommitLayerSolution,
    },
};
use clvm_traits::{ClvmEncoder, FromClvm, ToClvm, ToClvmError, clvm_quote, clvm_tuple};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The precommit [`Layer`] locks a payment, usually a CAT, until a controller singleton either
/// accepts or refunds it.
///
/// Registries such as `CATalog` and `XCHandles` use precommit coins for front-running protection: a
/// user first creates a precommit coin that commits to the registration data in its
/// [`value`](Self::value), and only reveals the data when the registry spends the coin after
/// [`relative_block_height`](Self::relative_block_height) blocks.
///
/// The puzzle is curried twice, first with [`PrecommitLayer1stCurryArgs`] and then with
/// [`PrecommitLayer2ndCurryArgs`]. When spent with a [`PrecommitLayerSolution`], it:
///
/// - Asserts that the coin is at least `relative_block_height` blocks old.
/// - Creates a coin with the same amount at [`payout_puzzle_hash`](Self::payout_puzzle_hash)
///   in [`PrecommitSpendMode::REGISTER`](chia_sdk_types::puzzles::PrecommitSpendMode::REGISTER)
///   mode, or at [`refund_puzzle_hash`](Self::refund_puzzle_hash) in
///   [`PrecommitSpendMode::REFUND`](chia_sdk_types::puzzles::PrecommitSpendMode::REFUND) mode,
///   hinted to the same puzzle hash.
/// - Requires a message containing the spend mode from the controller singleton.
///
/// The value isn't used by the puzzle, only committed to by its puzzle hash. See
/// [`CatalogPrecommitValue`] and [`XchandlesPrecommitValue`].
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{Layer, PrecommitLayer, Puzzle, SpendContext};
/// use clvm_utils::ToTreeHash;
///
/// let ctx = &mut SpendContext::new();
///
/// let layer = PrecommitLayer::new(
///     Bytes32::new([1; 32]),
///     32,
///     Bytes32::new([2; 32]),
///     Bytes32::new([3; 32]),
///     Bytes32::new([4; 32]),
/// );
///
/// let puzzle = layer.construct_puzzle(ctx)?;
/// assert_eq!(
///     ctx.tree_hash(puzzle),
///     PrecommitLayer::<Bytes32>::puzzle_hash(
///         layer.controller_singleton_struct_hash,
///         layer.relative_block_height,
///         layer.payout_puzzle_hash,
///         layer.refund_puzzle_hash,
///         layer.value.tree_hash(),
///     )
/// );
///
/// let parsed = PrecommitLayer::<Bytes32>::parse_puzzle(ctx, Puzzle::parse(ctx, puzzle))?
///     .expect("precommit layer");
/// assert_eq!(parsed.value, layer.value);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct PrecommitLayer<V> {
    /// The tree hash of the [`SingletonStruct`](chia_puzzle_types::singleton::SingletonStruct)
    /// of the singleton that can spend this coin, such as a registry.
    pub controller_singleton_struct_hash: Bytes32,
    /// The number of blocks that must pass after the coin is created before it can be spent.
    pub relative_block_height: u32,
    /// The puzzle hash that receives the coin when the controller accepts it.
    pub payout_puzzle_hash: Bytes32,
    /// The puzzle hash that receives the coin when the controller refunds it.
    pub refund_puzzle_hash: Bytes32,
    /// The value committed to by the puzzle, such as the registration data.
    pub value: V,
}

impl<V> PrecommitLayer<V> {
    /// Creates a new [`PrecommitLayer`].
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

    /// Computes the tree hash of the puzzle curried with [`PrecommitLayer1stCurryArgs`].
    ///
    /// Action puzzles are curried with this hash so that they can compute the puzzle hash of
    /// any precommit coin for their singleton.
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

    /// Computes the puzzle hash of a precommit coin from its fields and the tree hash of its
    /// value.
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

    /// Constructs the puzzle. This is the same as [`Layer::construct_puzzle`], but only requires
    /// `V` to implement [`ToClvm`].
    ///
    /// # Errors
    ///
    /// Returns an error if the puzzle can't be allocated.
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

    /// Allocates the solution. This is the same as [`Layer::construct_solution`], but doesn't
    /// require any traits on `V`.
    ///
    /// # Errors
    ///
    /// Returns an error if the solution can't be allocated.
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

/// The value of a `CATalog` registration [`PrecommitLayer`] coin.
///
/// It's encoded on-chain as `(tail_reveal . data_hash)`, where `data_hash` is the tree hash of
/// `(initial_inner_puzzle_hash . (cat_maker_hash . cat_maker_solution))`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogPrecommitValue<T = NodePtr, S = ()>
where
    S: ToTreeHash,
{
    /// The TAIL program of the CAT being registered. Its tree hash is the asset id.
    pub tail_reveal: T,
    /// The p2 puzzle hash of the eve NFT minted for the registered CAT, usually the tree hash
    /// of [`CatalogPrecommitValue::initial_inner_puzzle`].
    pub initial_inner_puzzle_hash: Bytes32,
    /// The puzzle hash of the CAT maker for the payment asset.
    pub cat_maker_hash: Bytes32,
    /// The solution of the CAT maker. Only its tree hash is used, so a [`TreeHash`] can be
    /// passed.
    pub cat_maker_solution: S,
}

impl<T> CatalogPrecommitValue<T> {
    /// Creates a value that uses the default CAT maker ([`DefaultCatMakerArgs`]) for a payment
    /// asset whose asset id has the tree hash `payment_asset_tail_hash_hash`.
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

    /// Creates the p2 puzzle of the eve NFT minted for a registered CAT.
    ///
    /// It's a quoted list of conditions that sends the NFT to `owner_inner_puzzle_hash` (hinted
    /// to it), sets the metadata to `initial_metadata` with [`AnyMetadataUpdater`], and adds a
    /// `"MEOW"` remark.
    ///
    /// # Errors
    ///
    /// Returns an error if the puzzle can't be allocated.
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

/// The value of an `XCHandles` registration [`PrecommitLayer`] coin.
///
/// It's encoded on-chain as only the tree hash of
/// `(((cat_maker_hash . cat_maker_solution) . (pricing_puzzle_hash . pricing_solution)) .
/// ((handle . secret) . (owner_launcher_id . resolved_launcher_id)))`, so the handle stays
/// hidden until the registry spends the precommit coin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesPrecommitValue<CS = (), PS = TreeHash, S = Bytes32>
where
    CS: ToTreeHash,
    PS: ToTreeHash,
    S: ToTreeHash,
{
    /// The puzzle hash of the CAT maker for the payment asset.
    pub cat_maker_hash: Bytes32,
    /// The solution of the CAT maker. Only its tree hash is used, so a [`TreeHash`] can be
    /// passed.
    pub cat_maker_solution: CS,
    /// The puzzle hash of the pricing puzzle used to compute the registration price.
    pub pricing_puzzle_hash: Bytes32,
    /// The solution of the pricing puzzle. Only its tree hash is used, so a [`TreeHash`] can
    /// be passed.
    pub pricing_solution: PS,
    /// The handle being registered.
    pub handle: String,
    /// A secret that prevents others from guessing the handle from the puzzle hash. Only its
    /// tree hash is committed to.
    pub secret: S,
    /// The launcher id of the singleton that will own the handle.
    pub owner_launcher_id: Bytes32,
    /// The launcher id of the singleton the handle will resolve to.
    pub resolved_launcher_id: Bytes32,
}

impl<CS, PS, S> XchandlesPrecommitValue<CS, PS, S>
where
    CS: ToTreeHash,
    PS: ToTreeHash,
    S: ToTreeHash,
{
    /// Creates a new [`XchandlesPrecommitValue`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cat_maker_hash: Bytes32,
        cat_maker_solution: CS,
        pricing_puzzle_hash: Bytes32,
        pricing_solution: PS,
        handle: String,
        secret: S,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
    ) -> Self {
        Self {
            cat_maker_hash,
            cat_maker_solution,
            pricing_puzzle_hash,
            pricing_solution,
            handle,
            secret,
            owner_launcher_id,
            resolved_launcher_id,
        }
    }
}

impl XchandlesPrecommitValue<(), TreeHash, Bytes32> {
    /// Creates a value for a registration paid with a CAT through the default CAT maker
    /// ([`DefaultCatMakerArgs`]).
    ///
    /// `payment_tail_hash_hash` is the tree hash of the payment asset id. Only the tree hash of
    /// `pricing_puzzle_solution` is stored.
    pub fn for_normal_registration<PS>(
        payment_tail_hash_hash: TreeHash,
        pricing_puzzle_hash: TreeHash,
        pricing_puzzle_solution: &PS,
        handle: String,
        secret: Bytes32,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
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
            resolved_launcher_id,
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
                clvm_tuple!(self.owner_launcher_id, self.resolved_launcher_id)
            )
        )
        .tree_hash()
        .into();

        data_hash.to_clvm(encoder)
    }
}
