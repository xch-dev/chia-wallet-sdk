# Dependencies

This is not a comprehensive list of every dependency chia-wallet-sdk has, but rather the foundations it's built upon.

## [chia_rs](https://github.com/Chia-Network/chia_rs)

This is an entire collection of low level crates for developing on the Chia blockchain, maintained by [Chia Network Inc](https://chia.net). One of its primary goals is to speed up aspects of the Chia full node through its Python bindings. However, we make use of a lot of the functionality in the Wallet SDK as well, to avoid reinventing the wheel.

For bls12_381 (BLS) signatures, we use chia-bls which itself currently depends on blst to implement the underlying cryptography efficiently and securely.

Similarly, the secp256k1 (K1) and secp256r1 (R1) curves are implemented in chia-secp via [k256](https://github.com/RustCrypto/elliptic-curves/tree/master/k256) and [p256](https://github.com/RustCrypto/elliptic-curves/tree/master/p256).

Common types for coins, spend bundles, and the wallet protocol, are implemented by chia-protocol. They can be serialized as JSON or via Chia's custom streamable binary serialization format.

Finally, clvm-traits, clvm-derive, and clvm-utils provide common utilities for encoding Rust types as CLVM values and vice versa. This makes it trivial to represent complex data structures in puzzles.

## [clvm_rs](https://github.com/Chia-Network/clvm_rs)

A low level runtime for the [Chialisp Virtual Machine (CLVM)](https://chialisp.com/clvm/). It's a necessary component to create and execute puzzles, but it's largely abstracted away by the utilities provided in chia_rs and chia-wallet-sdk.

## [chia_puzzles](https://github.com/Chia-Network/chia_puzzles)

This provides a single location to store standard Chia puzzles, so that they don't have to be copied into every project that relies upon them. While the Wallet SDK contains the actual wallet driver code to use these puzzles, chia_puzzles contains only the puzzle bytecode and hashes. Additionally, chia-puzzle-types (which is part of the chia_rs monorepo) contains data types for working with the arguments and solutions of the most common puzzles (for example XCH, CATs, NFTs, offers, and singletons).
