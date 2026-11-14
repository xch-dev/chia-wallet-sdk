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

#[cfg(test)]
mod tests {
    //! The CHIP's closed list of eligible spends ("Eligible CAT Spends"), the
    //! wrappers of a block ("Wrapper List of a Block") and `wrap`, over
    //! running or exactly shaped fixtures. The negatives are those of the
    //! reference's `tests/test_cats.py` and `tests/sim/test_layers.py`, and of
    //! the Rust crates' `layers.rs`.

    use super::*;
    use crate::{Cat, CatSpend, SpendContext, SpendWithConditions, StandardLayer};
    use chia_bls::SecretKey;
    use chia_protocol::{Coin, Program};
    use chia_puzzle_types::{LineageProof, Memos};
    use chia_puzzles::{
        CAT_PUZZLE, P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE, REVOCATION_LAYER, SETTLEMENT_PAYMENT,
        SETTLEMENT_PAYMENT_HASH,
    };
    use chia_sdk_types::{Conditions, run_puzzle};
    use clvm_traits::{ToClvm, clvm_curried_args};
    use clvmr::serde::node_to_bytes;
    use hex_literal::hex;

    const ASSET: Bytes32 = Bytes32::new([0xa1; 32]);
    const ASSET_B: Bytes32 = Bytes32::new([0xb2; 32]);
    const HIDDEN: Bytes32 = Bytes32::new([0x99; 32]);
    const HIDDEN_LOW: Bytes32 = Bytes32::new([0x55; 32]);

    fn key(seed: u8) -> PublicKey {
        SecretKey::from_seed(&[seed; 32]).public_key()
    }

    fn standard_ph(key: PublicKey) -> Bytes32 {
        StandardArgs::curry_tree_hash(key).into()
    }

    fn pay(amount: u64) -> Conditions {
        Conditions::new().create_coin(Bytes32::new([0x77; 32]), amount, Memos::None)
    }

    /// A CAT coin of `info` with a lineage proof whose parent is a CAT coin of
    /// the same puzzle hash and amount.
    fn cat_coin(parent: u8, info: CatInfo, amount: u64) -> Cat {
        let puzzle_hash: Bytes32 = info.puzzle_hash().into();
        let parent_parent = Bytes32::new([parent ^ 0xff; 32]);
        let parent_coin = Coin::new(parent_parent, puzzle_hash, amount);
        Cat::new(
            Coin::new(parent_coin.coin_id(), puzzle_hash, amount),
            Some(LineageProof {
                parent_parent_coin_info: parent_parent,
                parent_inner_puzzle_hash: info.inner_puzzle_hash().into(),
                parent_amount: amount,
            }),
            info,
        )
    }

    /// A spend of a CAT coin (`hidden` none) or a revocable CAT coin held in
    /// the standard puzzle of `committed`, built by the SDK's `Cat::spend_all`.
    /// The inner spend is made with the standard puzzle of `revealed`; the
    /// issuer's hidden path when `revoke`.
    fn cat_spend(
        parent: u8,
        asset_id: Bytes32,
        hidden: Option<Bytes32>,
        committed: PublicKey,
        revealed: PublicKey,
        revoke: bool,
    ) -> CoinSpend {
        let mut ctx = SpendContext::new();
        let cat = cat_coin(
            parent,
            CatInfo::new(asset_id, hidden, standard_ph(committed)),
            1,
        );
        let inner = StandardLayer::new(revealed)
            .spend_with_conditions(&mut ctx, pay(1))
            .expect("inner spend");
        let spend = if revoke {
            CatSpend::revoke(cat, inner)
        } else {
            CatSpend::new(cat, inner)
        };
        Cat::spend_all(&mut ctx, &[spend]).expect("spend_all");
        ctx.take().pop().expect("one coin spend")
    }

    /// A revocable CAT of (ASSET, `hidden`) held by `key(41)`.
    fn revocable(parent: u8, hidden: Bytes32, revoke: bool) -> CoinSpend {
        cat_spend(parent, ASSET, Some(hidden), key(41), key(41), revoke)
    }

    fn runs(spend: &CoinSpend) -> bool {
        let mut a = Allocator::new();
        let puzzle = node_from_bytes(&mut a, spend.puzzle_reveal.as_ref()).unwrap();
        let solution = node_from_bytes(&mut a, spend.solution.as_ref()).unwrap();
        run_puzzle(&mut a, puzzle, solution).is_ok()
    }

