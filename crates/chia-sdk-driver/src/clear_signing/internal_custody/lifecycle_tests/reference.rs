use super::*;

fn build(
    ctx: &mut SpendContext,
    v: &Value,
    old: &InternalCustodyPolicy,
    new: &InternalCustodyPolicy,
) -> anyhow::Result<Vec<CoinSpend>> {
    let input = spends(v);
    let signers = keys(v);
    let first = &input[0];
    let fee = v["fee_announcement"].as_bool().unwrap();
    Ok(match v["kind"].as_str().unwrap() {
        "payment" => {
            let node = canonical_program(ctx, first.solution.as_ref())?;
            let raw = SingletonSolution::<
                InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr, NodePtr>,
            >::from_clvm(&**ctx, node)?;
            let action =
                InternalCustodyPaymentArguments::from_clvm(&**ctx, raw.inner_solution.action)?;
            let action = if action.outgoing > 0 {
                InternalCustodyPaymentAction::Withdraw(InternalCustodyWithdrawal {
                    amount: action.outgoing,
                    destination: action.destination,
                })
            } else {
                InternalCustodyPaymentAction::Absorb {
                    destination_context: action.destination,
                }
            };
            InternalCustody::new(first.coin, raw.lineage_proof, old.clone()).payment_spends(
                ctx,
                InternalCustodyPaymentOptions {
                    action,
                    fee_announcement: fee,
                },
                &signers,
                &input[1..].iter().map(|s| s.coin).collect::<Vec<_>>(),
            )?
        }
        "clawback" => vec![old.payment_clawback_spend(
            ctx,
            first.coin,
            bytes(&data()["destination"]),
            &signers,
            fee,
        )?],
        "payment-complete" => {
            vec![old.completion_spend(ctx, first.coin, bytes(&data()["destination"]))?]
        }
        "rekey" => {
            let proof = proof(ctx, first);
            vec![old.start_rekey_spend(
                ctx,
                first.coin,
                proof,
                new,
                &signers,
                crate::InternalCustodyRekeyOptions {
                    timelock_multiple: u8::try_from(v["multiple"].as_u64().unwrap()).unwrap(),
                    fee_announcement: fee,
                },
            )?]
        }
        "rekey-cancel" => vec![old.rekey_clawback_spend(
            ctx,
            first.coin,
            rekey_state(
                old,
                v.get("new_root").map_or(new.info().puzzle_root, bytes),
                u8::try_from(v["multiple"].as_u64().unwrap()).unwrap(),
            ),
            &signers,
            fee,
        )?],
        "lock" => {
            let proof = proof(ctx, first);
            let lock = old.increase_security_spends(ctx, first.coin, proof, &signers)?;
            let mut out = vec![lock.authorization];
            if input.len() > 1 {
                out.extend(lock.completion);
            }
            out
        }
        "rekey-complete" | "lock-complete" => {
            let proof = proof(ctx, first);
            let node = canonical_program(ctx, input[1].solution.as_ref())?;
            let drop = InternalCustodyMerkleSolution::<NodePtr, NodePtr>::from_clvm(&**ctx, node)?;
            let (lineage, ()) =
                <(chia_puzzle_types::LineageProof, ())>::from_clvm(&**ctx, drop.solution)?;
            let state = InternalCustodyRekeyState {
                old_root: old.info().puzzle_root,
                new_root: if v["kind"] == "lock-complete" {
                    policy("next_policy").info().puzzle_root
                } else {
                    new.info().puzzle_root
                },
                timelock_multiple: u8::try_from(v["multiple"].as_u64().unwrap()).unwrap(),
            };
            old.complete_rekey_spends(ctx, first.coin, input[1].coin, state, (proof, lineage))?
                .to_vec()
        }
        _ => panic!("unknown operation"),
    })
}
#[test]
fn python_policy_roots_proofs_and_streamable_bytes() -> anyhow::Result<()> {
    for which in ["policy", "new_policy", "next_policy"] {
        let old = policy(which);
        assert_eq!(old.to_reference_bytes(), unhex(&data()[which]));
        let mut keys = old.keys().to_vec();
        keys.reverse();
        let derived =
            InternalCustodyPolicy::derive(&mut ctx(), old.info(), keys, old.thresholds())?;
        assert_eq!(
            derived.to_reference_bytes(),
            old.to_reference_bytes(),
            "{which}"
        );
        old.verify_definition(&mut ctx())?;
    }
    for raw in data()["policy_matrix"].as_array().unwrap() {
        let reference = InternalCustodyPolicy::from_reference_bytes(&unhex(raw))?;
        let derived = InternalCustodyPolicy::derive(
            &mut ctx(),
            reference.info(),
            reference.keys().to_vec(),
            reference.thresholds(),
        )?;
        assert_eq!(derived.to_reference_bytes(), unhex(raw));
    }
    Ok(())
}
#[test]
fn python_spends_conditions_costs_and_signature_contexts() -> anyhow::Result<()> {
    let constants = AggSigConstants::new(bytes(&data()["domain"]));
    let old = policy("policy");
    let new = policy("new_policy");
    let next = policy("next_policy");
    for v in data()["operations"].as_array().unwrap() {
        let name = v["name"].as_str().unwrap();
        let ctx = &mut ctx();
        let input = spends(v);
        let signers = keys(v);
        assert_eq!(
            build(ctx, v, &old, &new)?,
            input,
            "Python byte reconstruction: {name}"
        );
        for (spend, oracle) in input.iter().zip(v["execution"].as_array().unwrap()) {
            let puzzle = canonical_program(ctx, spend.puzzle_reveal.as_ref())?;
            let solution = canonical_program(ctx, spend.solution.as_ref())?;
            let result = run_program(
                ctx,
                &ChiaDialect::new(ClvmFlags::empty()),
                puzzle,
                solution,
                500_000_000,
            )?;
            assert_eq!(
                result.0,
                oracle["cost"].as_u64().unwrap(),
                "Python cost: {name}"
            );
            assert_eq!(
                node_to_bytes(ctx, result.1)?,
                unhex(&oracle["conditions"]),
                "Python conditions: {name}"
            );
        }
        let budget: u64 = v["execution"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["cost"].as_u64().unwrap())
            .sum();
        let signature = if v["kind"] == "payment" {
            let verified = verify_payment(ctx, &old, &input, &signers, &constants, budget)?;
            assert!(verify_payment(ctx, &old, &input, &signers, &constants, budget - 1).is_err());
            verified.required_signature().cloned()
        } else {
            let verified = parse_internal_custody_operation(
                ctx,
                &old,
                &input,
                request(v, &old, &new, &next, &signers),
                &constants,
                budget,
            )?;
            assert!(
                parse_internal_custody_operation(
                    ctx,
                    &old,
                    &input,
                    request(v, &old, &new, &next, &signers),
                    &constants,
                    0
                )
                .is_err()
            );
            assert!(
                parse_internal_custody_operation(
                    ctx,
                    &old,
                    &input,
                    request(v, &old, &new, &next, &signers),
                    &constants,
                    budget - 1
                )
                .is_err()
            );
            assert!(verified.sign_partial(&seed(42)).is_err());
            verified.required_signature().cloned()
        };
        if let Some(required) = signature {
            let oracle = &v["contexts"][0];
            assert_eq!(
                required.public_key.to_bytes().as_slice(),
                unhex(&oracle["public_key"])
            );
            assert_eq!(required.message(), unhex(&oracle["message"]));
            let mut mainnet = unhex(&oracle["raw_message"]);
            mainnet.extend(input[0].coin.coin_id().as_ref());
            mainnet.extend(unhex(&data()["mainnet_domain"]));
            assert_eq!(mainnet, unhex(&v["python_mainnet_message"]));
            for share in oracle["partials"].as_array().unwrap() {
                let key = (1..=6)
                    .map(seed)
                    .find(|k| k.public_key().to_bytes().as_slice() == unhex(&share["public_key"]))
                    .unwrap();
                assert_eq!(
                    required.sign_partial(&key)?.to_bytes().as_slice(),
                    unhex(&share["signature"])
                );
            }
        } else {
            assert!(v["contexts"].as_array().unwrap().is_empty());
        }
    }
    Ok(())
}

