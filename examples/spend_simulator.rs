use chia_puzzles::standard::StandardArgs;
use chia_wallet_sdk::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create the simulator server and connect the peer client.
    let sim = Simulator::new().await?;
    let peer = sim.connect().await?;

    // Setup the key, puzzle hash, and mint a coin.
    let sk = secret_key()?;
    let pk = sk.public_key();
    let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
    let coin = sim.mint_coin(puzzle_hash, 1_000).await;

    // Create the spend context and a simple transaction.
    let ctx = &mut SpendContext::new();

    let conditions = Conditions::new()
        .create_coin(puzzle_hash, 900)
        .reserve_fee(100);

    ctx.spend_p2_coin(coin, pk, conditions)?;

    // Sign and submit the transaction to the simulator.
    // This will produce an error if the transaction is not successful.
    let coin_spends = ctx.take_spends();
    test_transaction(&peer, coin_spends, &[sk], sim.config().genesis_challenge).await;

    Ok(())
}
