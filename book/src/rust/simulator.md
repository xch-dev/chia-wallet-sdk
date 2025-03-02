# Simulator

When testing wallet code, it's best not to use mainnet, because you would lose funds if anything went wrong. And while testnet11 would be a fine alternative, it still requires setup and getting test funds to use. Because live networks have a persistent state and every transaction block takes nearly a minute, it can be tedious to test this way. It's also very impractical for unit testing, where you need fast and deterministic results every time.

Chia provides a full node simulator, which works very well for emulating the whole blockchain. But this typically has to run as a standalone service, rather than being embedded in your tests, and even though it's faster than live networks, it's still slower than would be ideal for a unit test.

Because of this, the Wallet SDK provides a `Simulator` which allows you to test transactions efficiently with minimal setup. This is the recommended way to test out primitives.

## Setup

First you will need to create a simulator instance:

```rs
let mut sim = Simulator::new();
let mut ctx = SpendContext::new();
```
