# Chia Wallet SDK

[![crate](https://img.shields.io/crates/v/chia-wallet-sdk.svg)](https://crates.io/crates/chia-wallet-sdk)
[![documentation](https://docs.rs/chia-wallet-sdk/badge.svg)](https://docs.rs/chia-wallet-sdk)
[![minimum rustc 1.75](https://img.shields.io/badge/rustc-1.75+-red.svg)](https://rust-lang.github.io/rfcs/2495-min-rust-version.html)

This is an unofficial wallet SDK for the [Chia blockchain](https://chia.net), enabling the development of high-performance wallet applications built with Chia's [light wallet protocol](https://docs.chia.net/wallet-protocol).

![image](https://github.com/Rigidity/chia-wallet-sdk/assets/35380458/06dd1f97-1f0e-4f6d-98cb-cbcb2b47ee70)

## Why do you need an SDK?

If you intend on writing an application that uses the Chia blockchain, be it a dApp, a wallet, or even just tooling on top of it, you will most likely need some code to interact with a Chia wallet. The worst case scenario is that you need to write an entire wallet and all of its driver code from scratch every time. This is very challenging to do, takes a lot of time, and can be error prone if anything is done wrong.

To build driver code, you need libraries like `chia-bls` and `clvmr`, for interacting with Chia's native BLS signatures and CLVM runtime. You compose puzzles by currying them, and spend them by constructing solutions. Even with libraries in place to do this (assuming they are tested properly), it can be very tedious and hard to get right. That's what this wallet sdk is aiming to solve.

It's essentially a higher level wrapper over the core primitives that the Chia blockchain provides, and aims to make various things in the lifecycle of wallet development simpler such as state management and signing.

## chia_rs and clvm_rs

This SDK is built on top of the primitives developed in the [chia_rs](https://github.com/Chia-Network/chia_rs) and [clvm_rs](https://github.com/Chia-Network/clvm_rs) libraries. I help maintain chia_rs to add new core functionality necessary for wallet development as needed. And clvm_rs is a great implementation of the CLVM runtime, especially when combined with the [clvm-traits](https://docs.rs/clvm-traits/latest/clvm_traits/) helper library for translating Rust types to CLVM and vice versa.

## Supported primitives

Currently, the following Chia primitives are supported:

### P2 Puzzle (Standard Transaction)

You can spend the [standard transaction](https://chialisp.com/standard-transactions), either as an inner puzzle or by itself, with a list of conditions.

Note that the "hidden puzzle" functionality is not currently supported by the wallet sdk, and the (unspendable) `DEFAULT_HIDDEN_PUZZLE` will be used.

### CATs (Chia Asset Tokens)

You can spend a CAT with any TAIL (Token Asset Issuance Limitations) program, whose hash is otherwise known as an asset id. You can also issue CATs with the "everything with signature" TAIL (multi-issuance) or "genesis by coin id" TAIL (single-issuance).

Note that it is not currently possible to melt a CAT.

You can also parse an unknown puzzle into the info required to spend a CAT.

### DIDs (Decentralized Identifiers)

You can create new DIDs and spend them to mint NFTs with them. You cannot currently update the metadata, recover, or transfer a DID.

You can also parse an unknown puzzle into the info required to spend a DID.

### NFTs (Non-Fungible Tokens)

You can mint NFTs in bulk by spending a DID to creating an intermediate coin that launches the NFT. You can also spend and change the DID owner of an NFT.

Note that you _cannot_ yet parse NFTs from unknown puzzles.
