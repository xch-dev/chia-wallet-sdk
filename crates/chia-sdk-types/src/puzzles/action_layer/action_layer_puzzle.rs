use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

/// Serialized action layer puzzle, compiled from Rue.
///
/// Use [`ActionLayerArgs`] to curry it.
pub const ACTION_LAYER_PUZZLE: [u8; 630] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ff5fffff01ff02ff05ffff04ffff04ff0bff1780ffff
    04ffff02ff1effff04ff1effff04ffff04ffff02ff0cffff04ffff04ff04ffff
    04ff0aff168080ffff04ffff04ff80ff5f80ffff04ff0bff2f80808080ff2f80
    ffff04ff80ffff04ffff04ffff04ff80ff1780ff8080ff81bf8080808080ff81
    ff808080ffff01ff088080ff0180ffff04ffff04ffff04ffff01ff02ffff03ff
    ff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff
    02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff01ff02
    ffff03ff0dffff01ff02ffff03ffff02ffff03ff35ffff01ff09ff0bffff02ff
    0affff04ff0affff04ff35ffff0bffff0101ffff02ff08ffff04ff08ffff02ff
    25ff0f8080808080808080ffff01ff02ff0effff04ff0effff04ff25ff098080
    8080ff0180ffff01ff02ff0cffff04ff02ffff04ffff04ffff04ff25ff0980ff
    1d80ff07808080ffff01ff088080ff0180ffff010980ff018080ffff04ffff01
    ff02ffff03ff0dffff01ff02ff02ffff04ff02ffff04ffff04ffff17ff09ffff
    0181ff80ff1d80ffff0bffff0102ffff03ffff18ff09ffff010180ff15ff0780
    ffff03ffff18ff09ffff010180ff07ff158080808080ffff010780ff0180ffff
    04ffff01ff02ffff03ffff09ff0bff0580ffff01ff0101ffff01ff02ff02ffff
    04ff02ffff04ff05ff0f80808080ff0180ffff01ff02ffff03ff09ffff01ff02
    ff02ffff04ff02ffff04ffff04ff19ff0d80ffff04ffff04ff37ff0b80ffff04
    ffff02ffff02ff11ff0d80ffff04ff27ff2f8080ff3f8080808080ffff01ff04
    ff27ffff04ff37ff0b808080ff0180808080ff018080
    "
);

/// Tree hash of [`ACTION_LAYER_PUZZLE`].
pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b
    "
));

/// Curried arguments of the action layer, the inner puzzle of an action-based singleton.
///
/// Each spend runs one or more action puzzles whose hashes are leaves of
/// [`merkle_root`](Self::merkle_root). Every action receives
/// `(ephemeral_state . state)` plus its own solution and returns
/// `((new_ephemeral_state . new_state) . conditions)`. The persistent state is threaded
/// from one action to the next, and the ephemeral state starts as nil in every spend.
/// After the last action, the [`finalizer`](Self::finalizer) receives the Merkle root,
/// the initial state, and the final state and conditions, and outputs the spend's
/// conditions. It is normally what re-creates the singleton with the new state.
///
/// The solution is [`RawActionLayerSolution`].
///
/// # Example
///
/// Compute the full puzzle hash of an action layer singleton with the default
/// finalizer and a single delegated state action:
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_puzzle_types::singleton::SingletonArgs;
/// use chia_sdk_types::{
///     MerkleTree,
///     puzzles::{ActionLayerArgs, DefaultFinalizer2ndCurryArgs, DelegatedStateActionArgs},
/// };
/// use clvm_utils::ToTreeHash;
///
/// let launcher_id = Bytes32::new([1; 32]);
/// let price_singleton_launcher_id = Bytes32::new([2; 32]);
///
/// // The Merkle root commits to the set of actions the singleton can run.
/// let action_puzzle_hashes: Vec<Bytes32> =
///     vec![DelegatedStateActionArgs::curry_tree_hash(price_singleton_launcher_id).into()];
/// let merkle_root = MerkleTree::new(&action_puzzle_hashes).root();
///
/// // Any value that can be serialized to CLVM can be the state.
/// let state = (Bytes32::new([3; 32]), 100_u64);
///
/// let inner_puzzle_hash = ActionLayerArgs::curry_tree_hash(
///     DefaultFinalizer2ndCurryArgs::curry_tree_hash(launcher_id),
///     merkle_root,
///     state.tree_hash(),
/// );
/// let puzzle_hash = SingletonArgs::curry_tree_hash(launcher_id, inner_puzzle_hash);
/// # let _ = puzzle_hash;
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ActionLayerArgs<F, S> {
    /// Finalizer puzzle, run once after all actions to produce the final conditions.
    ///
    /// It is usually a curried [`DefaultFinalizer1stCurryArgs`](crate::puzzles::DefaultFinalizer1stCurryArgs)
    /// or [`ReserveFinalizer1stCurryArgs`](crate::puzzles::ReserveFinalizer1stCurryArgs) program.
    pub finalizer: F,
    /// Merkle root of the tree whose leaves are the tree hashes of the allowed action puzzles.
    ///
    /// Build it with [`MerkleTree`](crate::MerkleTree).
    pub merkle_root: Bytes32,
    /// Persistent state, passed to the first action of each spend.
    pub state: S,
}