#[test]
fn python_filter_rejection_vectors_fail_in_consensus_dialect() -> anyhow::Result<()> {
    let ctx = &mut ctx();
    for vector in data()["negative_programs"].as_array().unwrap() {
        let puzzle = canonical_program(ctx, &unhex(&vector["puzzle"]))?;
        let solution = canonical_program(ctx, &unhex(&vector["solution"]))?;
        assert!(
            run_program(
                ctx,
                &ChiaDialect::new(ClvmFlags::empty()),
                puzzle,
                solution,
                500_000_000
            )
            .is_err(),
            "{}",
            vector["name"]
        );
    }
    Ok(())
}

#[test]
fn python_higher_threshold_recoveries_match_bytes_signatures_and_consensus() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    let ctx = &mut ctx();
    for family in data()["recovery_matrix"].as_array().unwrap() {
        let old = InternalCustodyPolicy::from_reference_bytes(&unhex(&family["policy"]))?;
        let new = InternalCustodyPolicy::from_reference_bytes(&unhex(&family["new_policy"]))?;
        for v in family["operations"].as_array().unwrap() {
            let input = spends(v);
            let selected = keys(v);
            assert_eq!(build(ctx, v, &old, &new)?, input);
            let budget = v["execution"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["cost"].as_u64().unwrap())
                .sum();
            let verified = parse_internal_custody_operation(
                ctx,
                &old,
                &input,
                request(v, &old, &new, &new, &selected),
                &constants(),
                budget,
            )?;
            if let Some(required) = verified.required_signature() {
                let oracle = &v["contexts"][0];
                assert_eq!(
                    required.public_key.to_bytes().as_slice(),
                    unhex(&oracle["public_key"])
                );
                assert_eq!(required.message(), unhex(&oracle["message"]));
                for share in oracle["partials"].as_array().unwrap() {
                    let sk = (1..=6)
                        .map(seed)
                        .find(|sk| {
                            sk.public_key().to_bytes().as_slice() == unhex(&share["public_key"])
                        })
                        .unwrap();
                    assert_eq!(
                        verified.sign_partial(&sk)?.to_bytes().as_slice(),
                        unhex(&share["signature"])
                    );
                }
            }
            for (spend, oracle) in input.iter().zip(v["execution"].as_array().unwrap()) {
                let puzzle = canonical_program(ctx, spend.puzzle_reveal.as_ref())?;
                let solution = canonical_program(ctx, spend.solution.as_ref())?;
                let result = run_program(
                    ctx,
                    &ChiaDialect::new(ClvmFlags::empty()),
                    puzzle,
                    solution,
                    500_000_000,
                )?;
                assert_eq!(result.0, oracle["cost"].as_u64().unwrap());
                assert_eq!(node_to_bytes(ctx, result.1)?, unhex(&oracle["conditions"]));
            }
            let mut sim = Simulator::new();
            for spend in &input {
                sim.insert_coin(spend.coin);
            }
            sim.create_block();
            sim.pass_time(1000);
            sim.new_transaction(SpendBundle::new(input, signature(v)))?;
        }
    }
    Ok(())
}