    /// The same spend with the holder path's `hidden_flag` replaced.
    fn with_hidden_flag(
        spend: &CoinSpend,
        flag: impl FnOnce(&mut Allocator) -> NodePtr,
    ) -> CoinSpend {
        let mut a = Allocator::new();
        let solution = node_from_bytes(&mut a, spend.solution.as_ref()).unwrap();
        let SExp::Pair(inner_solution, rest) = a.sexp(solution) else {
            panic!("a CAT solution is a list");
        };
        let SExp::Pair(_, tail) = a.sexp(inner_solution) else {
            panic!("a holder-path inner solution is a list");
        };
        let new_flag = flag(&mut a);
        let inner_solution = a.new_pair(new_flag, tail).unwrap();
        let solution = a.new_pair(inner_solution, rest).unwrap();
        CoinSpend::new(
            spend.coin,
            spend.puzzle_reveal.clone(),
            Program::from(node_to_bytes(&a, solution).unwrap()),
        )
    }

    fn standard_puzzle(a: &mut Allocator, key: PublicKey) -> NodePtr {
        let module = node_from_bytes(a, &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE).unwrap();
        CurriedProgram {
            program: module,
            args: StandardArgs::new(key),
        }
        .to_clvm(a)
        .unwrap()
    }

    fn cat_puzzle(a: &mut Allocator, asset_id: Bytes32, inner: NodePtr) -> NodePtr {
        let module = node_from_bytes(a, &CAT_PUZZLE).unwrap();
        CurriedProgram {
            program: module,
            args: CatArgs::new(asset_id, inner),
        }
        .to_clvm(a)
        .unwrap()
    }

    /// A coin spend of `puzzle` with solution `()`, its coin's puzzle hash the
    /// puzzle's tree hash. Nothing is run.
    fn spend_of(a: &Allocator, parent: u8, puzzle: NodePtr) -> CoinSpend {
        CoinSpend::new(
            Coin::new(Bytes32::new([parent; 32]), tree_hash(a, puzzle).into(), 1),
            Program::from(node_to_bytes(a, puzzle).unwrap()),
            Program::from(vec![0x80]),
        )
    }

    /// `cat_v2` of `asset_id` around the offer settlement puzzle (not run).
    fn settlement_cat(parent: u8, asset_id: Bytes32) -> CoinSpend {
        let mut a = Allocator::new();
        let settlement = node_from_bytes(&mut a, &SETTLEMENT_PAYMENT).unwrap();
        let puzzle = cat_puzzle(&mut a, asset_id, settlement);
        spend_of(&a, parent, puzzle)
    }

    fn h(hex_str: &str) -> Bytes32 {
        Bytes32::new(hex::decode(hex_str).unwrap().try_into().unwrap())
    }

    #[test]
    fn test_module_hashes_are_the_chips() {
        assert_eq!(
            CAT_PUZZLE_HASH,
            hex!("37bef360ee858133b69d595a906dc45d01af50379dad515eb9518abb7c1d2a7a")
        );
        assert_eq!(
            REVOCATION_LAYER_HASH,
            hex!("00848115554ea674131f89f311707a959ad3f4647482648f3fe91ba289131f51")
        );
        assert_eq!(
            SETTLEMENT_PAYMENT_HASH,
            hex!("cfbfdeed5c4ca2de3d0bf520b9cb4bb7743a359bd2e6a188d19ce7dffc21d3e7")
        );
    }

    #[test]
    fn test_wrap_equals_cat_args_curry_tree_hash() {
        for seed in [0u8, 1, 0x5a, 0xff] {
            let inner = Bytes32::new([seed; 32]);
            let expected: Bytes32 = CatArgs::curry_tree_hash(ASSET, inner.into()).into();
            assert_eq!(Wrapper::cat(ASSET).wrap(inner), expected);
            assert_eq!(
                wrap_puzzle_hash(Some(&Wrapper::cat(ASSET)), inner),
                expected
            );
            assert_eq!(wrap_puzzle_hash(None, inner), inner);
            // The revocable wrap curries the revocation layer around the hash.
            let revocation: Bytes32 = RevocationArgs::new(HIDDEN, inner).curry_tree_hash().into();
            assert_eq!(
                Wrapper::revocable(ASSET, HIDDEN).wrap(inner),
                Bytes32::from(CatArgs::curry_tree_hash(ASSET, revocation.into()))
            );
        }
    }

