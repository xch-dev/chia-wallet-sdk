use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{cat::CatArgs, standard::StandardArgs};
use chia_wallet_sdk::*;

fn main() -> anyhow::Result<()> {
    let ctx = &mut SpendContext::new();
    let sk = secret_key()?;
    let pk = sk.public_key();
    let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
    let coin = Coin::new(Bytes32::default(), puzzle_hash, 1_000);

    // Issue the CAT using the single issuance (genesis by coin id) TAIL.
    let conditions = Conditions::new().create_hinted_coin(puzzle_hash, coin.amount, puzzle_hash);
    let (issue_cat, cat) = issue_cat_from_coin(ctx, coin.coin_id(), coin.amount, conditions)?;
    ctx.spend_p2_coin(coin, pk, issue_cat)?;
    println!("Issued test CAT with asset id {}", cat.asset_id);

    // Calculate the coin that was created.
    let cat_puzzle_hash = CatArgs::curry_tree_hash(cat.asset_id, puzzle_hash.into()).into();
    let cat_coin = Coin::new(cat.eve_coin.coin_id(), cat_puzzle_hash, coin.amount);
    println!("Created CAT coin with id {}", cat_coin.coin_id());

    // Spend the CAT coin.
    let cat_spends = [CatSpend::new(
        Cat::new(cat_coin, Some(cat.lineage_proof), cat.asset_id, puzzle_hash),
        Conditions::new()
            .create_hinted_coin(puzzle_hash, coin.amount, puzzle_hash)
            .p2_spend(ctx, pk)?,
    )];

    for coin_spend in Cat::spend_all(ctx, &cat_spends)? {
        ctx.insert_coin_spend(coin_spend);
    }

    let new_coin = Coin::new(cat_coin.coin_id(), cat_puzzle_hash, coin.amount);

    println!(
        "Spent the CAT coin to create new coin with id {}",
        new_coin.coin_id()
    );

    Ok(())
}
