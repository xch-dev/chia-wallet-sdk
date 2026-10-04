//! Builds a CHIP-0057 [`TweakData`] from the coin spends and additions of a
//! block.
//!
//! Given a block's `Vec<CoinSpend>` (removals with puzzle reveals and
//! solutions) and `Vec<Coin>` (additions), this forms the block's spend groups
//! as in the `ScanBlock` procedure of CHIP-0057 ("Scanning a Block") and emits
//! one tweak point `T = input_hash * A_sum` per group ("Tweak Points"), plus
//! one [`OutputMeta`] per addition. Decompressing the block generator is the
//! caller's responsibility; the function takes the spends as they come out of
//! it, so it is a pure function with no `chia-consensus` dependency.
//!
//! ## Spend groups
//!
//! - **Eligible spends.** Each [`CoinSpend`] is parsed with
//!   [`StandardLayer::parse_puzzle`]. A spend whose puzzle is anything other
//!   than the standard puzzle (CAT, NFT, any other puzzle) is ignored from then
//!   on: it contributes no key, no coin id, and no edge.
//! - **Pass 1 — single-input groups.** Every eligible spend on its own is a
//!   spend group.
//! - **Pass 2 — multi-input groups.** Every eligible spend is run to extract
//!   its conditions. Each `ASSERT_CONCURRENT_SPEND` condition naming another
//!   eligible spend of the block is a directed edge, and every strongly
//!   connected component of two or more spends is a spend group. Strongly
//!   connected components, not weakly connected ones, are used: a third party
//!   that asserts a coin of a group without being asserted back has an edge
//!   into the group but is not part of it, so it cannot change the group's
//!   `A_sum`.
//!
//! The two passes overlap: a coin of a multi-input group also forms a
//! single-input group.
//!
//! ## Tweak points
//!
//! For each group, `A_sum` is the sum of the synthetic keys (one term per
//! coin) and `input_hash` is computed from the smallest coin id and `A_sum`.
//! Groups whose `A_sum` is the identity element, or whose `input_hash` is
//! zero, yield no tweak point ("Edge Cases"). Byte-identical tweak points are
//! emitted once.
//!
//! Tweak points are emitted in a stable order: the single-input groups in
//! `coin_spends` order, then the multi-input groups in the order the strongly
//! connected components are found.

use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_sdk_types::{Condition, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};
use indexmap::IndexMap;

use crate::silent_payments::{OutputMeta, TweakData, compute_tweak_point};
use crate::{DriverError, Layer, Puzzle, StandardLayer};

/// Parsed-standard-puzzle row carried between stages.
struct StandardSpend {
    coin_id: Bytes32,
    synthetic_pk: PublicKey,
    puzzle: NodePtr,
    solution: NodePtr,
}