    #[test]
    fn test_wrap_reproduces_vector_9_and_vector_11() {
        // Vector 9 (vector_9_cat_payment, outputs[0]), copied from the JSON.
        let v9 = Wrapper::cat(h(
            "a031232e72df4aa270c253a530b9b0d782d46ac7b5dad63c2aa37690bbb37f6d",
        ));
        assert_eq!(
            v9.wrap(h(
                "df83709ebd837a46717cfb59cc3c75de70606ffb5e3499c202339cf9a5a20dd4"
            )),
            h("cf9f1b5dbf2bd67fa0bb69a554e38c0f2d53877fe565f34f16a16df9f6fa18e5")
        );

        // Vector 11 (vector_11_revocable_cat, outputs[0] and its negative
        // controls), copied from the JSON.
        let asset = h("c8bd63be5a809f7da4af2ff4c33be96ece3373aa596bec55c60c548c115ed011");
        let hidden = h("e5380d52cb3751ed5b07ceb5dc8eaf48335196e6e2847159c77354fad8372c3e");
        let p = h("4d3fbd4e5db29059a9f9e94474510a2e96f9d4017845ab5cea58bed984aa04a1");
        assert_eq!(
            Wrapper::revocable(asset, hidden).wrap(p),
            h("87b7f0ce1f5ed425c958cad384eb45754a7d02c7cbaa42b5f2d929f2a7d6eb4c")
        );
        assert_eq!(
            Bytes32::from(CatInfo::new(asset, Some(hidden), p).inner_puzzle_hash()),
            h("05a3f42711fed22fe7e2000571e6ad35a958a74a16bfb7c68e7829a5f9ef00ef")
        );
        // The same hash under the plain-CAT wrapper is another coin.
        assert_eq!(
            Wrapper::cat(asset).wrap(p),
            h("5bc561afde1054b263ecb5934acad1c93ed684eaac55b978bf343732619be916")
        );
    }

    /// A < B as asset IDs, `HIDDEN_LOW` < `HIDDEN`.
    fn chip_order_input() -> Vec<Wrapper> {
        vec![
            Wrapper::cat(ASSET_B),
            Wrapper::revocable(ASSET, HIDDEN),
            Wrapper::cat(ASSET),
            Wrapper::revocable(ASSET, HIDDEN_LOW),
            Wrapper::cat(ASSET_B),
        ]
    }

    fn chip_order_output() -> Vec<Wrapper> {
        vec![
            Wrapper::cat(ASSET),
            Wrapper::revocable(ASSET, HIDDEN_LOW),
            Wrapper::revocable(ASSET, HIDDEN),
            Wrapper::cat(ASSET_B),
        ]
    }

    #[test]
    fn test_sort_wrappers_chip_order_and_dedup() {
        assert!(ASSET < ASSET_B && HIDDEN_LOW < HIDDEN);
        assert_eq!(sort_wrappers(chip_order_input()), chip_order_output());
        assert_eq!(sort_wrappers(Vec::new()), Vec::<Wrapper>::new());
        // The wrapper without a hidden puzzle hash comes first even when a
        // hidden puzzle hash is all zeros.
        let zero = Wrapper::revocable(ASSET, Bytes32::new([0u8; 32]));
        assert_eq!(
            sort_wrappers([zero, Wrapper::cat(ASSET)]),
            vec![Wrapper::cat(ASSET), zero]
        );
    }

    /// Every permutation of `items` (Heap's algorithm).
    fn permutations(items: &[Wrapper]) -> Vec<Vec<Wrapper>> {
        fn heap(n: usize, items: &mut Vec<Wrapper>, out: &mut Vec<Vec<Wrapper>>) {
            if n <= 1 {
                out.push(items.clone());
                return;
            }
            for i in 0..n - 1 {
                heap(n - 1, items, out);
                if n.is_multiple_of(2) {
                    items.swap(i, n - 1);
                } else {
                    items.swap(0, n - 1);
                }
            }
            heap(n - 1, items, out);
        }
        let mut items = items.to_vec();
        let mut out = Vec::new();
        heap(items.len(), &mut items, &mut out);
        out
    }

    #[test]
    fn test_sort_wrappers_is_independent_of_input_order() {
        let all = permutations(&chip_order_input());
        assert_eq!(all.len(), 120);
        for permutation in all {
            assert_eq!(sort_wrappers(permutation), chip_order_output());
        }
    }

