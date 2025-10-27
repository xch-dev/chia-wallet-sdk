use anyhow::Result;
use chia_wallet_sdk::prelude::*;

fn main() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = sim.bls(1_000);
    let p2 = StandardLayer::new(alice.pk);

    let memos = ctx.hint(alice.puzzle_hash)?;

    // Issue the CAT using the single issuance (genesis by coin id) TAIL.
    let conditions = Conditions::new().create_coin(alice.puzzle_hash, 1_000, memos);
    let (issue_cat, cats) = Cat::issue_with_coin(ctx, alice.coin.coin_id(), 1_000, conditions)?;
    p2.spend(ctx, alice.coin, issue_cat)?;
    println!("Issued test CAT with asset id {}", cats[0].info.asset_id);

    // Spend the CAT coin.
    let cat = cats[0];
    let cat_spends = [CatSpend::new(
        cat,
        p2.spend_with_conditions(
            ctx,
            Conditions::new().create_coin(alice.puzzle_hash, 1000, memos),
        )?,
    )];

    Cat::spend_all(ctx, &cat_spends)?;

    let new_coin = cat.child(alice.puzzle_hash, 1000).coin;

    sim.spend_coins(ctx.take(), &[alice.sk])?;

    println!(
        "Spent the CAT coin to create new coin with id {}",
        new_coin.coin_id()
    );

    Ok(())
}