#[test]
fn verified_disclosures_and_offset_shares_match_every_signed_reference() -> anyhow::Result<()> {
    let old = policy("policy");
    let new = policy("new_policy");
    let next = policy("next_policy");
    for v in data()["operations"].as_array().unwrap() {
        let input = spends(v);
        let selected = keys(v);
        let ctx = &mut ctx();
        let request = if v["kind"] == "payment" {
            InternalCustodyOperationRequest::Payment { signers: &selected }
        } else {
            request(v, &old, &new, &next, &selected)
        };
        let verified = parse_internal_custody_operation(
            ctx,
            &old,
            &input,
            request,
            &constants(),
            500_000_000,
        )?;
        match verified.effects() {
            InternalCustodyOperationEffects::Payment(payment) => {
                let raw = singleton_solution_for_test(ctx, &input[0])?;
                let action =
                    InternalCustodyPaymentArguments::from_clvm(&**ctx, raw.inner_solution.action)?;
                assert_eq!(payment.signed_destination_context, action.destination);
                assert_eq!(
                    payment.vault_child.parent_coin_info,
                    input[0].coin.coin_id()
                );
            }
            InternalCustodyOperationEffects::Rekey {
                timelock_multiple,
                initiation_seconds,
                completion_seconds,
                ..
            } => {
                assert_eq!(
                    *initiation_seconds,
                    u128::from(old.info().rekey_increment) * u128::from(*timelock_multiple)
                        + if *timelock_multiple > 1 {
                            u128::from(old.info().slow_rekey)
                        } else {
                            0
                        }
                );
                assert_eq!(*completion_seconds, old.info().rekey_clawback);
            }
            _ => (),
        }
        let offset = verified.offset_share()?;
        if let Some(required) = verified.required_signature() {
            let mut shares = Vec::new();
            for secret in (1..=6)
                .map(seed)
                .filter(|s| selected.contains(&s.public_key()))
            {
                shares.push(verified.sign_partial(&secret)?);
            }
            if let Some(offset) = offset {
                shares.push(offset);
            }
            assert!(
                chia_bls::verify(
                    &chia_bls::aggregate(&shares),
                    &required.public_key,
                    required.message()
                ),
                "{}",
                v["name"]
            );
        } else {
            assert!(offset.is_none());
        }
    }
    Ok(())
}

fn singleton_solution_for_test(
    ctx: &mut SpendContext,
    spend: &CoinSpend,
) -> Result<
    SingletonSolution<InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr, NodePtr>>,
    DriverError,
> {
    let node = canonical_program(ctx, spend.solution.as_ref())?;
    let solution = SingletonSolution::from_clvm(&**ctx, node)?;
    // Round trip every payment solution through the public layer interface.
    let layer = policy("policy").inner_layer();
    let parsed = InternalCustodyLayer::parse_solution(
        ctx,
        SingletonSolution::<NodePtr>::from_clvm(&**ctx, node)?.inner_solution,
    )?;
    let rebuilt = layer.construct_solution(ctx, parsed)?;
    let original = SingletonSolution::<NodePtr>::from_clvm(&**ctx, node)?.inner_solution;
    assert_eq!(ctx.serialize(&rebuilt)?, ctx.serialize(&original)?);
    Ok(solution)
}
