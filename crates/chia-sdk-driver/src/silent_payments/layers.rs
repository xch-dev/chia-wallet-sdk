//! Wrappers and eligible spends of the Extension to CATs (CHIP-0057
//! "Extension to CATs": "Wrappers", "Eligible CAT Spends", "Wrapper List of a
//! Block").
//!
//! A [`Wrapper`] `W = (asset_id, hidden_ph)` names the layers around the
//! standard puzzle of a CAT coin: the `cat_v2` layer with its asset ID and, for
//! a revocable CAT (CHIP-0038), the revocation layer with its hidden puzzle
//! hash. An XCH coin has no wrapper (`None`). [`Wrapper::wrap`] is the CHIP's
//! `wrap(W, h)`, computed from tree hashes alone by [`CatInfo::puzzle_hash`],
//! the SDK's own computation of a CAT's puzzle hash.
//!
//! [`eligible_spend`] is the CHIP's procedure `eligible_spend`, which replaces
//! the procedure of Recognizing an Eligible Spend: it returns the pair `(A, W)`
//! of the spend's key and its wrapper for exactly three shapes of puzzle, the
//! CHIP's closed list:
//!
//! 1. the standard puzzle, bare: `(A, None)`;
//! 2. `cat_v2` around the standard puzzle: `(A, (asset_id, None))`;
//! 3. `cat_v2` around the CHIP-0038 revocation layer, spent on the holder's
//!    path, the revealed holder puzzle being the standard puzzle:
//!    `(A, (asset_id, hidden_ph))`.
//!
//! `A` is always the synthetic public key of the standard puzzle. Every other
//! spend (a CAT inside a CAT, a CAT around an offer's settlement coin, a
//! revocable CAT spent through the issuer's hidden path, a DID, an NFT, ...)
//! is not eligible.
//!
//! Matching is exact at every layer, as the CHIP requires: the module's tree
//! hash, the exact curried arguments (typed uncurry, 32-byte atoms, exact
//! arity) and the tree hash of the whole layer. A puzzle that only looks like
//! one of these layers contributes no key to a spend group. The SDK's
//! [`crate::CatLayer`] and [`crate::RevocationLayer`] parsers are not used
//! here: they check the module hash and the shape of the arguments only, and
//! return an error where this procedure returns `None`.
//!
//! [`coin_wrapper`] gives the wrapper every exact `cat_v2` spend adds to its
//! block's wrapper list, whether or not the spend is eligible, and
//! [`sort_wrappers`] puts a wrapper list in the CHIP's order.
//!
//! Nothing here panics or unwraps on block data: every failed step returns
//! `None`.

use chia_bls::PublicKey;
use chia_protocol::{Bytes32, CoinSpend};
use chia_puzzle_types::cat::CatArgs;
use chia_puzzle_types::standard::StandardArgs;
use chia_puzzles::{
    CAT_PUZZLE_HASH, P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH, REVOCATION_LAYER_HASH,
};
use chia_sdk_types::Mod;
use chia_sdk_types::puzzles::RevocationArgs;
use clvm_traits::{FromClvm, destructure_curried_args, match_curried_args};
use clvm_utils::{CurriedProgram, TreeHash, tree_hash};
use clvmr::serde::node_from_bytes;
use clvmr::{Allocator, NodePtr, SExp};

use crate::CatInfo;

/// A wrapper `W = (asset_id, hidden_ph)` of CHIP-0057 ("Wrappers").
///
/// `asset_id` is the 32-byte asset ID of the CAT in the `cat_v2` layer (the
/// hash of its TAIL), and `hidden_puzzle_hash` is, for a revocable CAT, the
/// 32-byte hidden puzzle hash of the CHIP-0038 revocation layer, or `None`
/// otherwise.
///
/// Two wrappers are equal when both fields are equal as bytes. Wrappers have
/// no implicit order: the order of a block's wrapper list is the CHIP's, given
/// by [`sort_wrappers`] only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Wrapper {
    /// The CAT's asset ID (the hash of its TAIL).
    pub asset_id: Bytes32,
    /// The revocation layer's hidden puzzle hash, for a revocable CAT.
    pub hidden_puzzle_hash: Option<Bytes32>,
}