impl<F, S> ActionLayerArgs<F, S> {
    /// Creates the curried arguments from their parts.
    pub fn new(finalizer: F, merkle_root: Bytes32, state: S) -> Self {
        Self {
            finalizer,
            merkle_root,
            state,
        }
    }
}

impl ActionLayerArgs<TreeHash, TreeHash> {
    /// Computes the curried action layer puzzle hash from the finalizer puzzle hash, the
    /// Merkle root of the action puzzle hashes, and the tree hash of the state.
    ///
    /// See the [type-level example](ActionLayerArgs#example).
    pub fn curry_tree_hash(
        finalizer: TreeHash,
        merkle_root: Bytes32,
        state_hash: TreeHash,
    ) -> TreeHash {
        CurriedProgram {
            program: ACTION_LAYER_PUZZLE_HASH,
            args: ActionLayerArgs::<TreeHash, TreeHash>::new(finalizer, merkle_root, state_hash),
        }
        .tree_hash()
    }
}

/// Solution to the action layer puzzle ([`ActionLayerArgs`]).
///
/// The action layer prepends each selected selector to a list as it verifies it, so the
/// actions run in the reverse order of
/// [`selectors_and_proofs`](Self::selectors_and_proofs), while
/// [`solutions`](Self::solutions) is consumed in execution order.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RawActionLayerSolution<P, S, F> {
    /// Reveals of the action puzzles used in this spend, each listed once even if it runs
    /// several times.
    pub puzzles: Vec<P>,
    /// For each action run, a CLVM path into [`puzzles`](Self::puzzles) that selects the
    /// action puzzle (`2` for the first, `5` for the second, `11` for the third, and so on),
    /// and a proof that the puzzle's hash is in the Merkle root.
    ///
    /// The proof may be `None` if the same selector already appeared earlier in this list
    /// with a proof.
    pub selectors_and_proofs: Vec<(u32, Option<MerkleProof>)>,
    /// Solution for each action, in execution order.
    pub solutions: Vec<S>,
    /// Extra solution passed to the finalizer (for example a [`ReserveFinalizerSolution`](crate::puzzles::ReserveFinalizerSolution)).
    #[clvm(rest)]
    pub finalizer_solution: F,
}

impl<P, S, F> Mod for RawActionLayerSolution<P, S, F> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ACTION_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        ACTION_LAYER_PUZZLE_HASH
    }
}

impl<P, S> Mod for ActionLayerArgs<P, S> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ACTION_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        ACTION_LAYER_PUZZLE_HASH
    }
}