/// Build a [`TweakData`] from a real (or simulator) block's coin spends and
/// additions: one tweak point per spend group (see
/// [`crate::silent_payments::compute_tweak_point`]) and one [`OutputMeta`] per
/// addition.
///
/// Spends of puzzles other than the standard puzzle are skipped silently,
/// without panicking or returning an error.
///
/// See module-level docs for how the spend groups are formed and for the order
/// of the tweak points.
///
/// # Errors
///
/// The current implementation never returns an error; the `Result` return
/// type leaves room for validation to be added without an API break.
pub fn tweak_data_from_block_spends(
    coin_spends: &[CoinSpend],
    additions: &[Coin],
) -> Result<TweakData, DriverError> {
    let mut allocator = Allocator::new();

    // Eligible spends: those whose puzzle reveal is the standard puzzle.
    let mut standard_spends: Vec<StandardSpend> = Vec::new();
    for spend in coin_spends {
        let Ok(puzzle_ptr) = spend.puzzle_reveal.to_clvm(&mut allocator) else {
            continue;
        };
        let parsed = Puzzle::parse(&allocator, puzzle_ptr);
        let Ok(Some(layer)) = StandardLayer::parse_puzzle(&allocator, parsed) else {
            continue;
        };
        let Ok(solution_ptr) = spend.solution.to_clvm(&mut allocator) else {
            continue;
        };
        standard_spends.push(StandardSpend {
            coin_id: spend.coin.coin_id(),
            synthetic_pk: layer.synthetic_key,
            puzzle: puzzle_ptr,
            solution: solution_ptr,
        });
    }

    // Candidate groups accumulate additively across both passes; a single coin
    // may appear in several (its Pass-1 singleton and/or a Pass-2 SCC). No pass
    // excludes a coin from any other pass.
    let mut groups: Vec<Vec<usize>> = Vec::new();

    // Pass 1 — a singleton candidate group for EVERY standard spend (in
    // `coin_spends` input order). This is the only pass that detects
    // single-input sends, so it runs unconditionally.
    for i in 0..standard_spends.len() {
        groups.push(vec![i]);
    }

    // Pass 2 — strongly connected components of the `ASSERT_CONCURRENT_SPEND`
    // graph over all eligible spends.
    if !standard_spends.is_empty() {
        // Coin-id -> graph-node-position map over every standard spend.
        let coin_id_to_pos: IndexMap<Bytes32, usize> = standard_spends
            .iter()
            .enumerate()
            .map(|(pos, ss)| (ss.coin_id, pos))
            .collect();

        // Adjacency list keyed by spend index (0..standard_spends.len()).
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); standard_spends.len()];
        for (pos, ss) in standard_spends.iter().enumerate() {
            let Ok(output) = run_puzzle(&mut allocator, ss.puzzle, ss.solution) else {
                continue;
            };
            let Ok(conditions) = Vec::<Condition>::from_clvm(&allocator, output) else {
                continue;
            };
            for cond in conditions {
                if let Condition::AssertConcurrentSpend(a) = cond
                    && let Some(&target_pos) = coin_id_to_pos.get(&a.coin_id)
                {
                    adj[pos].push(target_pos);
                }
            }
        }

        let sccs = iterative_tarjan_scc(&adj);
        for scc in sccs {
            if scc.len() >= 2 {
                groups.push(scc);
            }
        }
    }

    // Per-group aggregation and tweak point emission, de-duplicated by
    // the 48-byte compressed point (keeping the first occurrence to preserve the
    // documented emission order). Overlapping passes (a coin's singleton plus its
    // SCC membership) can yield byte-identical points; only byte-equal duplicates
    // are dropped, never groups that merely share a coin.
    let mut tweak_points: Vec<PublicKey> = Vec::new();
    let mut seen: std::collections::HashSet<[u8; 48]> = std::collections::HashSet::new();
    for group in groups {
        let coin_ids: Vec<Bytes32> = group.iter().map(|&i| standard_spends[i].coin_id).collect();
        let mut a_sum = standard_spends[group[0]].synthetic_pk;
        for &i in &group[1..] {
            a_sum += &standard_spends[i].synthetic_pk;
        }
        // A group whose keys sum to the identity element, or whose input hash is
        // zero, has no tweak point (CHIP-0057 "Tweak Points").
        let Some(tweak_point) = compute_tweak_point(&coin_ids, &a_sum) else {
            continue;
        };
        if seen.insert(tweak_point.to_bytes()) {
            tweak_points.push(tweak_point);
        }
    }

    // One OutputMeta per addition.
    let outputs: Vec<OutputMeta> = additions
        .iter()
        .map(|coin| OutputMeta {
            puzzle_hash: coin.puzzle_hash,
            coin_id: coin.coin_id(),
            amount: coin.amount,
            parent_coin_id: coin.parent_coin_info,
        })
        .collect();

    Ok(TweakData {
        tweak_points,
        outputs,
    })
}

