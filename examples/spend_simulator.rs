use chia_protocol::Coin;
use chia_puzzles::standard::StandardArgs;
use chia_wallet_sdk::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create the simulator server and connect the peer client.
    let sim = PeerSimulator::new().await?;
    let peer = sim.connect().await?;

    // Setup the key, puzzle hash, and mint a coin.
    let sk = test_secret_key()?;
    let pk = sk.public_key();
    let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
    let coin = sim.mint_coin(puzzle_hash, 1_000).await;

    println!("Minted test coin with coin id {}", coin.coin_id());

    // Create the spend context and a simple transaction.
    let ctx = &mut SpendContext::new();

    let conditions = Conditions::new()
        .create_coin(puzzle_hash, 900, Vec::new())
        .reserve_fee(100);

    ctx.spend_p2_coin(coin, pk, conditions)?;

    let new_coin = Coin::new(coin.coin_id(), puzzle_hash, 900);

    println!("Spent coin to create new coin {}", new_coin.coin_id());

    // Sign and submit the transaction to the simulator.
    // This will produce an error if the transaction is not successful.
    let coin_spends = ctx.take();
    test_transaction(&peer, coin_spends, &[sk], &sim.config().constants).await;

    println!("Transaction was successful.");

    Ok(())
}
