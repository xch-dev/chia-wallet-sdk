use chia_bls::{aggregate_verify_gt, hash_to_g2};
use chia_consensus::{
    allocator::make_allocator, consensus_constants::ConsensusConstants,
    owned_conditions::OwnedSpendBundleConditions, spendbundle_conditions::run_spendbundle,
    validation_error::ErrorCode,
};
use chia_protocol::SpendBundle;
use chia_sha2::Sha256;
use clvmr::LIMIT_HEAP;

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