    #[test]
    fn test_eligible_spend_closed_list_shapes() {
        // 1. The bare standard puzzle.
        let mut ctx = SpendContext::new();
        StandardLayer::new(key(41))
            .spend(
                &mut ctx,
                Coin::new(Bytes32::new([1; 32]), standard_ph(key(41)), 1),
                pay(1),
            )
            .unwrap();
        let xch = ctx.take().pop().unwrap();
        assert!(runs(&xch));
        assert_eq!(eligible_spend(&xch), Some((key(41), None)));
        assert_eq!(coin_wrapper(&xch), None);

        // 2. cat_v2 around the standard puzzle.
        let cat = cat_spend(2, ASSET, None, key(41), key(41), false);
        assert!(runs(&cat));
        assert_eq!(
            eligible_spend(&cat),
            Some((key(41), Some(Wrapper::cat(ASSET))))
        );
        assert_eq!(coin_wrapper(&cat), Some(Wrapper::cat(ASSET)));

        // 3. A revocable CAT on the holder's path: the key is read from the
        // revealed holder puzzle.
        let rcat = revocable(3, HIDDEN, false);
        assert!(runs(&rcat));
        let w = Wrapper::revocable(ASSET, HIDDEN);
        assert_eq!(eligible_spend(&rcat), Some((key(41), Some(w))));
        assert_eq!(coin_wrapper(&rcat), Some(w));

        // The fixtures' coins have puzzle hash wrap(W, standard puzzle hash).
        assert_eq!(
            cat.coin.puzzle_hash,
            Wrapper::cat(ASSET).wrap(standard_ph(key(41)))
        );
        assert_eq!(rcat.coin.puzzle_hash, w.wrap(standard_ph(key(41))));
    }

    #[test]
    fn test_issuer_hidden_path_is_not_eligible() {
        // The SDK's issuer path: hidden_flag = 1.
        let spend = revocable(4, HIDDEN, true);
        assert_eq!(eligible_spend(&spend), None);
        assert_eq!(
            coin_wrapper(&spend),
            Some(Wrapper::revocable(ASSET, HIDDEN))
        );
        let flag_one =
            with_hidden_flag(&revocable(4, HIDDEN, false), |a| a.new_atom(&[1]).unwrap());
        assert_eq!(eligible_spend(&flag_one), None);
    }

    #[test]
    fn test_hidden_flag_zero_byte_is_not_nil() {
        // CLVM's nil is the EMPTY atom; the one-byte atom 0x00 is not nil.
        let holder = revocable(5, HIDDEN, false);
        assert!(eligible_spend(&holder).is_some());
        let zero = with_hidden_flag(&holder, |a| a.new_atom(&[0]).unwrap());
        assert_eq!(eligible_spend(&zero), None);
        assert_eq!(coin_wrapper(&zero), Some(Wrapper::revocable(ASSET, HIDDEN)));
        // The empty atom put back is the holder's path again.
        let empty = with_hidden_flag(&zero, |a| a.new_atom(&[]).unwrap());
        assert_eq!(empty.solution, holder.solution);
        assert!(eligible_spend(&empty).is_some());
    }

    #[test]
    fn test_hidden_flag_pair_is_not_nil() {
        let pair = with_hidden_flag(&revocable(6, HIDDEN, false), |a| {
            a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap()
        });
        assert_eq!(eligible_spend(&pair), None);
        assert_eq!(coin_wrapper(&pair), Some(Wrapper::revocable(ASSET, HIDDEN)));
    }

    #[test]
    fn test_rehashed_holder_puzzle_is_not_eligible() {
        // The revealed puzzle is a standard puzzle, but of another key than
        // the one the layer committed to: its tree hash is not inner_ph.
        let spend = cat_spend(7, ASSET, Some(HIDDEN), key(41), key(42), false);
        assert_eq!(eligible_spend(&spend), None);
        assert_eq!(
            coin_wrapper(&spend),
            Some(Wrapper::revocable(ASSET, HIDDEN))
        );
    }

