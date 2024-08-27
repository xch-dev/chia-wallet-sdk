use chia_protocol::{Bytes32, Coin};
use chia_puzzles::standard::StandardArgs;
use chia_wallet_sdk::*;

fn main() -> anyhow::Result<()> {
    let ctx = &mut SpendContext::new();
    let sk = test_secret_key()?;
    let pk = sk.public_key();
    let p2_puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
    let coin = Coin::new(Bytes32::default(), p2_puzzle_hash, 1_000);

    // Issue the CAT using the single issuance (genesis by coin id) TAIL.
    let conditions =
        Conditions::new().create_coin(p2_puzzle_hash, coin.amount, vec![p2_puzzle_hash.into()]);
    let (issue_cat, cat) = Cat::single_issuance_eve(ctx, coin.coin_id(), coin.amount, conditions)?;
    ctx.spend_p2_coin(coin, pk, issue_cat)?;
    println!("Issued test CAT with asset id {}", cat.asset_id);

    // Spend the CAT coin.
    let new_cat = cat.wrapped_child(p2_puzzle_hash, 1000);
    let cat_spends = [CatSpend::new(
        new_cat,
        StandardLayer::new(pk).spend(
            ctx,
            Conditions::new().create_coin(p2_puzzle_hash, coin.amount, vec![p2_puzzle_hash.into()]),
        )?,
    )];

    Cat::spend_all(ctx, &cat_spends)?;

    let new_coin = new_cat.wrapped_child(p2_puzzle_hash, 1000).coin;

    println!(
        "Spent the CAT coin to create new coin with id {}",
        new_coin.coin_id()
    );

    Ok(())
}