impl Wrapper {
    /// The wrapper of a plain CAT: `(asset_id, None)`.
    pub fn cat(asset_id: Bytes32) -> Self {
        Self {
            asset_id,
            hidden_puzzle_hash: None,
        }
    }

    /// The wrapper of a revocable CAT: `(asset_id, hidden_ph)`.
    pub fn revocable(asset_id: Bytes32, hidden_puzzle_hash: Bytes32) -> Self {
        Self {
            asset_id,
            hidden_puzzle_hash: Some(hidden_puzzle_hash),
        }
    }

    /// `wrap(W, h)`: the puzzle hash of a coin with this wrapper whose inner
    /// standard puzzle has tree hash `puzzle_hash`.
    ///
    /// The tree hash of `cat_v2` curried with (`CAT_MOD_HASH`, `asset_id`,
    /// inner puzzle), where the inner puzzle has tree hash `puzzle_hash` for a
    /// plain CAT and, for a revocable CAT, the tree hash of the revocation
    /// layer curried with (`REVOCATION_MOD_HASH`, `hidden_ph`, `puzzle_hash`).
    /// This is [`CatInfo::puzzle_hash`].
    pub fn wrap(&self, puzzle_hash: Bytes32) -> Bytes32 {
        CatInfo::new(self.asset_id, self.hidden_puzzle_hash, puzzle_hash)
            .puzzle_hash()
            .into()
    }
}

/// `wrap(W, h)` for a wrapper that may be none: `wrap(None, h) = h`.
pub fn wrap_puzzle_hash(wrapper: Option<&Wrapper>, puzzle_hash: Bytes32) -> Bytes32 {
    wrapper.map_or(puzzle_hash, |w| w.wrap(puzzle_hash))
}

/// The distinct wrappers of `wrappers` in the CHIP's order ("Wrapper List of a
/// Block"):
///
/// "The list is given in a fixed, total order: by *`asset_id`* as bytes, and for
/// one asset ID the wrapper without a hidden puzzle hash first, then by
/// *`hidden_ph`* as bytes."
///
/// The key is spelled out rather than derived, so the order does not depend on
/// the field order of [`Wrapper`]. Duplicates are removed.
pub fn sort_wrappers(wrappers: impl IntoIterator<Item = Wrapper>) -> Vec<Wrapper> {
    let mut sorted: Vec<Wrapper> = wrappers.into_iter().collect();
    sorted.sort_by_key(|w| {
        (
            w.asset_id.to_bytes(),
            w.hidden_puzzle_hash.is_some(),
            w.hidden_puzzle_hash.map_or([0u8; 32], Bytes32::to_bytes),
        )
    });
    sorted.dedup();
    sorted
}

/// `eligible_spend` of a coin spend: `(A, W)` for the three eligible shapes,
/// else `None` (see the module documentation).
///
/// The puzzle reveal and the solution are deserialized once. A solution that
/// does not deserialize is read as nil: it is only read for the revocable
/// shape, which it then does not have.
pub fn eligible_spend(coin_spend: &CoinSpend) -> Option<(PublicKey, Option<Wrapper>)> {
    let mut a = Allocator::new();
    let puzzle = node_from_bytes(&mut a, coin_spend.puzzle_reveal.as_ref()).ok()?;
    let solution = match node_from_bytes(&mut a, coin_spend.solution.as_ref()) {
        Ok(node) => node,
        Err(_) => NodePtr::NIL,
    };
    eligible_spend_from_nodes(&a, puzzle, solution)
}

