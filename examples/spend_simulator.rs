use chia_protocol::Coin;
use chia_puzzles::standard::StandardArgs;
use chia_sdk_driver::{SpendContext, StandardLayer};
use chia_sdk_test::{test_secret_key, Simulator};
use chia_sdk_types::Conditions;

fn main() -> anyhow::Result<()> {
    // Create the simulator server and connect the peer client.
    let mut sim = Simulator::new();

    // Setup the key, puzzle hash, and mint a coin.
    let sk = test_secret_key()?;
    let pk = sk.public_key();
    let p2 = StandardLayer::new(pk);

    let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
    let coin = sim.new_coin(puzzle_hash, 1_000);

    println!("Minted test coin with coin id {}", coin.coin_id());

    // Create the spend context and a simple transaction.
    let ctx = &mut SpendContext::new();

    let conditions = Conditions::new()
        .create_coin(puzzle_hash, 900, Vec::new())
        .reserve_fee(100);

    p2.spend(ctx, coin, conditions)?;

    let new_coin = Coin::new(coin.coin_id(), puzzle_hash, 900);

    println!("Spent coin to create new coin {}", new_coin.coin_id());

    // Sign and submit the transaction to the simulator.
    // This will produce an error if the transaction is not successful.
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends, &[sk])?;

    println!("Transaction was successful.");

    Ok(())
}
