use super::*;
use chia_puzzle_types::Memos;

#[test]
fn connected_rekeys_cancel_or_finish_then_use_replacement_policy() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    let old = policy("policy");
    let new = policy("new_policy");
    let ctx = &mut ctx();
    for name in ["normal-rekey", "slow-rekey"] {
        let v = operation(name);
        let initial = spends(v);
        let selected = keys(v);
        let mut sim = Simulator::new();
        sim.insert_coin(initial[0].coin);
        sim.create_block();
        // Both ordinary and recovery initiation enforce the Python relative delay.
        assert!(
            sim.new_transaction(SpendBundle::new(initial.clone(), signature(v)))
                .is_err()
        );
        sim.pass_time(1000);
        let verified = parse_internal_custody_operation(
            ctx,
            &old,
            &initial,
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
        sim.new_transaction(SpendBundle::new(initial.clone(), signature(v)))?;
        let mut cancelled = sim.clone();
        let cancel = old.rekey_clawback_spend(
            ctx,
            pending,
            rekey_state(&old, new.info().puzzle_root, timelock_multiple),
            &selected,
            true,
        )?;
        let verified = parse_internal_custody_operation(
            ctx,
            &old,
            std::slice::from_ref(&cancel),
            InternalCustodyOperationRequest::RekeyClawback {
                state: rekey_state(&old, new.info().puzzle_root, timelock_multiple),
                signers: &selected,
            },
            &constants(),
            500_000_000,
        )?;
        cancelled.new_transaction(SpendBundle::new(
            vec![cancel],
            sign_required(verified.required_signature().unwrap(), &selected),
        ))?;
        assert!(cancelled.children(pending.coin_id()).is_empty());
        let lineage = child_proof(ctx, &old, initial[0].coin)?;
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
        assert!(
            sim.new_transaction(SpendBundle::new(finish.to_vec(), Signature::default()))
                .is_err()
        );
        sim.pass_time(old.info().rekey_clawback);
        assert!(
            cancelled
                .new_transaction(SpendBundle::new(finish.to_vec(), Signature::default()))
                .is_err()
        );
        let verified = parse_internal_custody_operation(
            ctx,
            &old,
            &finish,
            InternalCustodyOperationRequest::RekeyCompletion { new_policy: &new },
            &constants(),
            500_000_000,
        )?;
        let InternalCustodyOperationEffects::RekeyCompletion {
            vault_child: replacement,
            ..
        } = *verified.effects()
        else {
            panic!("completion effects")
        };
        sim.new_transaction(SpendBundle::new(finish.to_vec(), Signature::default()))?;
        assert!(
            sim.new_transaction(SpendBundle::new(initial.clone(), signature(v)))
                .is_err()
        );
        sim.pass_time(new.info().withdrawal_timelock);
        let selected: Vec<_> = new.keys()[..2].to_vec();
        let proof = child_proof(ctx, &old, vault_child)?;
        let payment = new.payment_spends(
            ctx,
            replacement,
            proof,
            Some(InternalCustodyWithdrawal {
                amount: 20,
                destination: bytes(&data()["destination"]),
            }),
            &selected,
            &[],
        )?;
        let sig = sign_payment(ctx, &new, &payment, &selected)?;
        sim.new_transaction(SpendBundle::new(payment, sig))?;
    }
    Ok(())
}
#[test]
fn launch_absorb_lock_resume_and_pay_at_the_higher_security_level() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::{Simulator, to_program, to_puzzle};
    let ctx = &mut ctx();
    let template = policy("policy");
    let mut sim = Simulator::new();
    let (hash, reveal) = to_puzzle(1)?;
    let funding = sim.new_coin(hash, 1001);
    let launcher = Launcher::new(funding.coin_id(), 101);
    let launcher_coin = launcher.coin();
    let p = InternalCustodyPolicy::derive(
        ctx,
        InternalCustodyInfo {
            launcher_id: launcher_coin.coin_id(),
            ..template.info()
        },
        template.keys().to_vec(),
        template.thresholds(),
    )?;
    let inner = p.inner_layer(ctx)?.construct_puzzle(ctx)?;
    let (conditions, eve) = launcher.spend(ctx, ctx.tree_hash(inner).into(), ())?;
    let deposit_ph = P2SingletonArgs::new(p.info().launcher_id)
        .curry_tree_hash()
        .into();
    let conditions = conditions.create_coin(deposit_ph, 900, Memos::None);
    let mut launch = ctx.take();
    launch.push(CoinSpend::new(funding, reveal, to_program(conditions)?));
    sim.new_transaction(SpendBundle::new(launch, Signature::default()))?;
    sim.pass_time(p.info().withdrawal_timelock);
    let proof = Proof::Eve(chia_puzzle_types::EveProof {
        parent_parent_coin_info: launcher_coin.parent_coin_info,
        parent_amount: launcher_coin.amount,
    });
    let selected = &p.keys()[..2];
    let absorb = p.payment_spends(
        ctx,
        eve,
        proof,
        None,
        selected,
        &[Coin::new(funding.coin_id(), deposit_ph, 900)],
    )?;
    let verified =
        parse_internal_custody_payment(ctx, &p, &absorb, selected, &constants(), 500_000_000)?;
    let funded = verified.payment().vault_child;
    sim.new_transaction(SpendBundle::new(
        absorb,
        sign_required(verified.required_signature(), selected),
    ))?;
    assert_eq!(funded.amount, 1001);
    let proof = child_proof(ctx, &p, eve)?;
    let lock = p.increase_security_spends(ctx, funded, proof, p.keys())?;
    let next = InternalCustodyPolicy::derive(
        ctx,
        p.info(),
        p.keys().to_vec(),
        InternalCustodyThresholds {
            required: 3,
            ..p.thresholds()
        },
    )?;
    let verified = parse_internal_custody_operation(
        ctx,
        &p,
        std::slice::from_ref(&lock.authorization),
        InternalCustodyOperationRequest::IncreaseSecurity {
            new_policy: &next,
            signers: p.keys(),
        },
        &constants(),
        500_000_000,
    )?;
    let InternalCustodyOperationEffects::IncreaseSecurity {
        vault_child: locked,
        pending,
        intermediate_singleton,
        ..
    } = *verified.effects()
    else {
        panic!("lock effects")
    };
    sim.new_transaction(SpendBundle::new(
        vec![lock.authorization],
        sign_required(verified.required_signature().unwrap(), p.keys()),
    ))?;
    // Reconstruct stage two using only the committed state/lineage, as a
    // restarted application does. It must equal the originally prepared stage.
    let lineage = child_proof(ctx, &p, funded)?;
    let Proof::Lineage(parent) = lineage else {
        unreachable!()
    };
    let resumed = p.complete_rekey_spends(
        ctx,
        intermediate_singleton,
        pending,
        InternalCustodyRekeyState {
            old_root: p.info().puzzle_root,
            new_root: next.info().puzzle_root,
            timelock_multiple: 0,
        },
        (lineage, parent),
    )?;
    assert_eq!(
        resumed,
        [lock.completion[1].clone(), lock.completion[0].clone()]
    );
    parse_internal_custody_operation(
        ctx,
        &p,
        &resumed,
        InternalCustodyOperationRequest::RekeyCompletion { new_policy: &next },
        &constants(),
        500_000_000,
    )?;
    sim.new_transaction(SpendBundle::new(resumed.to_vec(), Signature::default()))?;
    sim.pass_time(next.info().withdrawal_timelock);
    let proof = child_proof(ctx, &p, intermediate_singleton)?;
    let withdrawal = Some(InternalCustodyWithdrawal {
        amount: 100,
        destination: bytes(&data()["destination"]),
    });
    assert!(
        next.payment_spends(ctx, locked, proof, withdrawal, selected, &[])
            .is_err()
    );
    let payment = next.payment_spends(ctx, locked, proof, withdrawal, next.keys(), &[])?;
    let sig = sign_payment(ctx, &next, &payment, next.keys())?;
    sim.new_transaction(SpendBundle::new(payment, sig))?;
    Ok(())
}