/// `eligible_spend` over an already deserialized puzzle reveal and solution.
///
/// The CHIP's steps, in its order:
/// 1. the puzzle is the standard puzzle: (its key, `None`);
/// 2. unless the puzzle is exactly `cat_v2`: `None`;
/// 3. the CAT's inner puzzle is the standard puzzle: (its key, `(asset_id, None)`);
/// 4. unless the inner puzzle is exactly the revocation layer: `None`;
/// 5. `hidden_ph` and `inner_ph` are its second and third curried arguments;
/// 6. `hidden_flag` and `revealed` are the first two elements of the CAT
///    layer's inner solution, the first element of the solution; another
///    shape: `None`;
/// 7. `hidden_flag` is not nil (the issuer's hidden path): `None`;
/// 8. `tree_hash(revealed) != inner_ph`: `None`;
/// 9. `revealed` is the standard puzzle: (its key, `(asset_id, hidden_ph)`);
/// 10. otherwise `None`.
pub fn eligible_spend_from_nodes(
    a: &Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Option<(PublicKey, Option<Wrapper>)> {
    // (1) The bare standard puzzle.
    if let Some(key) = standard_puzzle_key(a, puzzle) {
        return Some((key, None));
    }
    // (2) Exactly cat_v2.
    let (asset_id, inner) = match_cat(a, puzzle)?;
    // (3) cat_v2 around the standard puzzle.
    if let Some(key) = standard_puzzle_key(a, inner) {
        return Some((key, Some(Wrapper::cat(asset_id))));
    }
    // (4, 5) Exactly the revocation layer.
    let (hidden_puzzle_hash, inner_puzzle_hash) = match_revocation(a, inner)?;
    // (6) The holder-path shape of the solution.
    let (hidden_flag, revealed) = holder_path(a, solution)?;
    // (7) nil is the EMPTY atom only: a pair or any non-empty atom (one zero
    // byte included) is the issuer's hidden path.
    if !is_nil(a, hidden_flag) {
        return None;
    }
    // (8) The revealed puzzle is the one the layer committed to.
    if Bytes32::from(tree_hash(a, revealed)) != inner_puzzle_hash {
        return None;
    }
    // (9) The revealed puzzle is the standard puzzle; (10) otherwise none.
    let key = standard_puzzle_key(a, revealed)?;
    Some((key, Some(Wrapper::revocable(asset_id, hidden_puzzle_hash))))
}

/// The wrapper a coin spend adds to its block's wrapper list, or `None` when
/// its puzzle reveal is not exactly `cat_v2` ("Wrapper List of a Block").
///
/// Every exact `cat_v2` spend has a wrapper, eligible or not: a CAT around an
/// offer's settlement coin, a revocable CAT spent through the issuer's hidden
/// path and a CAT inside a CAT all add theirs.
pub fn coin_wrapper(coin_spend: &CoinSpend) -> Option<Wrapper> {
    let mut a = Allocator::new();
    let puzzle = node_from_bytes(&mut a, coin_spend.puzzle_reveal.as_ref()).ok()?;
    coin_wrapper_from_node(&a, puzzle)
}

/// [`coin_wrapper`] over an already deserialized puzzle reveal:
/// `(asset_id, hidden_ph)` when the `cat_v2` inner puzzle is exactly the
/// revocation layer, `(asset_id, None)` for any other inner puzzle.
pub fn coin_wrapper_from_node(a: &Allocator, puzzle: NodePtr) -> Option<Wrapper> {
    let (asset_id, inner) = match_cat(a, puzzle)?;
    Some(Wrapper {
        asset_id,
        hidden_puzzle_hash: match_revocation(a, inner).map(|(hidden, _)| hidden),
    })
}

/// The synthetic public key of `puzzle` when it is exactly the standard puzzle
/// (CHIP-0057 "Recognizing an Eligible Spend"): its uncurried module has tree
/// hash `P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH`, it has exactly one curried
/// argument, a 48-byte public key, and its tree hash equals that of the
/// standard puzzle curried with that key.
fn standard_puzzle_key(a: &Allocator, puzzle: NodePtr) -> Option<PublicKey> {
    let curried = CurriedProgram::<NodePtr, StandardArgs>::from_clvm(a, puzzle).ok()?;
    if tree_hash(a, curried.program) != TreeHash::new(P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH) {
        return None;
    }
    let key = curried.args.synthetic_key;
    if tree_hash(a, puzzle) != StandardArgs::curry_tree_hash(key) {
        return None;
    }
    Some(key)
}

/// `(asset_id, inner puzzle)` when `puzzle` is exactly `cat_v2`.
///
/// Its uncurried module has tree hash `CAT_MOD_HASH`; it has exactly three
/// curried arguments, the first equal to `CAT_MOD_HASH`, the second a 32-byte
/// atom `asset_id` and the third a puzzle `inner`; and its tree hash equals
/// `wrap((asset_id, None), tree_hash(inner))`.
fn match_cat(a: &Allocator, puzzle: NodePtr) -> Option<(Bytes32, NodePtr)> {
    let curried = CurriedProgram::<NodePtr, CatArgs<NodePtr>>::from_clvm(a, puzzle).ok()?;
    if tree_hash(a, curried.program) != TreeHash::new(CAT_PUZZLE_HASH) {
        return None;
    }
    if curried.args.mod_hash != Bytes32::new(CAT_PUZZLE_HASH) {
        return None;
    }
    let asset_id = curried.args.asset_id;
    let inner = curried.args.inner_puzzle;
    let inner_hash = Bytes32::from(tree_hash(a, inner));
    if Bytes32::from(tree_hash(a, puzzle)) != Wrapper::cat(asset_id).wrap(inner_hash) {
        return None;
    }
    Some((asset_id, inner))
}

/// `(hidden_ph, inner_ph)` when `puzzle` is exactly the revocation layer.
///
/// Its uncurried module has tree hash `REVOCATION_MOD_HASH`; it has exactly
/// three curried arguments, each a 32-byte atom, the first equal to
/// `REVOCATION_MOD_HASH`; and its tree hash equals the tree hash of the
/// revocation layer curried with those three atoms.
fn match_revocation(a: &Allocator, puzzle: NodePtr) -> Option<(Bytes32, Bytes32)> {
    let curried =
        CurriedProgram::<NodePtr, match_curried_args!(Bytes32, Bytes32, Bytes32)>::from_clvm(
            a, puzzle,
        )
        .ok()?;
    if tree_hash(a, curried.program) != TreeHash::new(REVOCATION_LAYER_HASH) {
        return None;
    }
    let destructure_curried_args!(mod_hash, hidden_puzzle_hash, inner_puzzle_hash) = curried.args;
    if mod_hash != Bytes32::new(REVOCATION_LAYER_HASH) {
        return None;
    }
    if tree_hash(a, puzzle)
        != RevocationArgs::new(hidden_puzzle_hash, inner_puzzle_hash).curry_tree_hash()
    {
        return None;
    }
    Some((hidden_puzzle_hash, inner_puzzle_hash))
}

/// `(hidden_flag, revealed)`: the first two elements of the CAT layer's inner
/// solution, which is the first element of the spend's solution.
fn holder_path(a: &Allocator, solution: NodePtr) -> Option<(NodePtr, NodePtr)> {
    let SExp::Pair(inner_solution, _) = a.sexp(solution) else {
        return None;
    };
    let SExp::Pair(hidden_flag, rest) = a.sexp(inner_solution) else {
        return None;
    };
    let SExp::Pair(revealed, _) = a.sexp(rest) else {
        return None;
    };
    Some((hidden_flag, revealed))
}

/// True for the empty atom only (CLVM's nil).
fn is_nil(a: &Allocator, node: NodePtr) -> bool {
    matches!(a.sexp(node), SExp::Atom) && a.atom_len(node) == 0
}
