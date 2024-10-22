# Chia Wallet SDK

[![crate](https://img.shields.io/crates/v/cni-wallet-sdk.svg)](https://crates.io/crates/cni-wallet-sdk)
[![documentation](https://docs.rs/cni-wallet-sdk/badge.svg)](https://docs.rs/cni-wallet-sdk)
[![minimum rustc 1.75](https://img.shields.io/badge/rustc-1.75+-red.svg)](https://rust-lang.github.io/rfcs/2495-min-rust-version.html)
[![codecov](https://codecov.io/github/Rigidity/cni-wallet-sdk/graph/badge.svg?token=M2MPMFGCCA)](https://codecov.io/github/Rigidity/cni-wallet-sdk)

This is an unofficial wallet SDK for the [Chia blockchain](https://chia.net), enabling the development of high-performance wallet applications built with Chia's [light wallet protocol](https://docs.chia.net/wallet-protocol).

![image](https://github.com/Chia-Network/cni-wallet-sdk/assets/35380458/06dd1f97-1f0e-4f6d-98cb-cbcb2b47ee70)

## Why do you need an SDK?

If you intend on writing an application that uses the Chia blockchain, be it a dApp, a wallet, or even just tooling on top of it, you will most likely need some code to interact with a Chia wallet. The worst case scenario is that you need to write an entire wallet and all of its driver code from scratch every time. This is very challenging to do, takes a lot of time, and can be error prone if anything is done wrong.

To build driver code, you need libraries like [chia-bls](https://docs.rs/chia-bls) and [clvmr](https://docs.rs/clvm), for interacting with Chia's native BLS signatures and CLVM runtime. You compose puzzles by currying them, and spend them by constructing solutions. Even with libraries in place to do this (assuming they are tested properly), it can be very tedious and hard to get right. That's what this wallet sdk is aiming to solve.

It's essentially a higher level wrapper over the core primitives that the Chia blockchain provides, and aims to make various things in the lifecycle of wallet development simpler such as state management and signing.

## chia_rs and clvm_rs

This SDK is built on top of the primitives developed in the [chia_rs](https://github.com/Chia-Network/chia_rs) and [clvm_rs](https://github.com/Chia-Network/clvm_rs) libraries. I help maintain chia_rs to add new core functionality necessary for wallet development as needed. And clvm_rs is a great implementation of the CLVM runtime, especially when combined with the [clvm-traits](https://docs.rs/clvm-traits/latest/clvm_traits/) helper library for translating Rust types to CLVM and vice versa.

## Supported primitives

Currently, the following Chia primitives are supported:

- [Standard Transactions](https://chialisp.com/standard-transactions), either as an inner puzzle or standalone
- [CATs](https://chialisp.com/cats) (Chia Asset Tokens), with creation, parsing, and spending capabilities
- [DIDs](https://chialisp.com/dids) (Decentralized Identities), with creation, parsing, and (limited) spending capabilities
- [NFTs](https://chialisp.com/nfts) (Non-Fungible Tokens), with minting and (limited) spending capabilities

Additionally, the wallet sdk is designed to be modular, so you can extend it with your own primitives and driver code if needed! Contributions are welcome for adding things to the wallet sdk itself as well.

## Credits

Special thanks to [SumSet Tech, LLC](https://sumset.tech) for sponsoring the initial development of various parts of the wallet sdk.

Banner image produced by [Midjourney](https://www.midjourney.com).
