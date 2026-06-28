use chia_bls::{PublicKey, aggregate_verify_gt, hash_to_g2};
use chia_consensus::{
    allocator::make_allocator,
    conditions::{
        ELIGIBLE_FOR_DEDUP, MempoolVisitor, ParseState, SpendBundleConditions,
        process_single_spend, validate_conditions,
    },
    consensus_constants::ConsensusConstants,
    flags::COMPUTE_FINGERPRINT,
    owned_conditions::OwnedSpendBundleConditions,
    puzzle_fingerprint::compute_puzzle_fingerprint,
    run_block_generator::subtract_cost,
    solution_generator::calculate_generator_length,
    validation_error::{ErrorCode, ValidationErr},
};
use chia_protocol::{Bytes, SpendBundle};
use chia_sdk_types::run_puzzle_with_cost;
use chia_sha2::Sha256;
use clvm_utils::tree_hash;
use clvmr::{Allocator, LIMIT_HEAP, reduction::Reduction, serde::node_from_bytes};

// TODO: This function is copied here because WASM doesn't support std::time::Instant
// Should this be changed upstream?
pub fn validate_clvm_and_signature(
    spend_bundle: &SpendBundle,
    max_cost: u64,
    constants: &ConsensusConstants,
    flags: u32,
) -> Result<OwnedSpendBundleConditions, ErrorCode> {
    let mut a = make_allocator(LIMIT_HEAP);
    let (sbc, pkm_pairs) =
        run_spendbundle(&mut a, spend_bundle, max_cost, flags, constants).map_err(|e| e.1)?;
    let conditions = OwnedSpendBundleConditions::from(&a, sbc);

    // Collect all pairs in a single vector to avoid multiple iterations
    let mut pairs = Vec::new();

    let mut aug_msg = Vec::<u8>::new();

    for (pk, msg) in pkm_pairs {
        aug_msg.clear();
        aug_msg.extend_from_slice(&pk.to_bytes());
        aug_msg.extend(&*msg);
        let aug_hash = hash_to_g2(&aug_msg);
        let pairing = aug_hash.pair(&pk);

        let mut key = Sha256::new();
        key.update(&aug_msg);
        pairs.push((key.finalize(), pairing));
    }
    // Verify aggregated signature
    let result = aggregate_verify_gt(
        &spend_bundle.aggregated_signature,
        pairs.iter().map(|tuple| &tuple.1),
    );
    if !result {
        return Err(ErrorCode::BadAggregateSignature);
    }

    // Collect results
    Ok(conditions)
}

// TODO: This function is copied here because the upstream doesn't support custom dialects, and
// we want to use the debug dialect for testing Rue puzzles in the simulator.
// Should this be changed upstream?
#[allow(clippy::type_complexity)]
pub fn run_spendbundle(
    a: &mut Allocator,
    spend_bundle: &SpendBundle,
    max_cost: u64,
    flags: u32,
    constants: &ConsensusConstants,
) -> Result<(SpendBundleConditions, Vec<(PublicKey, Bytes)>), ValidationErr> {
    // below is an adapted version of the code from run_block_generators::run_block_generator2()
    // it assumes no block references are passed in
    let mut cost_left = max_cost;
    let mut ret = SpendBundleConditions::default();
    let mut state = ParseState::default();
    // We don't pay the size cost (nor execution cost) of being wrapped by a
    // quote (in solution_generator).
    let generator_length_without_quote = calculate_generator_length(&spend_bundle.coin_spends) - 2;

    let byte_cost = generator_length_without_quote as u64 * constants.cost_per_byte;
    subtract_cost(a, &mut cost_left, byte_cost)?;

    for coin_spend in &spend_bundle.coin_spends {
        // process the spend
        let puz = node_from_bytes(a, coin_spend.puzzle_reveal.as_slice())?;
        let sol = node_from_bytes(a, coin_spend.solution.as_slice())?;
        let parent = a.new_atom(coin_spend.coin.parent_coin_info.as_slice())?;
        let amount = a.new_number(coin_spend.coin.amount.into())?;
        let Reduction(clvm_cost, conditions) = run_puzzle_with_cost(a, puz, sol, cost_left, false)?;

        ret.execution_cost += clvm_cost;
        subtract_cost(a, &mut cost_left, clvm_cost)?;

        let buf = tree_hash(a, puz);
        if coin_spend.coin.puzzle_hash != buf.into() {
            return Err(ValidationErr(puz, ErrorCode::WrongPuzzleHash));
        }
        let puzzle_hash = a.new_atom(&buf)?;
        let spend = process_single_spend::<MempoolVisitor>(
            a,
            &mut ret,
            &mut state,
            parent,
            puzzle_hash,
            amount,
            conditions,
            flags,
            &mut cost_left,
            clvm_cost,
            constants,
        )?;

        if (spend.flags & ELIGIBLE_FOR_DEDUP) != 0 && (flags & COMPUTE_FINGERPRINT) != 0 {
            spend.fingerprint = compute_puzzle_fingerprint(a, conditions)?;
        }
    }

    validate_conditions(a, &ret, &state, a.nil(), flags)?;

    assert!(max_cost >= cost_left);
    ret.cost = max_cost - cost_left;
    Ok((ret, state.pkm_pairs))
}
