use chia_protocol::Coin;
use chia_sdk_driver::{SpendContext, StandardLayer};
use chia_sdk_test::Simulator;
use chia_sdk_types::Conditions;

fn main() -> anyhow::Result<()> {
    // Create the simulator server and connect the peer client.
    let mut sim = Simulator::new();

    // Setup the key, puzzle hash, and mint a coin.
    let alice = sim.bls(1_000);

    println!("Minted test coin with coin id {}", alice.coin.coin_id());

    // Create the spend context and a simple transaction.
    let ctx = &mut SpendContext::new();

    let conditions = Conditions::new()
        .create_coin(alice.puzzle_hash, 900, None)
        .reserve_fee(100);

    StandardLayer::new(alice.pk).spend(ctx, alice.coin, conditions)?;

    let new_coin = Coin::new(alice.coin.coin_id(), alice.puzzle_hash, 900);

    println!("Spent coin to create new coin {}", new_coin.coin_id());

    // Sign and submit the transaction to the simulator.
    // This will produce an error if the transaction is not successful.
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends, &[alice.sk])?;

    println!("Transaction was successful.");

    Ok(())
}
