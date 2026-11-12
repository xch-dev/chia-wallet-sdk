use super::*;
use chia_bls::{aggregate, verify};
use chia_puzzle_types::standard::StandardSolution;
use chia_sdk_types::{
    Conditions, Mod, conditions::ReserveFee, puzzles::InternalCustodyMerkleSolution,
};
use clvm_traits::{ToClvm, clvm_quote};
use clvmr::Allocator;
use serde_json::Value;

type Solution = SingletonSolution<InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr>>;
#[derive(FromClvm, ToClvm)]
#[clvm(list)]
struct FilterArgs<S> {
    standard: S,
    context: (Bytes32, Bytes32),
}

fn ctx() -> SpendContext {
    Allocator::new_limited(64 * 1024 * 1024).into()
}
fn bytes<const N: usize>(v: &Value) -> [u8; N] {
    hex::decode(v.as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap()
}
fn fixture() -> (
    InternalCustodyPolicy,
    CoinSpend,
    Vec<PublicKey>,
    AggSigConstants,
    Value,
) {
    // Public test vectors generated independently by Python internal-custody/hsms.
    let request: Value = serde_json::from_str(include_str!("tests/payment.json")).unwrap();
    let op = &request["operation"];
    let cs = &op["coin_spends"][0];
    let policy = InternalCustodyPolicy::from_legacy_bytes(
        &hex::decode(op["vault"]["root_derivation_hex"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let coin = Coin::new(
        bytes::<32>(&cs["coin"]["parent_coin_info"]).into(),
        bytes::<32>(&cs["coin"]["puzzle_hash"]).into(),
        cs["coin"]["amount"].as_str().unwrap().parse().unwrap(),
    );
    let spend = CoinSpend::new(
        coin,
        hex::decode(cs["puzzle_reveal"].as_str().unwrap())
            .unwrap()
            .into(),
        hex::decode(cs["solution"].as_str().unwrap())
            .unwrap()
            .into(),
    );
    let keys = op["selected_signers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| PublicKey::from_bytes(&bytes(v)).unwrap())
        .collect();
    (
        policy,
        spend,
        keys,
        AggSigConstants::new(bytes::<32>(&op["network"]["agg_sig_me_additional_data"]).into()),
        request,
    )
}
fn parse(
    policy: &InternalCustodyPolicy,
    spend: &CoinSpend,
    keys: &[PublicKey],
    constants: &AggSigConstants,
) -> Result<VerifiedInternalCustodyPayment, DriverError> {
    parse_internal_custody_payment(
        &mut ctx(),
        policy,
        std::slice::from_ref(spend),
        keys,
        constants,
        500_000_000,
    )
}
fn key(index: usize) -> SecretKey {
    let mut hash = chia_sha2::Sha256::new();
    hash.update(format!("custody-app-fixture-{index}"));
    let mnemonic = bip39::Mnemonic::from_entropy(&hash.finalize()).unwrap();
    SecretKey::from_seed(&mnemonic.to_seed(""))
}
#[test]
fn reference_payment_layer_construction_and_signatures() -> anyhow::Result<()> {
    let (policy, spend, keys, constants, request) = fixture();
    let verified = parse(&policy, &spend, &keys, &constants)?;
    let facts = verified.payment();
    assert_eq!(
        facts.pending_payment.map(|p| p.coin).unwrap().amount,
        2_000_000_000
    );
    assert_eq!(
        facts.input_coin.amount - facts.pending_payment.map(|p| p.coin).unwrap().amount,
        facts.vault_child.amount
    );
    assert_eq!(facts.clawback_seconds, 2);
    assert_eq!(facts.withdrawal_seconds, 1);
    let oracle: Value = serde_json::from_str(include_str!("tests/expected.json"))?;
    assert_eq!(
        facts.pending_payment.unwrap().destination,
        bytes::<32>(&oracle["destination_puzzle_hash"]).into()
    );
    assert_eq!(
        facts.input_coin.coin_id(),
        bytes::<32>(&oracle["singleton_coin_id"]).into()
    );
    for expected in oracle["signing_context"].as_array().unwrap() {
        assert_eq!(
            verified.required_signature().public_key.to_bytes(),
            bytes::<48>(&expected["final_public_key"])
        );
        assert_eq!(
            verified.required_signature().message(),
            hex::decode(expected["message"].as_str().unwrap())?
        );
    }
    let ctx = &mut ctx();
    let solution = spend.solution.to_clvm(&mut **ctx)?;
    let solution = SingletonSolution::<NodePtr>::from_clvm(&**ctx, solution)?;
    let rebuilt = policy.payment_spends(
        ctx,
        spend.coin,
        solution.lineage_proof,
        Some(crate::InternalCustodyWithdrawal {
            amount: facts.outgoing_amount(),
            destination: facts.pending_payment.unwrap().destination,
        }),
        &keys,
        &[],
    )?;
    assert_eq!(vec![spend.clone()], rebuilt);
    let layer = policy.inner_layer(ctx)?;
    let puzzle = layer.construct_puzzle(ctx)?;
    assert_eq!(
        InternalCustodyLayer::parse_puzzle(ctx, Puzzle::parse(ctx, puzzle))?,
        Some(layer)
    );
    let mut signatures = Vec::new();
    for secret in (0..3).map(key) {
        if !keys.contains(&secret.public_key()) {
            continue;
        }
        let partial = verified.sign_partial(&secret)?;
        assert!(verified.verify_partial(&secret.public_key(), &partial));
        assert!(!verified.verify_partial(&key(42).public_key(), &partial));
        assert!(!verify(
            &partial,
            &secret.public_key(),
            verified.required_signature().message()
        ));
        signatures.push(partial);
    }
    assert_eq!(signatures.len(), 2);
    assert!(verified.sign_partial(&key(42)).is_err());
    let offset = SecretKey::from_bytes(&bytes::<32>(
        &request["operation"]["sum_hints"][0]["synthetic_offset"],
    ))?;
    assert!(verified.verify_synthetic_offset(&offset));
    assert!(!verified.verify_synthetic_offset(&key(42)));
    signatures.push(verified.required_signature().sign_partial(&offset)?);
    assert!(verify(
        &aggregate(&signatures),
        &verified.required_signature().public_key,
        verified.required_signature().message()
    ));
    Ok(())
}
#[test]
fn rejects_unreviewed_effects_and_invalid_lineage() -> anyhow::Result<()> {
    let (policy, spend, keys, constants, _) = fixture();
    let mut wrong = spend.clone();
    wrong.coin.parent_coin_info = [0; 32].into();
    assert!(matches!(
        parse(&policy, &wrong, &keys, &constants),
        Err(DriverError::InternalCustody(
            InternalCustodyError::CoinAssertion
        ))
    ));
    let mut wrong = spend.clone();
    wrong.coin.puzzle_hash = [0; 32].into();
    assert!(parse(&policy, &wrong, &keys, &constants).is_err());
    let ctx = &mut ctx();
    let node = spend.solution.to_clvm(&mut **ctx)?;
    let original = Solution::from_clvm(&**ctx, node)?;
    let mut claimed = original.clone();
    let mut action =
        InternalCustodyPaymentArguments::from_clvm(&**ctx, claimed.inner_solution.action)?;
    action.incoming = 2;
    claimed.inner_solution.action = ctx.alloc(&action)?;
    let mut wrong = spend.clone();
    wrong.solution = ctx.serialize(&claimed)?;
    assert!(parse(&policy, &wrong, &keys, &constants).is_err());
    let mut rekey = original.clone();
    rekey.inner_solution.spend_type = 1;
    wrong.solution = ctx.serialize(&rekey)?;
    assert!(parse(&policy, &wrong, &keys, &constants).is_err());
    // Add a fee inside the delegated puzzle while preserving the payment-looking action.
    let mut hidden = original;
    let mut filter = InternalCustodyMerkleSolution::<NodePtr, NodePtr>::from_clvm(
        &**ctx,
        hidden.inner_solution.filter_solution,
    )?;
    let mut args =
        FilterArgs::<StandardSolution<NodePtr, NodePtr>>::from_clvm(&**ctx, filter.solution)?;
    let conditions = ctx.run(args.standard.delegated_puzzle, args.standard.solution)?;
    let conditions = Conditions::<NodePtr>::from_clvm(&**ctx, conditions)?.with(ReserveFee::new(1));
    args.standard.delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
    filter.solution = ctx.alloc(&args)?;
    hidden.inner_solution.filter_solution = ctx.alloc(&filter)?;
    wrong.solution = ctx.serialize(&hidden)?;
    assert!(matches!(
        parse(&policy, &wrong, &keys, &constants),
        Err(DriverError::InternalCustody(
            InternalCustodyError::UnexpectedConditions
        ))
    ));
    Ok(())
}
#[test]
fn rejects_bad_subsets_programs_and_resource_limits() {
    let (policy, spend, keys, constants, _) = fixture();
    let mut reversed = keys.clone();
    reversed.reverse();
    assert!(parse(&policy, &spend, &reversed, &constants).is_ok());
    assert!(parse(&policy, &spend, &[keys[0], keys[0]], &constants).is_err());
    assert!(parse(&policy, &spend, &[keys[0]], &constants).is_err());
    assert!(
        parse_internal_custody_payment(&mut ctx(), &policy, &[], &keys, &constants, 500_000_000)
            .is_err()
    );
    assert!(
        parse_internal_custody_payment(
            &mut ctx(),
            &policy,
            &[spend.clone(), spend.clone()],
            &keys,
            &constants,
            500_000_000
        )
        .is_err()
    );
    assert!(
        parse_internal_custody_payment(
            &mut ctx(),
            &policy,
            std::slice::from_ref(&spend),
            &keys,
            &constants,
            1
        )
        .is_err()
    );
    let mut wrong = spend.clone();
    wrong.solution = vec![0; 256 * 1024 + 1].into();
    assert!(parse(&policy, &wrong, &keys, &constants).is_err());
    let raw = spend.solution.as_ref();
    for at in (0..raw.len()).step_by(31) {
        let mut modified = raw.to_vec();
        modified[at] ^= 1;
        wrong = spend.clone();
        wrong.solution = modified.into();
        assert!(
            parse(&policy, &wrong, &keys, &constants).is_err(),
            "solution byte {at}"
        );
    }
}
#[test]
fn legacy_policy_and_proof_bounds_fail_closed() {
    let (policy, _, _, _, request) = fixture();
    let raw = hex::decode(
        request["operation"]["vault"]["root_derivation_hex"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for end in (0..raw.len()).step_by(13) {
        assert!(InternalCustodyPolicy::from_legacy_bytes(&raw[..end]).is_err());
    }
    let mut extra = raw;
    extra.push(0);
    assert!(InternalCustodyPolicy::from_legacy_bytes(&extra).is_err());
    assert!(
        InternalCustodyPolicy::new(
            policy.info(),
            vec![PublicKey::default()],
            policy.thresholds(),
            None,
            vec![],
            vec![]
        )
        .is_err()
    );
}

#[test]
fn completion_preserves_destination_and_requires_the_clawback_delay() -> anyhow::Result<()> {
    let (policy, spend, keys, constants, _) = fixture();
    let verified = parse(&policy, &spend, &keys, &constants)?;
    let facts = verified.payment();
    let ctx = &mut ctx();
    let completion = policy.completion_spend(
        ctx,
        facts.pending_payment.map(|p| p.coin).unwrap(),
        facts.pending_payment.unwrap().destination,
    )?;
    let puzzle = completion.puzzle_reveal.to_clvm(&mut **ctx)?;
    let solution = completion.solution.to_clvm(&mut **ctx)?;
    let output = run_program(
        ctx,
        &ChiaDialect::new(clvmr::ClvmFlags::empty()),
        puzzle,
        solution,
        500_000_000,
    )?
    .1;
    let conditions = Vec::<Condition>::from_clvm(&**ctx, output)?;
    assert_eq!(conditions.len(), 4);
    assert!(conditions.iter().any(
        |c| matches!(c,Condition::AssertSecondsRelative(c) if c.seconds == facts.clawback_seconds)
    ));
    assert!(conditions.iter().any(|c| matches!(c,Condition::CreateCoin(c) if c.puzzle_hash == facts.pending_payment.unwrap().destination && c.amount == facts.pending_payment.map(|p| p.coin).unwrap().amount)));
    assert!(
        !conditions
            .iter()
            .any(|c| c.clone().into_agg_sig().is_some())
    );
    assert!(
        policy
            .completion_spend(
                ctx,
                facts.pending_payment.map(|p| p.coin).unwrap(),
                [0; 32].into()
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn verifies_absorption_and_claim_only_with_exact_bound_inputs() -> anyhow::Result<()> {
    let (policy, spend, keys, constants, _) = fixture();
    let ctx = &mut ctx();
    let node = spend.solution.to_clvm(&mut **ctx)?;
    let original = Solution::from_clvm(&**ctx, node)?;
    let ph = chia_sdk_types::puzzles::P2SingletonArgs::new(policy.info().launcher_id)
        .curry_tree_hash()
        .into();
    // Odd coins are valid individually; their combined amount must be even.
    let mut deposits = vec![
        Coin::new([7; 32].into(), ph, 101),
        Coin::new([8; 32].into(), ph, 103),
    ];
    deposits.sort_by_key(Coin::coin_id);
    for outgoing in [0, 2_000_000_000] {
        let destination = if outgoing == 0 {
            Bytes32::default()
        } else {
            [9; 32].into()
        };
        let spends = policy.payment_spends(
            ctx,
            spend.coin,
            original.lineage_proof,
            (outgoing > 0).then_some(crate::InternalCustodyWithdrawal {
                amount: outgoing,
                destination,
            }),
            &keys,
            &deposits,
        )?;
        let verified =
            parse_internal_custody_payment(ctx, &policy, &spends, &keys, &constants, 500_000_000)?;
        let facts = verified.payment();
        assert_eq!(facts.incoming_amount, 204);
        assert_eq!(facts.outgoing_amount(), outgoing);
        assert_eq!(facts.deposit_count, 2);
        assert_eq!(facts.vault_child.amount, spend.coin.amount + 204 - outgoing);
        assert_eq!(
            facts.pending_payment.map(|p| p.coin).is_none(),
            outgoing == 0
        );
        assert!(
            parse_internal_custody_payment(
                ctx,
                &policy,
                &spends[..2],
                &keys,
                &constants,
                500_000_000
            )
            .is_err()
        );
        let mut wrong = spends.clone();
        wrong[1].coin.amount += 2;
        assert!(
            parse_internal_custody_payment(ctx, &policy, &wrong, &keys, &constants, 500_000_000)
                .is_err()
        );
        let mut wrong = spends.clone();
        wrong[1].solution = vec![0x80].into();
        assert!(
            parse_internal_custody_payment(ctx, &policy, &wrong, &keys, &constants, 500_000_000)
                .is_err()
        );
        let mut wrong = spends.clone();
        wrong.push(spends[1].clone());
        assert!(
            parse_internal_custody_payment(ctx, &policy, &wrong, &keys, &constants, 500_000_000)
                .is_err()
        );
    }
    for bad in [
        vec![deposits[0]],
        vec![deposits[0], deposits[0]],
        vec![Coin::new([1; 32].into(), [2; 32].into(), 200)],
        vec![Coin::new([1; 32].into(), ph, u64::MAX - 1)],
    ] {
        assert!(
            policy
                .payment_spends(ctx, spend.coin, original.lineage_proof, None, &keys, &bad)
                .is_err()
        );
    }
    assert!(
        policy
            .payment_spends(ctx, spend.coin, original.lineage_proof, None, &keys, &[])
            .is_ok()
    );
    assert!(
        policy
            .payment_spends(
                ctx,
                spend.coin,
                original.lineage_proof,
                Some(crate::InternalCustodyWithdrawal {
                    amount: 0,
                    destination: [9; 32].into()
                }),
                &keys,
                &deposits
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn absorption_and_claim_only_pass_independent_consensus() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::TESTNET11_CONSTANTS;
    let (policy, original, keys, _, request) = fixture();
    let constants = AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data);
    let offset = SecretKey::from_bytes(&bytes::<32>(
        &request["operation"]["sum_hints"][0]["synthetic_offset"],
    ))?;
    for outgoing in [0, original.coin.amount + 101] {
        let ctx = &mut ctx();
        let node = original.solution.to_clvm(&mut **ctx)?;
        let lineage = Solution::from_clvm(&**ctx, node)?.lineage_proof;
        let mut sim = Simulator::new();
        sim.insert_coin(original.coin);
        let deposit_hash = chia_sdk_types::puzzles::P2SingletonArgs::new(policy.info().launcher_id)
            .curry_tree_hash()
            .into();
        let mut deposits = vec![
            sim.new_coin(deposit_hash, 101),
            sim.new_coin(deposit_hash, 103),
        ];
        deposits.sort_by_key(Coin::coin_id);
        sim.create_block();
        sim.pass_time(policy.info().withdrawal_timelock + 1);
        let withdrawal = (outgoing > 0).then_some(crate::InternalCustodyWithdrawal {
            amount: outgoing,
            destination: [9; 32].into(),
        });
        let spends =
            policy.payment_spends(ctx, original.coin, lineage, withdrawal, &keys, &deposits)?;
        let verified =
            parse_internal_custody_payment(ctx, &policy, &spends, &keys, &constants, 500_000_000)?;
        let facts = *verified.payment();
        let mut shares = Vec::new();
        for secret in (0..3).map(key).filter(|k| keys.contains(&k.public_key())) {
            shares.push(verified.sign_partial(&secret)?);
        }
        shares.push(verified.required_signature().sign_partial(&offset)?);
        let signature = aggregate(&shares);
        assert!(
            sim.new_transaction(SpendBundle::new(vec![spends[0].clone()], signature.clone()))
                .is_err()
        );
        assert!(
            sim.new_transaction(SpendBundle::new(spends[1..].to_vec(), Signature::default()))
                .is_err()
        );
        assert!(
            sim.new_transaction(SpendBundle::new(spends.clone(), Signature::default()))
                .is_err()
        );
        sim.new_transaction(SpendBundle::new(spends, signature))?;
        assert_eq!(
            sim.children(original.coin.coin_id()).len(),
            if outgoing == 0 { 1 } else { 2 }
        );
        assert!(
            sim.coin_state(facts.vault_child.coin_id())
                .unwrap()
                .created_height
                .is_some()
        );
        assert_eq!(
            facts.vault_child.amount,
            original.coin.amount + 204 - outgoing
        );
        for deposit in deposits {
            assert!(
                sim.coin_state(deposit.coin_id())
                    .unwrap()
                    .spent_height
                    .is_some()
            );
        }
        if let Some(pending) = facts.pending_payment {
            let complete = policy.completion_spend(ctx, pending.coin, pending.destination)?;
            assert!(
                sim.new_transaction(SpendBundle::new(
                    vec![complete.clone()],
                    Signature::default()
                ))
                .is_err()
            );
            sim.pass_time(facts.clawback_seconds + 1);
            sim.new_transaction(SpendBundle::new(vec![complete], Signature::default()))?;
            let payout = Coin::new(pending.coin.coin_id(), pending.destination, outgoing);
            assert!(
                sim.coin_state(payout.coin_id())
                    .unwrap()
                    .created_height
                    .is_some()
            );
        } else {
            assert_eq!(outgoing, 0);
        }
    }
    Ok(())
}

#[test]
fn execution_budget_cannot_become_unlimited() -> anyhow::Result<()> {
    let (policy, original, keys, constants, _) = fixture();
    let ctx = &mut ctx();
    let node = original.solution.to_clvm(&mut **ctx)?;
    let lineage = Solution::from_clvm(&**ctx, node)?.lineage_proof;
    let hash = chia_sdk_types::puzzles::P2SingletonArgs::new(policy.info().launcher_id)
        .curry_tree_hash()
        .into();
    let mut deposits = vec![
        Coin::new([1; 32].into(), hash, 2),
        Coin::new([2; 32].into(), hash, 2),
    ];
    deposits.sort_by_key(Coin::coin_id);
    let spends = policy.payment_spends(ctx, original.coin, lineage, None, &keys, &deposits)?;
    let mut costs = Vec::new();
    for spend in &spends {
        let puzzle = canonical_program(ctx, spend.puzzle_reveal.as_ref())?;
        let solution = canonical_program(ctx, spend.solution.as_ref())?;
        costs.push(
            run_program(
                ctx,
                &ChiaDialect::new(ClvmFlags::empty()),
                puzzle,
                solution,
                500_000_000,
            )?
            .0,
        );
    }
    let total: u64 = costs.iter().sum();
    // Exhaustion before another deposit and before the singleton must both fail.
    for budget in [0, costs[1], costs[1] + costs[2], total - 1] {
        assert!(
            parse_internal_custody_payment(ctx, &policy, &spends, &keys, &constants, budget)
                .is_err(),
            "budget {budget}"
        );
    }
    for budget in [total, total + 1] {
        assert!(
            parse_internal_custody_payment(ctx, &policy, &spends, &keys, &constants, budget)
                .is_ok(),
            "budget {budget}"
        );
    }
    assert!(
        parse_internal_custody_payment(ctx, &policy, &[original], &keys, &constants, 0).is_err()
    );
    Ok(())
}
