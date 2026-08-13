//! Real-block CHIP-0057 [`TweakData`] builder for any post-decompression caller.
//!
//! Given a block's `Vec<CoinSpend>` (removals with puzzle reveals + solutions)
//! and `Vec<Coin>` (additions list), groups standard-puzzle spends into the
//! CHIP-0057 transaction-group shape that scanners detect against, then emits
//! one `tweak_point = input_hash * A_sum` per group plus paired
//! [`OutputMeta`] per addition. Generator decompression is the caller's
//! responsibility — the helper accepts the natural post-decompression shape so
//! it stays a pure function with no `chia-consensus` dependency.
//!
//! ## Grouping algorithm
//!
//! Two grouping passes run independently over the full set of standard
//! removals, mirroring the CHIP-0057 `ScanBlock` procedure (Pass 1 +
//! Pass 2). The passes are **additive and overlapping**, not a partition:
//! every standard-puzzle spend can appear in more than one candidate group, and
//! no pass consumes or excludes a coin from any other pass. A single coin always
//! contributes its own Pass-1 singleton, and may additionally appear in a Pass-2
//! concurrent-spend SCC.
//!
//! - **Stage 1 — defensive standard-puzzle filter.** Each [`CoinSpend`] is
//!   parsed via [`StandardLayer::parse_puzzle`]; non-standard puzzles (CAT,
//!   NFT, arbitrary mod hashes) skip silently.
//! - **Pass 1 — per-spend singletons.** Every standard-puzzle spend `i` emits a
//!   singleton candidate group `[i]`. This is the sole single-input detector (a
//!   lone coin never forms a Pass-2 SCC of size >= 2), so it runs
//!   unconditionally for all spends.
//! - **Pass 2 — `AssertConcurrentSpend` SCC over ALL removals.** Every
//!   standard-puzzle spend's puzzle+solution is executed via
//!   [`chia_sdk_types::run_puzzle`] to extract conditions; opcode-64
//!   `AssertConcurrentSpend` targets become directed edges in a graph over
//!   **all** standard-puzzle spends; iterative Tarjan SCC then groups spends that
//!   form a closed cycle, and each SCC of size 2 or more is emitted as an
//!   additional candidate group. The cycle pattern is exactly what the sender
//!   emits for any multi-input send via `Relation::AssertConcurrent` — including
//!   multiple inputs that happen to share a puzzle hash, since the sender binds
//!   every set of two or more non-ephemeral inputs into one cycle. A multi-input
//!   set that does not carry such a cycle is, by design, not a detectable shape.
//!   Strongly-connected (not weakly-connected) grouping is what defends against
//!   third-party "pollution" assertions pointing at a legitimate-send coin: a
//!   polluter has a forward edge into the cycle but no return edge, so it stays
//!   in its own trivial SCC and does not corrupt the legitimate group's `A_sum`.
//! - **Stage 3 — per-group aggregation + tweak emission.** Each candidate group
//!   computes `A_sum = Σ synthetic_key`,
//!   `input_hash = compute_input_hash(coin_ids, A_sum)`,
//!   `tweak_point = A_sum.scalar_multiply(input_hash)`. BLS12-381
//!   identity-element results are suppressed (CHIP §459). Because the passes
//!   overlap, distinct candidate groups can produce a byte-identical
//!   `tweak_point`; identical results are de-duplicated by the 48-byte compressed
//!   point, keeping the first occurrence. Groups that share a coin but differ in
//!   membership produce different points and both survive — dedup is byte-equality
//!   only, never by coin overlap.
//! - **Stage 4 — outputs.** Each addition becomes one [`OutputMeta`] (no
//!   grouping — outputs land flat in `TweakData.outputs`).
//!
//! ## Group emission order (load-bearing for byte-equality tests)
//!
//! Candidate groups are produced in this stable total order, then de-duplicated
//! by compressed-point bytes keeping the first occurrence:
//!
//! 1. Pass 1 singletons in `coin_spends` input order.
//! 2. Pass 2 SCCs (size >= 2) in Tarjan finishing order.
//!
//! Cross-call regression tests (including the simulator-helper round-trip
//! oracle) depend on this ordering being stable across runs.

use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_sdk_types::{Condition, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};
use indexmap::IndexMap;