    #[test]
    fn test_solution_without_holder_shape_is_not_eligible() {
        let spend = revocable(8, HIDDEN, false);
        let mut a = Allocator::new();
        let puzzle = node_from_bytes(&mut a, spend.puzzle_reveal.as_ref()).unwrap();
        let holder = node_from_bytes(&mut a, spend.solution.as_ref()).unwrap();
        assert!(eligible_spend_from_nodes(&a, puzzle, holder).is_some());

        let nil = NodePtr::NIL;
        // Solution (): no CAT inner solution.
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, nil), None);
        // An atom where the CAT inner solution should be.
        let atom = a.new_atom(&[1, 2, 3]).unwrap();
        let solution = a.new_pair(atom, nil).unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, solution), None);
        // An inner solution with one element only: no revealed puzzle.
        let one = a.new_pair(nil, nil).unwrap();
        let solution = a.new_pair(one, nil).unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, solution), None);
        // A whole solution that is an atom.
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, atom), None);
        // As coin spends: solution () and a solution that does not deserialize.
        for solution in [vec![0x80], vec![0xff]] {
            let bad = CoinSpend::new(
                spend.coin,
                spend.puzzle_reveal.clone(),
                Program::from(solution),
            );
            assert_eq!(eligible_spend(&bad), None);
            assert_eq!(coin_wrapper(&bad), Some(Wrapper::revocable(ASSET, HIDDEN)));
        }
    }

    #[test]
    fn test_cat_inside_cat_is_not_eligible() {
        let mut a = Allocator::new();
        let standard = standard_puzzle(&mut a, key(41));
        let inner_cat = cat_puzzle(&mut a, ASSET, standard);
        let puzzle = cat_puzzle(&mut a, ASSET, inner_cat);
        let spend = spend_of(&a, 9, puzzle);
        assert_eq!(eligible_spend(&spend), None);
        // The outer layer is an exact cat_v2: it still adds its wrapper.
        assert_eq!(coin_wrapper(&spend), Some(Wrapper::cat(ASSET)));
        // The inner CAT alone is eligible, so the outer layer is what rejects it.
        let inner_spend = spend_of(&a, 9, inner_cat);
        assert_eq!(
            eligible_spend(&inner_spend),
            Some((key(41), Some(Wrapper::cat(ASSET))))
        );
    }

    #[test]
    fn test_cat_around_settlement_is_not_eligible_but_has_wrapper() {
        let spend = settlement_cat(10, ASSET);
        assert_eq!(
            spend.coin.puzzle_hash,
            Wrapper::cat(ASSET).wrap(SETTLEMENT_PAYMENT_HASH.into())
        );
        assert_eq!(eligible_spend(&spend), None);
        assert_eq!(coin_wrapper(&spend), Some(Wrapper::cat(ASSET)));
    }

    #[test]
    fn test_malformed_layers_are_rejected() {
        let mut a = Allocator::new();
        let cat_mod = node_from_bytes(&mut a, &CAT_PUZZLE).unwrap();
        let standard = standard_puzzle(&mut a, key(41));
        let solution = NodePtr::NIL;

        // A 31-byte asset ID.
        let short_asset = a.new_atom(&[0xa1; 31]).unwrap();
        let puzzle = CurriedProgram {
            program: cat_mod,
            args: clvm_curried_args!(Bytes32::new(CAT_PUZZLE_HASH), short_asset, standard),
        }
        .to_clvm(&mut a)
        .unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, solution), None);
        assert_eq!(coin_wrapper_from_node(&a, puzzle), None);
        let spend = spend_of(&a, 11, puzzle);
        assert_eq!((eligible_spend(&spend), coin_wrapper(&spend)), (None, None));

        // A fourth curried CAT argument.
        let extra = a.new_atom(&[7]).unwrap();
        let puzzle = CurriedProgram {
            program: cat_mod,
            args: clvm_curried_args!(Bytes32::new(CAT_PUZZLE_HASH), ASSET, standard, extra),
        }
        .to_clvm(&mut a)
        .unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, solution), None);
        assert_eq!(coin_wrapper_from_node(&a, puzzle), None);

        // A first curried argument that is not CAT_MOD_HASH.
        let puzzle = CurriedProgram {
            program: cat_mod,
            args: clvm_curried_args!(Bytes32::new([0x37; 32]), ASSET, standard),
        }
        .to_clvm(&mut a)
        .unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, solution), None);
        assert_eq!(coin_wrapper_from_node(&a, puzzle), None);

        // A look-alike module with CAT-shaped arguments: its tree hash is not
        // CAT_MOD_HASH.
        let look_alike = a.new_atom(&[1]).unwrap();
        let puzzle = CurriedProgram {
            program: look_alike,
            args: CatArgs::new(ASSET, standard),
        }
        .to_clvm(&mut a)
        .unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, solution), None);
        assert_eq!(coin_wrapper_from_node(&a, puzzle), None);

        // A revocation layer with four curried arguments, inside an exact
        // cat_v2: not eligible, and the inner puzzle is not exactly the
        // revocation layer, so the wrapper is (asset_id, None) (CHIP "Wrapper
        // List of a Block").
        let revocation_mod = node_from_bytes(&mut a, &REVOCATION_LAYER).unwrap();
        let four = CurriedProgram {
            program: revocation_mod,
            args: clvm_curried_args!(
                Bytes32::new(REVOCATION_LAYER_HASH),
                HIDDEN,
                standard_ph(key(41)),
                Bytes32::new([0u8; 32])
            ),
        }
        .to_clvm(&mut a)
        .unwrap();
        let puzzle = cat_puzzle(&mut a, ASSET, four);
        let holder = {
            let tail = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();
            let tail = a.new_pair(standard, tail).unwrap();
            let inner = a.new_pair(NodePtr::NIL, tail).unwrap();
            a.new_pair(inner, NodePtr::NIL).unwrap()
        };
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, holder), None);
        assert_eq!(
            coin_wrapper_from_node(&a, puzzle),
            Some(Wrapper::cat(ASSET))
        );

        // The same holder solution with an exact three-argument layer is
        // eligible, so the fourth argument is what rejects it.
        let three = CurriedProgram {
            program: revocation_mod,
            args: RevocationArgs::new(HIDDEN, standard_ph(key(41))),
        }
        .to_clvm(&mut a)
        .unwrap();
        let puzzle = cat_puzzle(&mut a, ASSET, three);
        assert_eq!(
            eligible_spend_from_nodes(&a, puzzle, holder),
            Some((key(41), Some(Wrapper::revocable(ASSET, HIDDEN))))
        );

        // A revocation layer whose first argument is not REVOCATION_MOD_HASH.
        let wrong_mod = CurriedProgram {
            program: revocation_mod,
            args: clvm_curried_args!(Bytes32::new([0x01; 32]), HIDDEN, standard_ph(key(41))),
        }
        .to_clvm(&mut a)
        .unwrap();
        let puzzle = cat_puzzle(&mut a, ASSET, wrong_mod);
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, holder), None);
        assert_eq!(
            coin_wrapper_from_node(&a, puzzle),
            Some(Wrapper::cat(ASSET))
        );
    }

    #[test]
    fn test_standard_puzzle_with_extra_argument_is_not_eligible() {
        let mut a = Allocator::new();
        let module = node_from_bytes(&mut a, &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE).unwrap();
        let extra = a.new_atom(&[7]).unwrap();
        let puzzle = CurriedProgram {
            program: module,
            args: clvm_curried_args!(key(41), extra),
        }
        .to_clvm(&mut a)
        .unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, NodePtr::NIL), None);
        // Nor inside cat_v2.
        let cat = cat_puzzle(&mut a, ASSET, puzzle);
        assert_eq!(eligible_spend_from_nodes(&a, cat, NodePtr::NIL), None);
        assert_eq!(coin_wrapper_from_node(&a, cat), Some(Wrapper::cat(ASSET)));
        // A look-alike module with one key argument.
        let look_alike = a.new_atom(&[1]).unwrap();
        let puzzle = CurriedProgram {
            program: look_alike,
            args: StandardArgs::new(key(41)),
        }
        .to_clvm(&mut a)
        .unwrap();
        assert_eq!(eligible_spend_from_nodes(&a, puzzle, NodePtr::NIL), None);
    }

    #[test]
    fn test_issuer_path_revocable_cat_keeps_its_wrapper() {
        // Not eligible, but an exact cat_v2 around an exact revocation layer:
        // its wrapper carries the hidden puzzle hash.
        for hidden in [HIDDEN, HIDDEN_LOW] {
            let spend = revocable(12, hidden, true);
            assert_eq!(eligible_spend(&spend), None);
            assert_eq!(
                coin_wrapper(&spend),
                Some(Wrapper::revocable(ASSET, hidden))
            );
        }
    }

    #[test]
    fn test_puzzle_that_does_not_deserialize_is_neither_eligible_nor_wrapped() {
        let spend = CoinSpend::new(
            Coin::new(Bytes32::new([13; 32]), Bytes32::new([14; 32]), 1),
            Program::from(vec![0xff]),
            Program::from(vec![0x80]),
        );
        assert_eq!(eligible_spend(&spend), None);
        assert_eq!(coin_wrapper(&spend), None);
    }
}
