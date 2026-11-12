use super::*;

#[test]
fn lifecycle_verification_rejects_substitution_missing_inputs_and_wrong_subsets()
-> anyhow::Result<()> {
    let old = policy("policy");
    let new = policy("new_policy");
    let next = policy("next_policy");
    let ctx = &mut ctx();
    for v in data()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["kind"] != "payment")
    {
        let input = spends(v);
        let selected = keys(v);
        for i in 0..input.len() {
            for mutate in 0..4 {
                let mut bad = input.clone();
                match mutate {
                    0 => bad[i].coin.parent_coin_info = [42; 32].into(),
                    1 => bad[i].coin.amount += 2,
                    2 => bad[i].coin.puzzle_hash = [42; 32].into(),
                    _ => {
                        let mut raw = bad[i].solution.to_vec();
                        raw.push(0x80);
                        bad[i].solution = raw.into();
                    }
                }
                // ACH/rekey cancellation and ACH completion intentionally do not
                // commit to a parent in their solution. A different parent is a
                // different valid coin; signed branches bind it via AGG_SIG_ME.
                if mutate == 0
                    && matches!(
                        v["kind"].as_str().unwrap(),
                        "clawback" | "payment-complete" | "rekey-cancel"
                    )
                {
                    let changed = parse_internal_custody_operation(
                        ctx,
                        &old,
                        &bad,
                        request(v, &old, &new, &next, &selected),
                        &constants(),
                        500_000_000,
                    )?;
                    if let Some(required) = changed.required_signature() {
                        assert_ne!(required.message(), unhex(&v["contexts"][0]["message"]));
                    }
                    continue;
                }
                assert!(
                    parse_internal_custody_operation(
                        ctx,
                        &old,
                        &bad,
                        request(v, &old, &new, &next, &selected),
                        &constants(),
                        500_000_000
                    )
                    .is_err(),
                    "{} input {i} mutation {mutate}",
                    v["name"]
                );
            }
        }
        if input.len() == 2 {
            assert!(
                parse_internal_custody_operation(
                    ctx,
                    &old,
                    &input[..1],
                    request(v, &old, &new, &next, &selected),
                    &constants(),
                    500_000_000
                )
                .is_err()
            );
            let mut reversed = input.clone();
            reversed.reverse();
            assert!(
                parse_internal_custody_operation(
                    ctx,
                    &old,
                    &reversed,
                    request(v, &old, &new, &next, &selected),
                    &constants(),
                    500_000_000
                )
                .is_err()
            );
        }
        if !selected.is_empty() {
            for bad in [vec![selected[0], selected[0]], vec![seed(42).public_key()]] {
                assert!(
                    parse_internal_custody_operation(
                        ctx,
                        &old,
                        &input,
                        request(v, &old, &new, &next, &bad),
                        &constants(),
                        500_000_000
                    )
                    .is_err()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn recovery_and_lock_authorizations_cannot_be_used_for_payments() -> anyhow::Result<()> {
    let ctx = &mut ctx();
    let p = policy("policy");
    let input = spends(operation("payment"));
    let proof = proof(ctx, &input[0]);
    for selected in [keys(operation("slow-rekey")), keys(operation("lock"))] {
        assert!(
            p.payment_spends(
                ctx,
                input[0].coin,
                proof,
                payment_options(Some(InternalCustodyWithdrawal {
                    amount: 20,
                    destination: bytes(&data()["destination"])
                })),
                &selected,
                &[]
            )
            .is_err()
        );
    }
    let locked = policy("next_policy");
    assert!(
        locked
            .increase_security_spends(ctx, input[0].coin, proof, locked.keys())
            .is_err()
    );
    // A stronger quorum can veto a slower attempt; a weaker subset cannot
    // authorize an attempt shorter than its minimum delay.
    let pending = spends(operation("slow-cancel"))[0].coin;
    assert!(
        p.rekey_clawback_spend(
            ctx,
            pending,
            rekey_state(&p, policy("new_policy").info().puzzle_root, 2),
            &keys(operation("normal-rekey")),
            false
        )
        .is_ok()
    );
    assert!(
        p.rekey_clawback_spend(
            ctx,
            spends(operation("normal-cancel"))[0].coin,
            rekey_state(&p, policy("new_policy").info().puzzle_root, 1),
            &keys(operation("slow-rekey")),
            false
        )
        .is_err()
    );
    // All curried STATE parameters remain immutable, including the two omitted
    // from the historical Python is_valid_update metadata check.
    for field in 0..6 {
        let new = policy("new_policy");
        let mut info = new.info();
        match field {
            0 => info.launcher_id = [42; 32].into(),
            1 => info.withdrawal_timelock += 1,
            2 => info.payment_clawback += 1,
            3 => info.rekey_clawback += 1,
            4 => info.rekey_increment += 1,
            _ => info.slow_rekey += 1,
        }
        let target =
            InternalCustodyPolicy::derive(ctx, info, new.keys().to_vec(), new.thresholds())?;
        assert!(
            p.start_rekey_spend(
                ctx,
                input[0].coin,
                proof,
                &target,
                &keys(operation("normal-rekey")),
                crate::InternalCustodyRekeyOptions {
                    timelock_multiple: 1,
                    fee_announcement: false
                }
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn rekey_review_rejects_a_roster_not_committed_by_the_target_root() -> anyhow::Result<()> {
    let ctx = &mut ctx();
    let old = policy("policy");
    let new = policy("new_policy");
    let claimed = InternalCustodyPolicy::new(
        new.info(),
        old.keys().to_vec(),
        new.thresholds(),
        new.next_root(),
        vec![],
        vec![],
    )?;
    let v = operation("normal-rekey");
    let input = spends(v);
    let selected = keys(v);
    assert!(
        parse_internal_custody_operation(
            ctx,
            &old,
            &input,
            InternalCustodyOperationRequest::Rekey {
                new_policy: &claimed,
                signers: &selected
            },
            &constants(),
            500_000_000
        )
        .is_err()
    );
    // A lock cannot be cancelled using the normal or recovery authorization path.
    let pending = spends(operation("lock"))[1].coin;
    for selected in [
        keys(operation("normal-rekey")),
        keys(operation("slow-rekey")),
    ] {
        assert!(
            old.rekey_clawback_spend(
                ctx,
                pending,
                rekey_state(&old, policy("next_policy").info().puzzle_root, 0),
                &selected,
                false
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn policy_derivation_bounds_expansion_and_rejects_forged_next_level() -> anyhow::Result<()> {
    let ctx = &mut ctx();
    let p = policy("policy");
    let oversized = (1..=16).map(|i| seed(i).public_key()).collect();
    assert!(
        InternalCustodyPolicy::derive(
            ctx,
            p.info(),
            oversized,
            InternalCustodyThresholds {
                required: 8,
                maximum: 16,
                minimum: 1
            }
        )
        .is_err()
    );
    let duplicates = vec![p.keys()[0], p.keys()[0]];
    assert!(
        InternalCustodyPolicy::derive(
            ctx,
            p.info(),
            duplicates,
            InternalCustodyThresholds {
                required: 1,
                maximum: 2,
                minimum: 1
            }
        )
        .is_err()
    );
    let forged = InternalCustodyPolicy::new(
        p.info(),
        p.keys().to_vec(),
        p.thresholds(),
        Some([42; 32].into()),
        vec![],
        vec![],
    )?;
    assert!(forged.verify_definition(ctx).is_err());
    Ok(())
}

#[test]
fn payment_arithmetic_checks_final_outputs_instead_of_the_intermediate_total() -> anyhow::Result<()>
{
    let ctx = &mut ctx();
    let old = policy("policy");
    let v = operation("overflow-intermediate");
    let input = spends(v);
    let selected = keys(v);
    let verified = verify_payment(ctx, &old, &input, &selected, &constants(), 500_000_000)?;
    assert_eq!(verified.payment().unwrap().input_coin.amount, u64::MAX - 2);
    assert_eq!(verified.payment().unwrap().incoming_amount, 4);
    assert_eq!(verified.payment().unwrap().outgoing_amount(), 4);
    assert_eq!(verified.payment().unwrap().vault_child.amount, u64::MAX - 2);
    let proof = proof(ctx, &input[0]);
    // The intermediate sum is legal, but an out-of-range final output is not.
    assert!(
        old.payment_spends(
            ctx,
            input[0].coin,
            proof,
            payment_options(None),
            &selected,
            &[input[1].coin]
        )
        .is_err()
    );
    let remaining = crate::internal_custody_remaining_amount;
    assert_eq!(remaining(u64::MAX - 2, 4, 2)?, u64::MAX);
    assert!(remaining(u64::MAX - 2, 4, 0).is_err());
    assert!(remaining(101, 4, 106).is_err());
    assert!(
        old.payment_spends(
            ctx,
            Coin::new(
                input[0].coin.parent_coin_info,
                input[0].coin.puzzle_hash,
                101
            ),
            proof,
            payment_options(Some(InternalCustodyWithdrawal {
                amount: 106,
                destination: bytes(&data()["destination"])
            })),
            &selected,
            &[input[1].coin]
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn cancelling_an_unknown_target_binds_the_attempt_without_a_target_roster() -> anyhow::Result<()> {
    let ctx = &mut ctx();
    let old = policy("policy");
    let v = operation("unknown-target-cancel");
    let input = spends(v);
    let selected = keys(v);
    let state = rekey_state(&old, bytes(&v["new_root"]), 1);
    let verified = parse_internal_custody_operation(
        ctx,
        &old,
        &input,
        InternalCustodyOperationRequest::RekeyClawback {
            state,
            signers: &selected,
        },
        &constants(),
        500_000_000,
    )?;
    assert!(
        matches!(verified.effects(), InternalCustodyOperationEffects::RekeyClawback { new_root, timelock_multiple: 1, .. } if *new_root == state.new_root)
    );
    for changed in [
        InternalCustodyRekeyState {
            old_root: [43; 32].into(),
            ..state
        },
        InternalCustodyRekeyState {
            new_root: [43; 32].into(),
            ..state
        },
        InternalCustodyRekeyState {
            timelock_multiple: 2,
            ..state
        },
        InternalCustodyRekeyState {
            timelock_multiple: 0,
            ..state
        },
    ] {
        assert!(
            parse_internal_custody_operation(
                ctx,
                &old,
                &input,
                InternalCustodyOperationRequest::RekeyClawback {
                    state: changed,
                    signers: &selected
                },
                &constants(),
                500_000_000
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn explicit_attempt_delay_must_cover_the_selected_subset_minimum() {
    let ctx = &mut ctx();
    let old = policy("policy");
    let new = policy("new_policy");
    let input = spends(operation("normal-rekey"));
    let proof = proof(ctx, &input[0]);
    let slow = keys(operation("slow-rekey"));
    for multiple in [0, 1] {
        assert!(
            old.start_rekey_spend(
                ctx,
                input[0].coin,
                proof,
                &new,
                &slow,
                crate::InternalCustodyRekeyOptions {
                    timelock_multiple: multiple,
                    fee_announcement: false
                }
            )
            .is_err()
        );
    }
}
