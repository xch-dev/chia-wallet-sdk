use super::*;

#[test]
fn connected_payment_clawback_refund_absorption_and_completion_race() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    let p = policy("policy");
    let ctx = &mut ctx();
    let v = operation("payment");
    let initial = spends(v);
    let selected = keys(v);
    let mut sim = Simulator::new();
    sim.insert_coin(initial[0].coin);
    sim.create_block();
    sim.pass_time(1000);
    let facts = *verify_payment(ctx, &p, &initial, &selected, &constants(), 500_000_000)?
        .payment()
        .unwrap();
    sim.new_transaction(SpendBundle::new(initial.clone(), signature(v)))?;
    let pending = facts.pending_payment.unwrap();
    let complete = p.completion_spend(ctx, pending.coin, pending.destination)?;
    assert!(
        sim.new_transaction(SpendBundle::new(
            vec![complete.clone()],
            Signature::default()
        ))
        .is_err()
    );
    let mut completion_branch = sim.clone();
    completion_branch.pass_time(p.info().payment_clawback);
    completion_branch.new_transaction(SpendBundle::new(
        vec![complete.clone()],
        Signature::default(),
    ))?;
    // A cancellation remains valid even after the completion delay has elapsed.
    sim.pass_time(p.info().payment_clawback + 1);
    let cancel =
        p.payment_clawback_spend(ctx, pending.coin, pending.destination, &selected, true)?;
    let verified = parse_internal_custody_operation(
        ctx,
        &p,
        std::slice::from_ref(&cancel),
        InternalCustodyOperationRequest::PaymentClawback {
            destination: pending.destination,
            signers: &selected,
        },
        &constants(),
        500_000_000,
    )?;
    let InternalCustodyOperationEffects::PaymentClawback { refund, .. } = *verified.effects()
    else {
        panic!("clawback effects")
    };
    let sig = sign_required(verified.required_signature().unwrap(), &selected);
    assert!(
        completion_branch
            .new_transaction(SpendBundle::new(vec![cancel.clone()], sig.clone()))
            .is_err()
    );
    sim.new_transaction(SpendBundle::new(vec![cancel], sig))?;
    assert!(
        sim.new_transaction(SpendBundle::new(vec![complete], Signature::default()))
            .is_err()
    );
    let proof = child_proof(ctx, &p, initial[0].coin)?;
    sim.pass_time(p.info().withdrawal_timelock);
    let absorb = p.payment_spends(
        ctx,
        facts.vault_child,
        proof,
        payment_options(None),
        &selected,
        &[refund],
    )?;
    let sig = sign_payment(ctx, &p, &absorb, &selected)?;
    let final_coin = verify_payment(ctx, &p, &absorb, &selected, &constants(), 500_000_000)?
        .payment()
        .unwrap()
        .vault_child;
    sim.new_transaction(SpendBundle::new(absorb, sig))?;
    assert_eq!(final_coin.amount, initial[0].coin.amount);
    assert!(
        sim.coin_state(final_coin.coin_id())
            .unwrap()
            .spent_height
            .is_none()
    );
    Ok(())
}
#[test]
fn pending_payment_keeps_original_policy_after_the_singleton_rekeys() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    let ctx = &mut ctx();
    let old = policy("policy");
    let new = policy("new_policy");
    let v = operation("payment");
    let input = spends(v);
    let selected = keys(v);
    let facts = *verify_payment(ctx, &old, &input, &selected, &constants(), 500_000_000)?
        .payment()
        .unwrap();
    let mut sim = Simulator::new();
    sim.insert_coin(input[0].coin);
    sim.create_block();
    sim.pass_time(1000);
    sim.new_transaction(SpendBundle::new(input.clone(), signature(v)))?;
    sim.pass_time(1000);
    let lineage = child_proof(ctx, &old, input[0].coin)?;
    let rekey = old.start_rekey_spend(
        ctx,
        facts.vault_child,
        lineage,
        &new,
        &selected,
        crate::InternalCustodyRekeyOptions {
            timelock_multiple: 1,
            fee_announcement: false,
        },
    )?;
    let verified = parse_internal_custody_operation(
        ctx,
        &old,
        std::slice::from_ref(&rekey),
        InternalCustodyOperationRequest::Rekey {
            new_policy: &new,
            signers: &selected,
        },
        &constants(),
        500_000_000,
    )?;
    let InternalCustodyOperationEffects::Rekey {
        vault_child,
        pending,
        timelock_multiple,
        ..
    } = *verified.effects()
    else {
        panic!("rekey effects")
    };
    sim.new_transaction(SpendBundle::new(
        vec![rekey],
        sign_required(verified.required_signature().unwrap(), &selected),
    ))?;
    sim.pass_time(old.info().rekey_clawback);
    let lineage = child_proof(ctx, &old, facts.vault_child)?;
    let Proof::Lineage(parent) = lineage else {
        unreachable!()
    };
    let finish = old.complete_rekey_spends(
        ctx,
        vault_child,
        pending,
        InternalCustodyRekeyState {
            old_root: old.info().puzzle_root,
            new_root: new.info().puzzle_root,
            timelock_multiple,
        },
        (lineage, parent),
    )?;
    sim.new_transaction(SpendBundle::new(finish.to_vec(), Signature::default()))?;
    let ach = facts.pending_payment.unwrap();
    assert!(
        new.completion_spend(ctx, ach.coin, ach.destination)
            .is_err()
    );
    assert!(
        new.payment_clawback_spend(ctx, ach.coin, ach.destination, &new.keys()[..2], false)
            .is_err()
    );
    let cancel = old.payment_clawback_spend(ctx, ach.coin, ach.destination, &selected, false)?;
    let verified = parse_internal_custody_operation(
        ctx,
        &old,
        std::slice::from_ref(&cancel),
        InternalCustodyOperationRequest::PaymentClawback {
            destination: ach.destination,
            signers: &selected,
        },
        &constants(),
        500_000_000,
    )?;
    sim.new_transaction(SpendBundle::new(
        vec![cancel],
        sign_required(verified.required_signature().unwrap(), &selected),
    ))?;
    Ok(())
}
