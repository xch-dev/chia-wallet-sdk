use chia_sdk_driver::{Cat, CatSpend, SpendContext, SpendWithConditions, StandardLayer};
use chia_sdk_test::Simulator;
use chia_sdk_types::Conditions;

fn main() -> anyhow::Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = sim.bls(1_000);
    let p2 = StandardLayer::new(alice.pk);

    let memos = ctx.hint(alice.puzzle_hash)?;

    // Issue the CAT using the single issuance (genesis by coin id) TAIL.
    let conditions = Conditions::new().create_coin(alice.puzzle_hash, 1_000, Some(memos));
    let (issue_cat, cat) = Cat::single_issuance_eve(ctx, alice.coin.coin_id(), 1_000, conditions)?;
    p2.spend(ctx, alice.coin, issue_cat)?;
    println!("Issued test CAT with asset id {}", cat.asset_id);

    // Spend the CAT coin.
    let new_cat = cat.wrapped_child(alice.puzzle_hash, 1000);
    let cat_spends = [CatSpend::new(
        new_cat,
        p2.spend_with_conditions(
            ctx,
            Conditions::new().create_coin(alice.puzzle_hash, 1000, Some(memos)),
        )?,
    )];

    Cat::spend_all(ctx, &cat_spends)?;

    let new_coin = new_cat.wrapped_child(alice.puzzle_hash, 1000).coin;

    sim.spend_coins(ctx.take(), &[alice.sk])?;

    println!(
        "Spent the CAT coin to create new coin with id {}",
        new_coin.coin_id()
    );

    Ok(())
}
