# Chia Wallet SDK

[![crate](https://img.shields.io/crates/v/chia-wallet-sdk.svg)](https://crates.io/crates/chia-wallet-sdk)
[![documentation](https://docs.rs/chia-wallet-sdk/badge.svg)](https://docs.rs/chia-wallet-sdk)
[![npm](https://img.shields.io/npm/v/chia-wallet-sdk.svg)](https://www.npmjs.com/package/chia-wallet-sdk)
[![PyPI](https://img.shields.io/pypi/v/chia-wallet-sdk.svg)](https://pypi.org/project/chia-wallet-sdk)

A high-performance Rust SDK for building applications that interact with coins on the [Chia blockchain](https://chia.net), with bindings for Node.js, WebAssembly, and Python.

The Wallet SDK handles the low-level details of Chia's coin set model so you can focus on your application: constructing and parsing puzzles, building spend bundles, calculating required signatures, talking to full nodes, and testing everything against a simulator.

> [!NOTE]
> This is a library, _not_ a prebuilt wallet. If you're looking for a ready-to-use wallet, check out [Sage](https://github.com/xch-dev/sage), a light wallet built on top of the Wallet SDK that also exposes an RPC interface.

## Features

- **Puzzle drivers** for standard primitives, including CATs, NFTs, DIDs, singletons, vaults (MIPS), DataLayer stores, clawbacks, options, and streamed assets
- **Offers** - create, inspect, and take Chia offer files
- **Action system** for describing high-level operations (sends, issuance, minting, etc) and letting the SDK assemble the spends
- **Signing** - compute the BLS and secp signatures required by a set of coin spends
- **Networking** - connect to full node peers over the light wallet protocol, query the [coinset.org](https://coinset.org) API, or talk to a local Chia daemon via RPC
- **Testing** - an in-memory blockchain simulator for exercising spends without a real network
- **Typed conditions and puzzle info** shared across the whole stack

## Packages

| Language    | Package                                                                      | Install                            |
| ----------- | ---------------------------------------------------------------------------- | ---------------------------------- |
| Rust        | [`chia-wallet-sdk`](https://crates.io/crates/chia-wallet-sdk)                | `cargo add chia-wallet-sdk`        |
| Node.js     | [`chia-wallet-sdk`](https://www.npmjs.com/package/chia-wallet-sdk)           | `npm install chia-wallet-sdk`      |
| WebAssembly | [`chia-wallet-sdk-wasm`](https://www.npmjs.com/package/chia-wallet-sdk-wasm) | `npm install chia-wallet-sdk-wasm` |
| Python      | [`chia-wallet-sdk`](https://pypi.org/project/chia-wallet-sdk)                | `pip install chia-wallet-sdk`      |

The bindings are generated from a single definition ([`bindings.json`](bindings.json) and [`bindings/`](bindings)), so the API is consistent across languages.

The Rust crate re-exports a set of smaller crates, which can also be used individually:

| Crate                                         | Description                                                       |
| --------------------------------------------- | ----------------------------------------------------------------- |
| [`chia-sdk-driver`](crates/chia-sdk-driver)   | Puzzle drivers, spend construction, offers, and the action system |
| [`chia-sdk-types`](crates/chia-sdk-types)     | Conditions, puzzle arguments, and other shared types              |
| [`chia-sdk-signer`](crates/chia-sdk-signer)   | Calculates the signatures required for coin spends                |
| [`chia-sdk-client`](crates/chia-sdk-client)   | Full node peer connections via the light wallet protocol          |
| [`chia-sdk-coinset`](crates/chia-sdk-coinset) | Client for the coinset.org and full node RPC APIs                 |
| [`chia-sdk-daemon`](crates/chia-sdk-daemon)   | Websocket client for the Chia daemon                              |
| [`chia-sdk-test`](crates/chia-sdk-test)       | Blockchain simulator and testing utilities                        |
| [`chia-sdk-utils`](crates/chia-sdk-utils)     | Address encoding and other general utilities                      |

### Optional features

| Feature                 | Description                                                                                               |
| ----------------------- | --------------------------------------------------------------------------------------------------------- |
| `chip-0007`             | Off-chain NFT metadata ([CHIP-0007](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0007.md), [CHIP-0015](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0015.md)) |
| `chip-0008`             | Splitter puzzle ([CHIP-0008](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0008.md))         |
| `chip-0035`             | DataLayer store support ([CHIP-0035](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0035.md)) |
| `chip-0037`             | EIP-712 vault members ([CHIP-0037](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0037.md))   |
| `chip-0052`             | Partial offers ([CHIP-0052](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0052.md))          |
| `action-layer`          | Action layer puzzles and drivers                                                                          |
| `auction`               | English auctions for NFTs, built on the action layer                                                      |
| `offer-compression`     | Compression and decompression of offer files                                                              |
| `peer-simulator`        | Simulated full node peer for networking tests                                                             |
| `native-tls` / `rustls` | TLS backend for network connections                                                                       |

## Getting Started

If you're new to developing on Chia, [xch.dev](https://xch.dev) is a great place to learn about the coin set model, Chialisp, and the standard puzzles.

From there:

- Browse the [Rust API docs](https://docs.rs/chia-wallet-sdk/latest/chia_wallet_sdk)
- Check out the [examples](examples), such as [spending CATs](examples/cat_spends.rs) or [using the simulator](examples/spend_simulator.rs)
- Look at the binding tests in [`napi/__test__`](napi/__test__), [`wasm/__test__`](wasm/__test__), and [`pyo3/tests`](pyo3/tests) for usage from JavaScript and Python

You can run an example with:

```bash
cargo run --example cat_spends
```

## Credits

Special thanks to:

- [SumSet Tech, LLC](https://sumset.tech) for sponsoring the initial development of the Wallet SDK
- [Solomons Lot](https://solslot.com) for sponsoring the WASM bindings
- [FireAcademy.io](https://www.fireacademy.io/)
- All of the open source contributors who have helped it get this far

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
