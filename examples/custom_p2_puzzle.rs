use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_wallet_sdk::*;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;
use hex_literal::hex;

// We need to define the puzzle reveal.
// This can be found in `../puzzles/custom_p2_puzzle.clsp.hex`.
pub const CUSTOM_P2_PUZZLE: [u8; 137] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ff05ffff04ffff02ff06ffff04ff02ffff
    04ff0bff80808080ff80808080ff0b80ffff04ffff01ff32ff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080
    ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff018080
    "
);

// The puzzle hash can be calculated with `opc -H "$(opd <puzzle_reveal>)"` with `chia-dev-tools`.
pub const CUSTOM_P2_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "0ff94726f1a8dea5c3f70d3121945190778d3b2b3fcda3735a1f290977e98341"
));

// These are the curried arguments that the puzzle accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct CustomArgs {
    pub public_key: PublicKey,
}

impl CustomArgs {
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }

    pub fn curry_tree_hash(public_key: PublicKey) -> TreeHash {
        CurriedProgram {
            program: CUSTOM_P2_PUZZLE_HASH,
            args: CustomArgs::new(public_key),
        }
        .tree_hash()
    }
}

// And the solution is just a list of conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct CustomSolution<T> {
    pub conditions: T,
}

// For convenience, we can add a way to allocate our puzzle on the `SpendContext`.
pub trait CustomExt {
    fn custom_puzzle(&mut self) -> Result<NodePtr, DriverError>;
    fn spend_custom_coin(
        &mut self,
        coin: Coin,
        public_key: PublicKey,
        conditions: Conditions,
    ) -> Result<(), DriverError>;
}

impl CustomExt for SpendContext {
    fn custom_puzzle(&mut self) -> Result<NodePtr, DriverError> {
        self.puzzle(CUSTOM_P2_PUZZLE_HASH, &CUSTOM_P2_PUZZLE)
    }

    fn spend_custom_coin(
        &mut self,
        coin: Coin,
        public_key: PublicKey,
        conditions: Conditions,
    ) -> Result<(), DriverError> {
        let spend = conditions.custom_spend(self, public_key)?;
        let puzzle_reveal = self.serialize(&spend.puzzle)?;
        let solution = self.serialize(&spend.solution)?;
        self.insert(CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }
}

// Let's extend the `Conditions` struct to generate spends for our new p2 puzzle.
pub trait CustomSpend {
    fn custom_spend(
        self,
        ctx: &mut SpendContext,
        public_key: PublicKey,
    ) -> Result<Spend, DriverError>;
}

impl CustomSpend for Conditions {
    fn custom_spend(
        self,
        ctx: &mut SpendContext,
        public_key: PublicKey,
    ) -> Result<Spend, DriverError> {
        let custom_puzzle = ctx.custom_puzzle()?;

        let puzzle = ctx.alloc(&CurriedProgram {
            program: custom_puzzle,
            args: CustomArgs::new(public_key),
        })?;

        let solution = ctx.alloc(&CustomSolution { conditions: self })?;

        Ok(Spend::new(puzzle, solution))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create the simulator server and connect the peer client.
    let sim = PeerSimulator::new().await?;
    let peer = sim.connect().await?;

    // Setup the key, puzzle hash, and mint a coin.
    let sk = test_secret_key()?;
    let pk = sk.public_key();
    let puzzle_hash = CustomArgs::curry_tree_hash(pk).into();
    let coin = sim.mint_coin(puzzle_hash, 1_000).await;

    println!("Minted custom test coin with coin id {}", coin.coin_id());

    // Create the spend context and a simple transaction.
    let ctx = &mut SpendContext::new();

    let conditions = Conditions::new()
        .create_coin(puzzle_hash, 900, Vec::new())
        .reserve_fee(100);

    ctx.spend_custom_coin(coin, pk, conditions)?;

    let new_coin = Coin::new(coin.coin_id(), puzzle_hash, 900);

    println!("Spent coin to create new coin {}", new_coin.coin_id());

    // Sign and submit the transaction to the simulator.
    // This will produce an error if the transaction is not successful.
    let coin_spends = ctx.take();
    test_transaction(&peer, coin_spends, &[sk], &sim.config().constants).await;

    println!("Transaction was successful.");

    Ok(())
}