#[test]
fn rekey_completion_uses_the_latest_singleton_and_original_pending_lineage() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    let ctx = &mut ctx();
    let old = policy("policy");
    let new = policy("new_policy");
    let v = operation("normal-rekey");
    let input = spends(v);
    let selected = keys(v);
    let verified = parse_internal_custody_operation(
        ctx,
        &old,
        &input,
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
    let mut sim = Simulator::new();
    sim.insert_coin(input[0].coin);
    sim.create_block();
    sim.pass_time(1000);
    sim.new_transaction(SpendBundle::new(input.clone(), signature(v)))?;
    sim.pass_time(1000);
    let original_lineage = child_proof(ctx, &old, input[0].coin)?;
    let payment = old.payment_spends(
        ctx,
        vault_child,
        original_lineage,
        Some(InternalCustodyWithdrawal {
            amount: 20,
            destination: bytes(&data()["destination"]),
        }),
        &selected,
        &[],
    )?;
    let verified =
        parse_internal_custody_payment(ctx, &old, &payment, &selected, &constants(), 500_000_000)?;
    let latest = verified.payment().vault_child;
    sim.new_transaction(SpendBundle::new(
        payment,
        sign_required(verified.required_signature(), &selected),
    ))?;
    let latest_lineage = child_proof(ctx, &old, vault_child)?;
    let Proof::Lineage(pending_lineage) = original_lineage else {
        unreachable!()
    };
    let state = InternalCustodyRekeyState {
        old_root: old.info().puzzle_root,
        new_root: new.info().puzzle_root,
        timelock_multiple,
    };
    let finish = old.complete_rekey_spends(
        ctx,
        latest,
        pending,
        state,
        (latest_lineage, pending_lineage),
    )?;
    parse_internal_custody_operation(
        ctx,
        &old,
        &finish,
        InternalCustodyOperationRequest::RekeyCompletion { new_policy: &new },
        &constants(),
        500_000_000,
    )?;
    sim.new_transaction(SpendBundle::new(finish.to_vec(), Signature::default()))?;
    let hash = new.singleton_puzzle(ctx)?;
    let expected = Coin::new(latest.coin_id(), ctx.tree_hash(hash).into(), latest.amount);
    assert!(
        sim.coin_state(expected.coin_id())
            .unwrap()
            .spent_height
            .is_none()
    );
    assert_eq!(expected.amount, 81);
    Ok(())
}