use crate::silent_payments::{OutputMeta, TweakData, compute_input_hash};
use crate::{DriverError, Layer, Puzzle, StandardLayer};

/// Parsed-standard-puzzle row carried between stages.
struct StandardSpend {
    coin_id: Bytes32,
    synthetic_pk: PublicKey,
    puzzle: NodePtr,
    solution: NodePtr,
}

/// Build a [`TweakData`] from a real (or simulator) block's coin spends and
/// additions.
///
/// Returns `Err(DriverError)` only on protocol-level corruption (e.g., a
/// downstream invariant in [`compute_input_hash`] failing); non-standard
/// puzzle reveals skip silently, never panicking or erroring.
///
/// See module-level docs for the grouping algorithm and ordering contract.
///
/// # Errors
///
/// Returns [`DriverError`] from downstream protocol primitives. The current
/// implementation never produces an error in practice; the `Result` return
/// shape reserves room for future protocol-level validation without an
/// API break.
pub fn tweak_data_from_block_spends(
    coin_spends: &[CoinSpend],
    additions: &[Coin],
) -> Result<TweakData, DriverError> {
    let mut allocator = Allocator::new();

    // Stage 1 — defensive standard-puzzle filter.
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

    // Pass 2 — `AssertConcurrentSpend` SCC over ALL standard spends. The graph
    // and the coin-id->position map cover every spend index, so a cycle spanning
    // distinct puzzle hashes (including multiple inputs that share a puzzle hash)
    // still forms a single SCC.
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

    // Stage 3 — per-group aggregation + tweak_point emission, de-duplicated by
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
        let input_hash = compute_input_hash(&coin_ids, &a_sum);
        let mut tweak_point = a_sum;
        tweak_point.scalar_multiply(&input_hash.to_bytes());
        if tweak_point.is_inf() {
            continue;
        }
        if seen.insert(tweak_point.to_bytes()) {
            tweak_points.push(tweak_point);
        }
    }

    // Stage 4 — pair additions with OutputMeta (no grouping; flat Vec).
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
    /// silently at Stage 1; no tweak point is emitted and the helper does not
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
    /// the CHIP §459 guard suppresses emission so `tweak_points` stays empty.
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
    /// more non-ephemeral inputs, even when they share a puzzle hash).
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
        // each spend can assert the other (the cyclic opcode-64 binding).
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

    /// Regression for mixed-puzzle-hash multi-input fragmentation.
    ///
    /// Three coins bound by ONE `AssertConcurrent` cycle: two coins share `PH_x`
    /// (both curried over the SAME synthetic key, so identical puzzle hash) and a
    /// third sits at `PH_y`. The cycle is the exact shape the sender emits — each
    /// coin asserts its predecessor, coin 0 closes the cycle to coin N-1.
    ///
    /// The Pass-2 SCC is built over ALL standard removals, so all three coins
    /// form one strongly connected component and the aggregate
    /// `A_sum = pk_dup + pk_dup + pk_solo` over all three coin ids is emitted as
    /// a `tweak_point`. We compute that 3-coin-aggregate point by hand and assert
    /// it is PRESENT in the output. Building the graph over every removal (rather
    /// than excluding any same-puzzle-hash subset) is what lets the `PH_y` coin's
    /// edge into a `PH_x` coin resolve and close the 3-coin cycle.
    #[test]
    fn test_bug1_mixed_ph_multi_input_full_cycle_detected() {
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

    /// Regression for a single-input send sharing a puzzle hash with an
    /// unrelated coin.
    ///
    /// Two coins curried over the SAME synthetic key (identical puzzle hash) with
    /// NO `AssertConcurrent` binding: one is a single-input SP send's input, the
    /// other an unrelated standard coin. The SP input must still be detected via
    /// its Pass-1 singleton (`A_sum = K_send` over its single coin id).
    ///
    /// Because Pass 1 emits a singleton for EVERY standard spend unconditionally,
    /// a single-input send is detected even when it collides on puzzle hash with
    /// an unrelated coin. With no cycle present, Pass 2 forms no SCC of size >= 2,
    /// so the only detectable shape is each coin's own singleton.
    #[test]
    fn test_bug2_single_input_sharing_ph_detected_via_singleton() {
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
}
