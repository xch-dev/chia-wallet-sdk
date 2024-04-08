[![crate](https://img.shields.io/crates/v/chia-wallet-sdk.svg)](https://crates.io/crates/chia-wallet-sdk)
[![documentation](https://docs.rs/chia-wallet-sdk/badge.svg)](https://docs.rs/chia-wallet-sdk)
[![minimum rustc 1.75](https://img.shields.io/badge/rustc-1.75+-red.svg)](https://rust-lang.github.io/rfcs/2495-min-rust-version.html)

This is an unofficial wallet SDK for the [Chia blockchain](https://chia.net), enabling the development of high-performance wallet applications built with Chia's [light wallet protocol](https://docs.chia.net/wallet-protocol).

## Why do you need an SDK?

If you intend on writing an application that uses the Chia blockchain, be it a dApp, a wallet, or even just tooling on top of it, you will most likely need some code to interact with a Chia wallet. The worst case scenario is that you need to write an entire wallet and all of its driver code from scratch every time. This is very challenging to do, takes a lot of time, and can be error prone if anything is done wrong.

To build driver code, you need libraries like `chia-bls` and `clvmr`, for interacting with Chia's native BLS signatures and CLVM runtime. You compose puzzles by currying them, and spend them by constructing solutions. Even with libraries in place to do this (assuming they are tested properly), it can be very tedious and hard to get right. That's what this Wallet SDK is aiming to solve.

It's essentially a higher level wrapper over the core primitives that the Chia blockchain provides, and aims to make various things in the lifecycle of wallet development simpler such as state management and signing.

## chia_rs and clvm_rs

This SDK is built on top of the primitives developed in the [chia_rs](https://github.com/Chia-Network/chia_rs) and [clvm_rs](https://github.com/Chia-Network/clvm_rs) libraries. I help maintain chia_rs to add new core functionality necessary for wallet development as needed. And clvm_rs is a great implementation of the CLVM runtime, especially when combined with the [clvm-traits](https://docs.rs/clvm-traits/latest/clvm_traits/) helper library for translating Rust types to CLVM and vice versa.

## Supported primitives

Currently, only a subset of Chia's primitives are supported:

### Standard puzzle

You can spend the standard puzzle, also known as the "standard transaction", "p2 puzzle", or "p2_delegated_puzzle_or_hidden_puzzle". The hidden puzzle functionality is not currently supported.

### CATs

CATs with any asset id can be spent, but only as long as the inner puzzle is the standard puzzle. You can also issue CATs with the "everything with signature" TAIL (multi-issuance).

### DIDs

You cannot create new DIDs or recover them, and the API for spending them is not as flexible as it will be in the future. But there is a very thin wrapper for spending DIDs, as long as the inner puzzle is the standard puzzle.

### NFTs

You can spend NFTs and mint them in bulk by spending a DID as the parent. You can also change the DID owner of an NFT. As previously mentioned, all primitives assume the standard puzzle is the inner puzzle, which is typically the case at this time.
