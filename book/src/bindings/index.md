# Bindings

The Chia Wallet SDK provides bindings for multiple languages, allowing you to use the SDK's functionality from your preferred programming environment.

## Available Bindings

- [Node.js](./nodejs.md) - Use the SDK in TypeScript/JavaScript applications
- [Python](./python.md) - Use the SDK in Python applications
- [WebAssembly (WASM)](./wasm.md) - Use the SDK in web applications

Each binding provides access to the same core functionality, with language-specific adaptations for idiomatic usage.

## Common Features Across Bindings

All bindings provide access to:

- Key management (creating, deriving, and using keys with mnemonics)
- Address handling (encoding and decoding addresses)
- Transaction creation and signing
- Network interaction (querying the blockchain and submitting transactions)
- Working with special assets like CATs (Colored Coins) and NFTs
- Testing with the simulator

## Best Practices

When using the bindings, follow these best practices:

1. **Use mnemonics for key generation** - Generate keys from mnemonics rather than raw seeds for better security and compatibility with wallet standards
2. **Test with the simulator** - Use the built-in simulator for testing transactions before deploying to testnet or mainnet
3. **Properly verify signatures** - Use the correct signature verification methods
4. **Handle memory management** - For WASM bindings, ensure proper memory cleanup
5. **Use TypeScript for Node.js and WASM** - TypeScript provides better type safety and developer experience
