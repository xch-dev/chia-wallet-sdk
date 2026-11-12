use super::*;

#[test]
fn independent_consensus_accepts_every_reference_operation_and_staged_lock() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::Simulator;
    for v in data()["operations"].as_array().unwrap() {
        if v["name"] == "lock-start" {
            continue;
        }
        let input = spends(v);
        let mut sim = Simulator::new();
        let ids: Vec<_> = input.iter().map(|s| s.coin.coin_id()).collect();
        for spend in &input {
            if !ids.contains(&spend.coin.parent_coin_info) {
                sim.insert_coin(spend.coin);
            }
        }
        sim.create_block();
        sim.pass_time(1000);
        if v["name"] == "lock" {
            assert!(
                sim.new_transaction(SpendBundle::new(input.clone(), signature(v)))
                    .is_err(),
                "combined lock must be staged"
            );
            sim.new_transaction(SpendBundle::new(vec![input[0].clone()], signature(v)))?;
            sim.new_transaction(SpendBundle::new(input[1..].to_vec(), Signature::default()))?;
        } else {
            sim.new_transaction(SpendBundle::new(input.clone(), signature(v)))
                .map_err(|e| anyhow::anyhow!("{}: {e:?}", v["name"]))?;
        }
        assert!(
            sim.new_transaction(SpendBundle::new(input, signature(v)))
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn cli_fee_announcement_binds_an_independent_fee_sponsor() -> anyhow::Result<()> {
    use chia_protocol::SpendBundle;
    use chia_sdk_test::{Simulator, to_program, to_puzzle};
    use chia_sdk_types::{Conditions, announcement_id};
    for name in [
        "payment-fee",
        "normal-rekey-fee",
        "slow-rekey-fee",
        "clawback-fee",
        "normal-cancel",
        "slow-cancel",
    ] {
        let v = operation(name);
        let mut input = spends(v);
        let mut sim = Simulator::new();
        for s in &input {
            sim.insert_coin(s.coin);
        }
        let (hash, reveal) = to_puzzle(1)?;
        let sponsor = sim.new_coin(hash, 1000);
        let conditions = Conditions::new()
            .assert_coin_announcement(announcement_id(input[0].coin.coin_id(), []))
            .reserve_fee(1000);
        let fee = CoinSpend::new(sponsor, reveal, to_program(conditions)?);
        sim.create_block();
        sim.pass_time(1000);
        assert!(
            sim.new_transaction(SpendBundle::new(vec![fee.clone()], Signature::default()))
                .is_err()
        );
        input.push(fee);
        sim.new_transaction(SpendBundle::new(input, signature(v)))?;
    }
    Ok(())
}