#[test]
fn extended_recovery_can_complete_or_be_vetoed_by_a_normal_quorum() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    let ctx = &mut ctx();
    let old = policy("policy");
    let new = policy("new_policy");
    let start = operation("long-recovery-start");
    let input = spends(start);
    let selected = keys(start);
    let verified = parse_internal_custody_operation(
        ctx,
        &old,
        &input,
        InternalCustodyOperationRequest::Rekey {
            new_policy: &new,
            signers: &selected,
        },
        &constants(),
        500_000_000,
    )?;
    let InternalCustodyOperationEffects::Rekey {
        pending,
        timelock_multiple: 3,
        ..
    } = *verified.effects()
    else {
        panic!("extended recovery")
    };
    let mut sim = Simulator::new();
    sim.insert_coin(input[0].coin);
    sim.create_block();
    sim.pass_time(1000);
    sim.new_transaction(SpendBundle::new(input, signature(start)))?;
    let mut cancelled = sim.clone();
    let normal = keys(operation("normal-rekey"));
    let state = rekey_state(&old, new.info().puzzle_root, 3);
    let cancel = old.rekey_clawback_spend(ctx, pending, state, &normal, false)?;
    assert_eq!(
        vec![cancel.clone()],
        spends(operation("long-normal-cancel"))
    );
    let verified = parse_internal_custody_operation(
        ctx,
        &old,
        std::slice::from_ref(&cancel),
        InternalCustodyOperationRequest::RekeyClawback {
            state,
            signers: &normal,
        },
        &constants(),
        500_000_000,
    )?;
    let sig = sign_required(verified.required_signature().unwrap(), &normal);
    cancelled.new_transaction(SpendBundle::new(vec![cancel.clone()], sig.clone()))?;
    let finish = spends(operation("long-recovery-complete"));
    assert!(
        sim.new_transaction(SpendBundle::new(finish.clone(), Signature::default()))
            .is_err()
    );
    sim.pass_time(old.info().rekey_clawback);
    assert!(
        cancelled
            .new_transaction(SpendBundle::new(finish.clone(), Signature::default()))
            .is_err()
    );
    sim.new_transaction(SpendBundle::new(finish, Signature::default()))?;
    assert!(
        sim.new_transaction(SpendBundle::new(vec![cancel], sig))
            .is_err()
    );
    Ok(())
}
