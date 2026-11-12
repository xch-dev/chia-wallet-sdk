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
                        request(v, &new, &next, &selected),
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
                        request(v, &new, &next, &selected),
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
                    request(v, &new, &next, &selected),
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
                    request(v, &new, &next, &selected),
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
                        request(v, &new, &next, &bad),
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
                Some(InternalCustodyWithdrawal {
                    amount: 20,
                    destination: bytes(&data()["destination"])
                }),
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
    // Cancellation cannot substitute a stronger/weaker timelock-multiple filter.
    let pending = spends(operation("slow-cancel"))[0].coin;
    assert!(
        p.rekey_clawback_spend(
            ctx,
            pending,
            &policy("new_policy"),
            &keys(operation("normal-rekey")),
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
                false
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
            old.rekey_clawback_spend(ctx, pending, &policy("next_policy"), &selected, false)
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
