# Crates

The library is split into a bunch of individual crates in a [Cargo workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html). This has a few benefits:

1. Enforces a separation of concerns, which makes code easier to manage.
2. If needed, you can depend on smaller subsets of the library to reduce dependencies and binary size.
3. Bindings can be built as part of the same repository without introducing additional dependencies.

## chia-sdk-driver

This is the most important, since it implements all of the wallet driver code for each primitive the Wallet SDK supports. It also provides the SpendContext for managing lots of complex coin spends in an easier way.

## chia-sdk-test

A Simulator testing framework for Chia, similar to the official full node simulator but more lightweight and wallet focused. Instead of including a proper mempool, it treats every transaction as its own block and validates the coin spends. This makes unit testing very fast and easy.

Note that this also provides a PeerSimulator, for simulating the wallet protocol via the chia-sdk-client Peer just like you would when connecting to an actual full node. It only implements a subset of the protocol, but it's nice to have for integration testing in wallets for example.

## chia-sdk-client

A client implementation of the Chia wallet protocol, with an easy to use interface. Provides both a native-tls and rustls option, to make it more portable to a wider variety of devices. You can connect to introducers, find and connect to peers, and make requests to get coin data or send transactions, entirely peer to peer.

## chia-sdk-coinset

As an alternative to chia-sdk-client (and one that can be used on the web as well), you can use the Coinset API. It can also potentially point to your local full node RPC since it has the same (for the most part) endpoints.

## chia-sdk-signer

Provides utilities for calculating the required signatures for a given unsigned spend bundle. The usefulness of this varies depending on the type of wallet you are building. For example, with BLS wallets it is common to generate an unsigned spend bundle and sign it later. Whereas with vaults, you usually will want to only put together the spend bundle at the end, once you have all of the required signatures already.

## chia-sdk-types

Provides common types for interacting with Chia puzzles, such as conditions and merkle trees. It also currently defines puzzle types that are missing in the chia-puzzle-types library in chia_rs.

## chia-sdk-derive

A procedural macro to make implementing the condition types in chia-sdk-types easier. Currently that's all this crate does, so it's not particularly interesting.

## chia-sdk-utils

Utilities for addresses and coin selection. Hopefully the scope of this can expand in the future to simplify the development process.

## chia-sdk-bindings

The Rust implementation of the Wallet SDK bindings. It's powered by the bindy and bindy-macro crates, which convert JSON files into Node.js, WASM, and Python bindings.
