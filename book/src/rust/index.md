# Rust SDK

The Chia Wallet SDK provides a comprehensive set of tools for interacting with the Chia blockchain from Rust applications. This guide will walk you through using the SDK to build wallet functionality and blockchain applications.

## Getting Started

After [installing the SDK](/getting-started.md), you can start using it in your Rust projects.

```rust
use chia_sdk_driver::{SpendContext, StandardLayer};
use chia_sdk_test::Simulator;
use chia_sdk_types::Conditions;

fn main() -> anyhow::Result<()> {
    // Your wallet code here
    Ok(())
}
```

## Key Components

The SDK is composed of several crates, each with a specific responsibility:

- **chia-sdk-driver**: Provides high-level interfaces for common operations like creating transactions
- **chia-sdk-client**: Handles communication with the Chia blockchain network
- **chia-sdk-coinset**: Implements the coin set model fundamental to Chia
- **chia-sdk-signer**: Manages cryptographic signing operations
- **chia-sdk-test**: Tools for testing wallet applications, including the Simulator
- **chia-sdk-types**: Core types and data structures used throughout the SDK

## Common Wallet Operations

### Creating a Spend Transaction

```rust
use chia_sdk_driver::{SpendContext, StandardLayer};
use chia_sdk_test::Simulator;
use chia_sdk_types::Conditions;

fn main() -> anyhow::Result<()> {
    // Create a simulator for testing
    let mut sim = Simulator::new();
    
    // Set up a key and mint a test coin
    let alice = sim.bls(1_000);
    
    // Create a spend context and transaction
    let ctx = &mut SpendContext::new();
    
    // Define the conditions for the transaction
    let conditions = Conditions::new()
        .create_coin(alice.puzzle_hash, 900, None)
        .reserve_fee(100);
    
    // Create a standard layer spend using the public key
    StandardLayer::new(alice.pk).spend(ctx, alice.coin, conditions)?;
    
    // Sign and submit the transaction
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends, &[alice.sk])?;
    
    Ok(())
}
```

### Working with CAT Tokens

Colored coins (CATs) in Chia are a form of user-defined token. Here's how to create and spend them:

```rust
use chia_sdk_driver::{Cat, CatSpend, SpendContext, StandardLayer};
use chia_sdk_test::Simulator;
use chia_sdk_types::Conditions;

fn main() -> anyhow::Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();
    
    let alice = sim.bls(1_000);
    let p2 = StandardLayer::new(alice.pk);
    
    // Issue a new CAT
    let memos = ctx.hint(alice.puzzle_hash)?;
    let conditions = Conditions::new().create_coin(alice.puzzle_hash, 1_000, Some(memos));
    let (issue_cat, cat) = Cat::single_issuance_eve(ctx, alice.coin.coin_id(), 1_000, conditions)?;
    p2.spend(ctx, alice.coin, issue_cat)?;
    
    // Spend the CAT
    let new_cat = cat.wrapped_child(alice.puzzle_hash, 1000);
    let cat_spends = [CatSpend::new(
        new_cat,
        p2.spend_with_conditions(
            ctx,
            Conditions::new().create_coin(alice.puzzle_hash, 1000, Some(memos)),
        )?,
    )];
    
    Cat::spend_all(ctx, &cat_spends)?;
    sim.spend_coins(ctx.take(), &[alice.sk])?;
    
    Ok(())
}
```

### Custom Puzzles

You can create custom puzzles for more advanced use cases:

```rust
// Define a custom puzzle
pub const CUSTOM_PUZZLE: [u8; 137] = /* puzzle bytes */;

// Define the arguments and solution for the puzzle
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct CustomArgs {
    pub public_key: PublicKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct CustomSolution<T> {
    pub conditions: T,
}

// Create an extension trait for the SpendContext
pub trait CustomExt {
    fn custom_puzzle(&mut self) -> Result<NodePtr, DriverError>;
    fn spend_custom_coin(
        &mut self,
        coin: Coin,
        public_key: PublicKey,
        conditions: Conditions,
    ) -> Result<(), DriverError>;
}

// Implement the extension
impl CustomExt for SpendContext {
    // Implementation details...
}
```

## Error Handling

The SDK uses Rust's Result type for error handling. Most functions return a Result that may contain a DriverError:

```rust
fn some_operation() -> Result<(), DriverError> {
    // Implementation...
    Ok(())
}
```

## Working with the Network

To interact with the Chia blockchain network:

```rust
use chia_sdk_client::CoinsetClient;

async fn fetch_coin_records() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to testnet
    let client = CoinsetClient::testnet11();
    
    // Or connect to mainnet
    // let client = CoinsetClient::mainnet();
    
    // Get blockchain state
    let state = client.get_blockchain_state().await?;
    
    // Query coins by puzzle hash
    let puzzle_hash = /* your puzzle hash */;
    let coin_records = client.get_coin_records_by_puzzle_hash(puzzle_hash, None, None, None).await?;
    
    Ok(())
}
```