/// Iterative Tarjan strongly-connected-components over a directed graph
/// represented as adjacency lists.
///
/// Returns SCCs in Tarjan finishing order. Iterative (explicit stack) to
/// avoid stack overflow on adversarial deep graphs that a recursive
/// implementation would not survive.
fn iterative_tarjan_scc(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut index_counter: usize = 0;
    let mut stack: Vec<usize> = Vec::new();
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut indices: Vec<Option<usize>> = vec![None; n];
    let mut lowlinks: Vec<usize> = vec![0; n];
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    for start in 0..n {
        if indices[start].is_some() {
            continue;
        }

        // Initialize root frame.
        indices[start] = Some(index_counter);
        lowlinks[start] = index_counter;
        index_counter += 1;
        stack.push(start);
        on_stack[start] = true;

        // Explicit call-stack frames: (node, neighbor iterator position).
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];

        while let Some(&(v, next_neighbor)) = work.last() {
            if next_neighbor < adj[v].len() {
                let w = adj[v][next_neighbor];
                // Advance the iterator on the current frame before recursing.
                if let Some(frame) = work.last_mut() {
                    frame.1 += 1;
                }
                if indices[w].is_none() {
                    indices[w] = Some(index_counter);
                    lowlinks[w] = index_counter;
                    index_counter += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, 0));
                } else if on_stack[w] {
                    let w_index = indices[w].expect("on-stack node has an index");
                    if w_index < lowlinks[v] {
                        lowlinks[v] = w_index;
                    }
                }
            } else {
                // All neighbors exhausted — finish this node.
                let v_index = indices[v].expect("visited node has an index");
                if lowlinks[v] == v_index {
                    let mut scc: Vec<usize> = Vec::new();
                    loop {
                        let w = stack.pop().expect("tarjan stack invariant");
                        on_stack[w] = false;
                        scc.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(scc);
                }
                work.pop();
                if let Some(&(parent, _)) = work.last()
                    && lowlinks[v] < lowlinks[parent]
                {
                    lowlinks[parent] = lowlinks[v];
                }
            }
        }
    }

    sccs
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::silent_payments::compute_input_hash;
    use crate::silent_payments::protocol::tweak_point_from_input_hash;
    use chia_sdk_types::silent_payments::ScalarField;

    use chia_protocol::{Coin, Program};
    use chia_puzzle_types::standard::StandardArgs;
    use chia_sdk_test::Simulator;
    use chia_sdk_test::silent_payments::tweak_data_from_simulator_block;
    use chia_sdk_types::Conditions;

    use crate::SpendContext;
    use crate::StandardLayer;

    /// Build a `(CoinSpend, Coin)` pair for a synthetic-key-controlled coin
    /// whose solution outputs `conditions`. The caller is responsible for the
    /// parent coin info; the puzzle hash is derived from `synthetic_pk` so the
    /// coin shape is internally consistent.
    fn build_standard_coin_spend(
        synthetic_pk: PublicKey,
        parent_coin_info: Bytes32,
        amount: u64,
        conditions: Conditions,
    ) -> CoinSpend {
        let puzzle_hash: Bytes32 = StandardArgs::curry_tree_hash(synthetic_pk).into();
        let coin = Coin::new(parent_coin_info, puzzle_hash, amount);

        let mut ctx = SpendContext::new();
        let layer = StandardLayer::new(synthetic_pk);
        layer
            .spend(&mut ctx, coin, conditions)
            .expect("layer spend");
        ctx.take().pop().expect("one coin spend")
    }

    /// An empty block produces no tweak points and no outputs without panic.
    /// Adding additions with no spends produces zero `tweak_points` and one
    /// `OutputMeta` per addition.
    #[test]
    fn test_empty_block() {
        let td = tweak_data_from_block_spends(&[], &[]).expect("empty block ok");
        assert!(td.tweak_points.is_empty());
        assert!(td.outputs.is_empty());

        let parent: Bytes32 = [0xAAu8; 32].into();
        let puzzle_hash: Bytes32 = [0xBBu8; 32].into();
        let coin = Coin::new(parent, puzzle_hash, 100);
        let td = tweak_data_from_block_spends(&[], &[coin]).expect("additions-only ok");
        assert!(td.tweak_points.is_empty());
        assert_eq!(td.outputs.len(), 1);
        assert_eq!(td.outputs[0].puzzle_hash, puzzle_hash);
    }

    /// A `CoinSpend` whose puzzle reveal is not the standard p2 puzzle skips
    /// silently; no tweak point is emitted and the helper does not
    /// error.
    #[test]
    fn test_non_standard_puzzle_skip() {
        // `Program::default()` deserializes to NIL — not a curried standard puzzle.
        let parent: Bytes32 = [0x11u8; 32].into();
        let puzzle_hash: Bytes32 = [0x22u8; 32].into();
        let coin = Coin::new(parent, puzzle_hash, 1);
        let spend = CoinSpend::new(coin, Program::default(), Program::default());

        let td = tweak_data_from_block_spends(&[spend], &[]).expect("non-standard skip ok");
        assert!(td.tweak_points.is_empty(), "non-standard puzzle must skip");
        assert!(td.outputs.is_empty());
    }

    /// A standard-puzzle spend whose synthetic key is the BLS12-381 identity
    /// element yields `A_sum = identity` and therefore `tweak_point = identity`;
    /// such a group is skipped (CHIP-0057 "Edge Cases"), so `tweak_points` stays
    /// empty.
    #[test]
    fn test_identity_element_guard() {
        let identity_pk = PublicKey::default();
        assert!(identity_pk.is_inf(), "PublicKey::default must be identity");

        let parent: Bytes32 = [0x33u8; 32].into();
        let spend = build_standard_coin_spend(identity_pk, parent, 1, Conditions::new());

        let td = tweak_data_from_block_spends(&[spend], &[]).expect("identity guard ok");
        assert!(
            td.tweak_points.is_empty(),
            "identity-element tweak_point must be suppressed",
        );
    }

    /// A single submitted standard-puzzle spend in the simulator produces the
    /// same `tweak_points` whether the data flows through the existing
    /// simulator helper or the new block-shape helper. The lone spend yields
    /// exactly one Pass-1 singleton (no cycle, so no Pass-2 SCC); this locks the
    /// single-input branch against drift versus the simulator-helper oracle.
    #[test]
    fn single_input_round_trip_matches_simulator_helper() {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(10);

        let layer = StandardLayer::new(alice.pk);
        layer
            .spend(&mut ctx, alice.coin, Conditions::new())
            .expect("alice spend");
        let coin_spends = ctx.take();

        sim.spend_coins(coin_spends, &[alice.sk]).expect("submit");
        let height = sim.height();

        let block_spends = sim.block_spends(height);
        let block_outputs = sim.block_outputs(height);

        let new_td =
            tweak_data_from_block_spends(&block_spends, &block_outputs).expect("new helper ok");
        let sim_td = tweak_data_from_simulator_block(&sim, height);

        assert_eq!(
            new_td.tweak_points.len(),
            sim_td.tweak_points.len(),
            "tweak_point count must match simulator helper",
        );
        for (a, b) in new_td.tweak_points.iter().zip(sim_td.tweak_points.iter()) {
            assert_eq!(a.to_bytes(), b.to_bytes(), "tweak_point bytes must match");
        }
    }

    /// Two standard-puzzle spends sharing the same `puzzle_hash`, bound by an
    /// `a <-> b` `AssertConcurrent` cycle — the exact shape the sender emits for
    /// any multi-input send (the input-binding gate forces the cycle for two or
    /// more spent coins, even when they share a puzzle hash).
    ///
    /// The two passes overlap: Pass 1 emits a singleton for EACH spend (two
    /// distinct `tweak_point`s — same `A_sum = alice_public` but different single
    /// coin-id sets, hence different `input_hash`), and Pass 2 emits the
    /// `{a, b}` SCC aggregate (`A_sum = 2 * alice_public` over both coin ids — a
    /// third distinct `tweak_point`). All three points are byte-distinct, so
    /// dedup keeps all three.
    #[test]
    fn same_ph_multi_input_round_trip_via_concurrent_spend() {
        let alice = chia_bls::SecretKey::from_seed(&[0x07u8; 32]);
        let alice_public = alice.public_key();

        let parent_a: Bytes32 = [0x55u8; 32].into();
        let parent_b: Bytes32 = [0x66u8; 32].into();

        // Same synthetic key -> identical puzzle hash. Pre-compute coin ids so
        // each spend can assert the other (the `ASSERT_CONCURRENT_SPEND` cycle).
        let puzzle_hash: Bytes32 = StandardArgs::curry_tree_hash(alice_public).into();
        let coin_a = Coin::new(parent_a, puzzle_hash, 100);
        let coin_b = Coin::new(parent_b, puzzle_hash, 200);
        let id_a = coin_a.coin_id();
        let id_b = coin_b.coin_id();

        let spend_a = build_standard_coin_spend(
            alice_public,
            parent_a,
            100,
            Conditions::new().assert_concurrent_spend(id_b),
        );
        let spend_b = build_standard_coin_spend(
            alice_public,
            parent_b,
            200,
            Conditions::new().assert_concurrent_spend(id_a),
        );

        assert_eq!(
            spend_a.coin.puzzle_hash, spend_b.coin.puzzle_hash,
            "same synthetic_pk must curry to the same puzzle_hash",
        );

        let td = tweak_data_from_block_spends(&[spend_a, spend_b], &[]).expect("multi-input ok");
        assert_eq!(
            td.tweak_points.len(),
            3,
            "two same-PH spends bound by a cycle: 2 Pass-1 singletons + 1 Pass-2 SCC aggregate",
        );
    }

    /// Pass 2 "pollution attack" oracle: a legitimate 2-coin SP cycle
    /// (`a -> b`, `b -> a`) coexists in the same block with a polluter coin
    /// whose solution emits `AssertConcurrentSpend(a)`. SCC grouping must
    /// place `{a, b}` in one group and the polluter alone in its own trivial
    /// group; `A_sum` for the legitimate pair must NOT be contaminated by the
    /// polluter's synthetic key.
    ///
    /// Concretely, the tweak point emitted for `{a, b}` in the polluted block
    /// must equal the tweak point emitted when only `{a, b}` are passed to
    /// the helper in isolation.
    #[test]
    fn test_concurrent_spend_pollution_resistance() {
        let sk_a = chia_bls::SecretKey::from_seed(&[0x01u8; 32]);
        let sk_b = chia_bls::SecretKey::from_seed(&[0x02u8; 32]);
        let sk_polluter = chia_bls::SecretKey::from_seed(&[0x03u8; 32]);
        let pk_a = sk_a.public_key();
        let pk_b = sk_b.public_key();
        let pk_polluter = sk_polluter.public_key();

        // Distinct parent_coin_infos so coin_ids differ from puzzle_hash and
        // from each other; required for the AssertConcurrentSpend edges to
        // resolve to the right targets.
        let parent_a: Bytes32 = [0xA0u8; 32].into();
        let parent_b: Bytes32 = [0xB0u8; 32].into();
        let parent_polluter: Bytes32 = [0xC0u8; 32].into();

        // Pre-compute coin_ids so each spend can reference the other.
        let puzzle_hash_a: Bytes32 = StandardArgs::curry_tree_hash(pk_a).into();
        let puzzle_hash_b: Bytes32 = StandardArgs::curry_tree_hash(pk_b).into();
        let puzzle_hash_polluter: Bytes32 = StandardArgs::curry_tree_hash(pk_polluter).into();
        let coin_a = Coin::new(parent_a, puzzle_hash_a, 100);
        let coin_b = Coin::new(parent_b, puzzle_hash_b, 200);
        let coin_polluter = Coin::new(parent_polluter, puzzle_hash_polluter, 300);
        let id_a = coin_a.coin_id();
        let id_b = coin_b.coin_id();

        // Closed cycle: a asserts b, b asserts a.
        let spend_a = build_standard_coin_spend(
            pk_a,
            parent_a,
            100,
            Conditions::new().assert_concurrent_spend(id_b),
        );
        let spend_b = build_standard_coin_spend(
            pk_b,
            parent_b,
            200,
            Conditions::new().assert_concurrent_spend(id_a),
        );
        // Polluter: forward edge into the cycle, no return edge.
        let spend_polluter = build_standard_coin_spend(
            pk_polluter,
            parent_polluter,
            300,
            Conditions::new().assert_concurrent_spend(id_a),
        );

        assert_eq!(spend_a.coin, coin_a);
        assert_eq!(spend_b.coin, coin_b);
        assert_eq!(spend_polluter.coin, coin_polluter);

        // Compute the legit `{a, b}` SCC tweak_point by hand so we can assert it
        // is PRESENT and byte-invariant across runs regardless of emission index.
        let mut legit_a_sum = pk_a;
        legit_a_sum += &pk_b;
        let legit_input_hash = compute_input_hash(&[id_a, id_b], &legit_a_sum);
        let mut legit_point = legit_a_sum;
        legit_point.scalar_multiply(&legit_input_hash.to_bytes());
        let legit = legit_point.to_bytes();

        let polluted =
            tweak_data_from_block_spends(&[spend_a.clone(), spend_b.clone(), spend_polluter], &[])
                .expect("polluted block ok");
        let clean = tweak_data_from_block_spends(&[spend_a, spend_b], &[]).expect("clean block ok");

        // Pass 1 emits a singleton per coin; Pass 2 emits the {a, b} SCC; the
        // polluter sits in its own trivial SCC (forward edge into the cycle but
        // no return edge) and is excluded from the legitimate group.
        assert_eq!(
            polluted.tweak_points.len(),
            4,
            "polluted block: 3 Pass-1 singletons (a, b, polluter) + 1 Pass-2 SCC {{a, b}}",
        );
        assert_eq!(
            clean.tweak_points.len(),
            3,
            "clean block: 2 Pass-1 singletons (a, b) + 1 Pass-2 SCC {{a, b}}",
        );

        // The legit SCC's tweak point must be present in BOTH runs and identical
        // — proving the polluter's synthetic key did NOT leak into A_sum.
        // Assert membership rather than a fixed index, since the emission order
        // places SCC groups after the singletons.
        assert!(
            polluted.tweak_points.iter().any(|p| p.to_bytes() == legit),
            "legit SCC tweak_point must be present in the polluted block",
        );
        assert!(
            clean.tweak_points.iter().any(|p| p.to_bytes() == legit),
            "legit SCC tweak_point must be present in the clean block",
        );
    }

    /// Three coins bound by one `ASSERT_CONCURRENT_SPEND` cycle, two of which
    /// share a synthetic key (and therefore a puzzle hash). The cycle is the
    /// shape the sender emits: each coin asserts its predecessor, and coin 0
    /// closes the cycle to the last coin.
    ///
    /// All three coins form one strongly connected component, and its tweak
    /// point uses `A_sum = pk_dup + pk_dup + pk_solo` (one term per coin, even
    /// though two coins share a key) over all three coin ids. That point is
    /// computed by hand and must be present in the output.
    #[test]
    fn three_coin_cycle_with_a_shared_key_forms_one_group() {
        // Two coins at PH_x share one synthetic key; the third uses a different
        // key (PH_y).
        let sk_dup = chia_bls::SecretKey::from_seed(&[0x11u8; 32]);
        let sk_solo = chia_bls::SecretKey::from_seed(&[0x22u8; 32]);
        let pk_dup = sk_dup.public_key();
        let pk_solo = sk_solo.public_key();

        let parent_dup1: Bytes32 = [0xD1u8; 32].into();
        let parent_dup2: Bytes32 = [0xD2u8; 32].into();
        let parent_solo: Bytes32 = [0xE1u8; 32].into();

        let puzzle_hash_dup: Bytes32 = StandardArgs::curry_tree_hash(pk_dup).into();
        let puzzle_hash_solo: Bytes32 = StandardArgs::curry_tree_hash(pk_solo).into();
        let coin_dup1 = Coin::new(parent_dup1, puzzle_hash_dup, 100);
        let coin_dup2 = Coin::new(parent_dup2, puzzle_hash_dup, 200);
        let coin_solo = Coin::new(parent_solo, puzzle_hash_solo, 300);
        let id_dup1 = coin_dup1.coin_id();
        let id_dup2 = coin_dup2.coin_id();
        let id_solo = coin_solo.coin_id();

        // Cyclic AssertConcurrent over coins [dup1, dup2, solo] in that order:
        // coin 0 asserts coin N-1, every other coin asserts its predecessor
        // (matches the sender's emit_relation cycle).
        let spend_dup1 = build_standard_coin_spend(
            pk_dup,
            parent_dup1,
            100,
            Conditions::new().assert_concurrent_spend(id_solo),
        );
        let spend_dup2 = build_standard_coin_spend(
            pk_dup,
            parent_dup2,
            200,
            Conditions::new().assert_concurrent_spend(id_dup1),
        );
        let spend_solo = build_standard_coin_spend(
            pk_solo,
            parent_solo,
            300,
            Conditions::new().assert_concurrent_spend(id_dup2),
        );

        assert_eq!(spend_dup1.coin, coin_dup1);
        assert_eq!(spend_dup2.coin, coin_dup2);
        assert_eq!(spend_solo.coin, coin_solo);
        assert_eq!(
            spend_dup1.coin.puzzle_hash, spend_dup2.coin.puzzle_hash,
            "the two dup coins must share one puzzle hash",
        );
        assert_ne!(
            spend_dup1.coin.puzzle_hash, spend_solo.coin.puzzle_hash,
            "the solo coin must sit at a distinct puzzle hash",
        );

        // Hand-compute the 3-coin aggregate `tweak_point` the sender would target.
        let mut a_sum = pk_dup;
        a_sum += &pk_dup;
        a_sum += &pk_solo;
        let input_hash = compute_input_hash(&[id_dup1, id_dup2, id_solo], &a_sum);
        let mut expected_point = a_sum;
        expected_point.scalar_multiply(&input_hash.to_bytes());
        let expected = expected_point.to_bytes();

        let td = tweak_data_from_block_spends(&[spend_dup1, spend_dup2, spend_solo], &[])
            .expect("mixed-PH cycle ok");

        assert!(
            td.tweak_points.iter().any(|p| p.to_bytes() == expected),
            "the full 3-coin-cycle aggregate tweak_point must be present (Pass-2 SCC over all \
             removals)",
        );
    }

    /// Two coins with the same synthetic key (and therefore the same puzzle
    /// hash) and no `ASSERT_CONCURRENT_SPEND` binding: one is the input of a
    /// single-input silent payment, the other an unrelated coin.
    ///
    /// Sharing a puzzle hash does not group coins. Each coin is its own
    /// single-input group, so the payment's input yields the tweak point with
    /// `A_sum = K_send` over its own coin id, and no multi-input group exists.
    #[test]
    fn coins_sharing_a_puzzle_hash_without_a_cycle_stay_single_input_groups() {
        let sk_send = chia_bls::SecretKey::from_seed(&[0x44u8; 32]);
        let pk_send = sk_send.public_key();

        let parent_send: Bytes32 = [0xF1u8; 32].into();
        let parent_other: Bytes32 = [0xF2u8; 32].into();

        let ph: Bytes32 = StandardArgs::curry_tree_hash(pk_send).into();
        let coin_send = Coin::new(parent_send, ph, 100);
        let id_send = coin_send.coin_id();

        // No AssertConcurrent: these two coins merely collide on puzzle hash.
        let spend_send = build_standard_coin_spend(pk_send, parent_send, 100, Conditions::new());
        let spend_other = build_standard_coin_spend(pk_send, parent_other, 200, Conditions::new());

        assert_eq!(
            spend_send.coin.puzzle_hash, spend_other.coin.puzzle_hash,
            "both coins must share one puzzle hash (the collision precondition)",
        );

        // Hand-compute the single-input send's Pass-1 singleton tweak_point.
        let single_a_sum = pk_send;
        let single_input_hash = compute_input_hash(&[id_send], &single_a_sum);
        let mut single_point = single_a_sum;
        single_point.scalar_multiply(&single_input_hash.to_bytes());
        let expected_single = single_point.to_bytes();

        let td = tweak_data_from_block_spends(&[spend_send, spend_other], &[])
            .expect("PH-collision single-input ok");

        assert!(
            td.tweak_points
                .iter()
                .any(|p| p.to_bytes() == expected_single),
            "the single-input send's Pass-1 singleton tweak_point must be present despite the \
             puzzle-hash collision",
        );
    }

    /// `r - sk`, the additive inverse of a secret key mod r.
    fn negate(sk: &chia_bls::SecretKey) -> chia_bls::SecretKey {
        use chia_sdk_types::silent_payments::GROUP_ORDER;

        let bytes = sk.to_bytes();
        let mut out = [0u8; 32];
        let mut borrow = 0u16;
        for i in (0..32).rev() {
            let lhs = u16::from(GROUP_ORDER[i]);
            let rhs = u16::from(bytes[i]) + borrow;
            if lhs >= rhs {
                out[i] = u8::try_from(lhs - rhs).unwrap();
                borrow = 0;
            } else {
                out[i] = u8::try_from(lhs + 256 - rhs).unwrap();
                borrow = 1;
            }
        }
        chia_bls::SecretKey::from_bytes(&out).expect("r - sk is below r")
    }

    /// CHIP-0057 "Edge Cases", scanner side: a spend group whose public keys
    /// sum to the identity element is skipped. Two coins with keys `a` and
    /// `r - a` bound in a cycle yield their two single-input tweak points, but
    /// none for the multi-input group.
    #[test]
    fn group_with_identity_key_sum_is_skipped() {
        let a = chia_bls::SecretKey::from_seed(&[0x21u8; 32]);
        let minus_a = negate(&a);
        let pk_a = a.public_key();
        let pk_b = minus_a.public_key();
        assert!((pk_a + &pk_b).is_inf());

        let parent_a: Bytes32 = [0x71u8; 32].into();
        let parent_b: Bytes32 = [0x72u8; 32].into();
        let id_a = Coin::new(parent_a, StandardArgs::curry_tree_hash(pk_a).into(), 1).coin_id();
        let id_b = Coin::new(parent_b, StandardArgs::curry_tree_hash(pk_b).into(), 1).coin_id();

        let spend_a = build_standard_coin_spend(
            pk_a,
            parent_a,
            1,
            Conditions::new().assert_concurrent_spend(id_b),
        );
        let spend_b = build_standard_coin_spend(
            pk_b,
            parent_b,
            1,
            Conditions::new().assert_concurrent_spend(id_a),
        );

        let td = tweak_data_from_block_spends(&[spend_a, spend_b], &[]).expect("ok");
        assert_eq!(
            td.tweak_points.len(),
            2,
            "only the two single-input groups yield a tweak point"
        );
        for (pk, id) in [(pk_a, id_a), (pk_b, id_b)] {
            let expected =
                tweak_point_from_input_hash(&pk, &compute_input_hash(&[id], &pk)).unwrap();
            assert!(td.tweak_points.contains(&expected));
        }
    }

    /// A group whose input hash is zero is skipped. A hash that reduces to zero
    /// cannot be produced on demand, so the helper is given the zero scalar.
    #[test]
    fn group_with_zero_input_hash_is_skipped() {
        let pk = chia_bls::SecretKey::from_seed(&[0x22u8; 32]).public_key();
        let zero = ScalarField::from_bytes_raw([0u8; 32]);
        assert!(tweak_point_from_input_hash(&pk, &zero).is_none());

        let one = {
            let mut bytes = [0u8; 32];
            bytes[31] = 1;
            ScalarField::from_bytes_raw(bytes)
        };
        assert_eq!(tweak_point_from_input_hash(&pk, &one), Some(pk));
        assert!(tweak_point_from_input_hash(&PublicKey::default(), &one).is_none());
    }

    /// A spend that is not an eligible spend: its puzzle is `1` (which returns
    /// its solution), so it can output any conditions, but it is not the
    /// standard puzzle.
    fn build_non_standard_coin_spend(
        parent_coin_info: Bytes32,
        amount: u64,
        conditions: &Conditions,
    ) -> CoinSpend {
        let mut ctx = SpendContext::new();
        let puzzle = ctx.alloc(&1u8).expect("alloc puzzle");
        let solution = ctx.alloc(conditions).expect("alloc solution");
        let puzzle_hash: Bytes32 = ctx.tree_hash(puzzle).into();
        CoinSpend::new(
            Coin::new(parent_coin_info, puzzle_hash, amount),
            ctx.serialize(&puzzle).expect("serialize puzzle"),
            ctx.serialize(&solution).expect("serialize solution"),
        )
    }

    /// The tweak points of the given single-input and multi-input groups.
    fn expected_points(groups: &[&[(PublicKey, Bytes32)]]) -> Vec<[u8; 48]> {
        groups
            .iter()
            .map(|group| {
                let mut a_sum = group[0].0;
                for (key, _) in &group[1..] {
                    a_sum += key;
                }
                let coin_ids: Vec<Bytes32> = group.iter().map(|(_, id)| *id).collect();
                tweak_point_from_input_hash(&a_sum, &compute_input_hash(&coin_ids, &a_sum))
                    .unwrap()
                    .to_bytes()
            })
            .collect()
    }

    fn sorted_points(td: &TweakData) -> Vec<[u8; 48]> {
        let mut points: Vec<[u8; 48]> = td.tweak_points.iter().map(PublicKey::to_bytes).collect();
        points.sort_unstable();
        points
    }

    /// CHIP-0057 "Required Behaviors": a spend that is not an eligible spend
    /// contributes no key, no coin id, and no edge, even when it outputs
    /// `ASSERT_CONCURRENT_SPEND` conditions.
    ///
    /// Coins `a` and `b` form a cycle. A non-standard coin `x` asserts `a` and
    /// is asserted by `a`, which would put it in the cycle if it counted. The
    /// tweak points are exactly those of the block without `x`.
    #[test]
    fn non_eligible_spend_contributes_nothing() {
        let pk_a = chia_bls::SecretKey::from_seed(&[0x31u8; 32]).public_key();
        let pk_b = chia_bls::SecretKey::from_seed(&[0x32u8; 32]).public_key();
        let parent_a: Bytes32 = [0xa1u8; 32].into();
        let parent_b: Bytes32 = [0xb1u8; 32].into();
        let parent_x: Bytes32 = [0xc1u8; 32].into();
        let id_a = Coin::new(parent_a, StandardArgs::curry_tree_hash(pk_a).into(), 1).coin_id();
        let id_b = Coin::new(parent_b, StandardArgs::curry_tree_hash(pk_b).into(), 1).coin_id();

        // `x` asserts `a`. Its coin id does not depend on its solution.
        let spend_x = build_non_standard_coin_spend(
            parent_x,
            1,
            &Conditions::new().assert_concurrent_spend(id_a),
        );
        let id_x = spend_x.coin.coin_id();
        assert_ne!(spend_x.coin.puzzle_hash, Bytes32::default());

        let spend_a = build_standard_coin_spend(
            pk_a,
            parent_a,
            1,
            Conditions::new()
                .assert_concurrent_spend(id_b)
                .assert_concurrent_spend(id_x),
        );
        let spend_b = build_standard_coin_spend(
            pk_b,
            parent_b,
            1,
            Conditions::new().assert_concurrent_spend(id_a),
        );

        let with_x =
            tweak_data_from_block_spends(&[spend_x, spend_a.clone(), spend_b.clone()], &[])
                .unwrap();
        let without_x = tweak_data_from_block_spends(&[spend_a, spend_b], &[]).unwrap();

        let mut expected = expected_points(&[
            &[(pk_a, id_a)],
            &[(pk_b, id_b)],
            &[(pk_a, id_a), (pk_b, id_b)],
        ]);
        expected.sort_unstable();
        assert_eq!(sorted_points(&with_x), expected);
        assert_eq!(sorted_points(&without_x), expected);
    }

    /// CHIP-0057 sender requirement 5: a cycle that passes through a coin that
    /// is not an eligible spend does not bind the eligible coins on either
    /// side of it. With `a -> x -> b -> a` and `x` non-standard, `a` and `b`
    /// are single-input groups only.
    #[test]
    fn cycle_through_non_eligible_spend_does_not_bind() {
        let pk_a = chia_bls::SecretKey::from_seed(&[0x33u8; 32]).public_key();
        let pk_b = chia_bls::SecretKey::from_seed(&[0x34u8; 32]).public_key();
        let parent_a: Bytes32 = [0xa2u8; 32].into();
        let parent_b: Bytes32 = [0xb2u8; 32].into();
        let parent_x: Bytes32 = [0xc2u8; 32].into();
        let id_a = Coin::new(parent_a, StandardArgs::curry_tree_hash(pk_a).into(), 1).coin_id();
        let id_b = Coin::new(parent_b, StandardArgs::curry_tree_hash(pk_b).into(), 1).coin_id();

        let spend_x = build_non_standard_coin_spend(
            parent_x,
            1,
            &Conditions::new().assert_concurrent_spend(id_b),
        );
        let id_x = spend_x.coin.coin_id();
        let spend_a = build_standard_coin_spend(
            pk_a,
            parent_a,
            1,
            Conditions::new().assert_concurrent_spend(id_x),
        );
        let spend_b = build_standard_coin_spend(
            pk_b,
            parent_b,
            1,
            Conditions::new().assert_concurrent_spend(id_a),
        );

        let td = tweak_data_from_block_spends(&[spend_a, spend_x, spend_b], &[]).unwrap();

        let mut expected = expected_points(&[&[(pk_a, id_a)], &[(pk_b, id_b)]]);
        expected.sort_unstable();
        assert_eq!(sorted_points(&td), expected);
    }
}
