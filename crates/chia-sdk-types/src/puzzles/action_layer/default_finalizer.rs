use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{Mod, puzzles::ACTION_LAYER_PUZZLE_HASH};

/// Serialized default action layer finalizer puzzle.
///
/// It re-creates the singleton with amount 1, the same finalizer and Merkle root, and the
/// new state, hinted to a fixed value, and passes through all conditions output by the
/// actions. Use [`DefaultFinalizer1stCurryArgs`] and [`DefaultFinalizer2ndCurryArgs`] to
/// curry it.
pub const DEFAULT_FINALIZER_PUZZLE: [u8; 494] = hex!(
    "
    ff02ffff01ff04ffff04ffff0133ffff04ffff02ff0affff04ff0effff04ff05
    ffff04ffff02ff0affff04ff0effff04ff17ffff04ffff0bffff0101ff1780ff
    8080808080ffff04ffff0bffff0101ff4f80ffff04ffff02ff0cffff04ff0cff
    82019f8080ff80808080808080ffff04ffff0101ffff04ffff04ff0bff8080ff
    8080808080ffff02ff08ffff04ff08ff81df808080ffff04ffff04ffff04ffff
    01ff02ffff03ff05ffff01ff04ff09ffff02ff02ffff04ff02ffff04ff0dff07
    80808080ffff01ff02ffff03ff07ffff01ff02ff02ffff04ff02ff078080ffff
    018080ff018080ff0180ffff01ff02ffff03ffff07ff0380ffff01ff0bffff01
    02ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff
    01ff0bffff0101ff038080ff018080ffff04ffff01ff0bffff0102ffff0bffff
    0182010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580
    ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080
    ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0b
    ffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ff
    ff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff01
    8201018080ff01808080ff018080
    "
);

/// Tree hash of [`DEFAULT_FINALIZER_PUZZLE`].
pub const DEFAULT_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b
    "
));

/// First curry of the default finalizer ([`DEFAULT_FINALIZER_PUZZLE`]).
///
/// The finalizer is curried twice so that it can refer to its own hash: the result of
/// this curry is curried again with [`DefaultFinalizer2ndCurryArgs`], whose only argument
/// is the hash of this first curry.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultFinalizer1stCurryArgs {
    /// Mod hash of the action layer ([`ACTION_LAYER_PUZZLE_HASH`]), used to compute the
    /// re-created singleton's inner puzzle hash.
    pub action_layer_mod_hash: Bytes32,
    /// Hint (memo) added to the coin that re-creates the singleton, usually the launcher id.
    pub hint: Bytes32,
}

impl DefaultFinalizer1stCurryArgs {
    /// Creates the arguments with the standard action layer mod hash.
    pub fn new(hint: Bytes32) -> Self {
        Self {
            action_layer_mod_hash: ACTION_LAYER_PUZZLE_HASH.into(),
            hint,
        }
    }

    /// Computes the hash of the finalizer after the first curry.
    pub fn curry_tree_hash(hint: Bytes32) -> TreeHash {
        CurriedProgram {
            program: DEFAULT_FINALIZER_PUZZLE_HASH,
            args: DefaultFinalizer1stCurryArgs::new(hint),
        }
        .tree_hash()
    }
}

/// Second curry of the default finalizer.
///
/// The resulting hash is what goes into [`ActionLayerArgs::finalizer`](crate::puzzles::ActionLayerArgs::finalizer).
///
/// # Example
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_sdk_types::puzzles::{
///     DEFAULT_FINALIZER_PUZZLE, DefaultFinalizer1stCurryArgs, DefaultFinalizer2ndCurryArgs,
/// };
/// use clvm_traits::ToClvm;
/// use clvm_utils::{CurriedProgram, tree_hash};
/// use clvmr::{Allocator, serde::node_from_bytes};
///
/// let hint = Bytes32::new([1; 32]);
///
/// // Build the finalizer program by currying it twice.
/// let mut allocator = Allocator::new();
/// let finalizer_mod = node_from_bytes(&mut allocator, &DEFAULT_FINALIZER_PUZZLE)?;
/// let first_curry = CurriedProgram {
///     program: finalizer_mod,
///     args: DefaultFinalizer1stCurryArgs::new(hint),
/// }
/// .to_clvm(&mut allocator)?;
/// let finalizer = CurriedProgram {
///     program: first_curry,
///     args: DefaultFinalizer2ndCurryArgs::new(hint),
/// }
/// .to_clvm(&mut allocator)?;
///
/// // The hash helper agrees with the hash of the actual program.
/// assert_eq!(
///     tree_hash(&allocator, finalizer),
///     DefaultFinalizer2ndCurryArgs::curry_tree_hash(hint),
/// );
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultFinalizer2ndCurryArgs {
    /// Hash of the finalizer after the first curry (see
    /// [`DefaultFinalizer1stCurryArgs::curry_tree_hash`]).
    pub finalizer_self_hash: Bytes32,
}

impl DefaultFinalizer2ndCurryArgs {
    /// Creates the arguments for the finalizer with the given hint.
    pub fn new(hint: Bytes32) -> Self {
        Self {
            finalizer_self_hash: DefaultFinalizer1stCurryArgs::curry_tree_hash(hint).into(),
        }
    }

    /// Computes the full finalizer puzzle hash (after both curries) for the given hint.
    pub fn curry_tree_hash(hint: Bytes32) -> TreeHash {
        let self_hash = DefaultFinalizer1stCurryArgs::curry_tree_hash(hint);

        CurriedProgram {
            program: self_hash,
            args: DefaultFinalizer2ndCurryArgs {
                finalizer_self_hash: self_hash.into(),
            },
        }
        .tree_hash()
    }
}

impl Mod for DefaultFinalizer1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DEFAULT_FINALIZER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DEFAULT_FINALIZER_PUZZLE_HASH
    }
}
