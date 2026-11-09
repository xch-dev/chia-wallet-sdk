//! An unofficial SDK for building wallets and other applications on the
//! [Chia blockchain](https://chia.net).
//!
//! The SDK covers the full lifecycle of a transaction: building coin spends for the standard
//! puzzle, CATs, NFTs, DIDs, vaults, offers, and more, calculating and making the signatures
//! they need, testing them against a simulator, and submitting them to the network through a
//! full node peer or an RPC API. It is not a prebuilt wallet. For that, see
//! [Sage Wallet](https://github.com/xch-dev/sage), which is built with this SDK.
//!
//! To learn more about developing on Chia and its coin set model, see
//! [xch.dev](https://xch.dev).
//!
//! # Crates
//!
//! This crate re-exports the crates that make up the SDK, so that they can be used through a
//! single dependency:
//!
//! | Module | Crate | Purpose |
//! | --- | --- | --- |
//! | [`driver`] | `chia-sdk-driver` | Building and parsing spends of puzzles, with [`SpendContext`](driver::SpendContext), layers such as [`StandardLayer`](driver::StandardLayer), and primitives such as [`Cat`](driver::Cat), [`Nft`](driver::Nft), [`Did`](driver::Did), and [`Offer`](driver::Offer). |
//! | [`types`] | `chia-sdk-types` | Conditions, puzzle argument and solution types, and consensus constants. |
//! | [`signer`] | `chia-sdk-signer` | Calculating the signatures a transaction needs with [`RequiredSignature`](signer::RequiredSignature). |
//! | [`test`](mod@test) | `chia-sdk-test` | An in-memory [`Simulator`](test::Simulator) for testing spends, and test key pairs. |
//! | [`client`] | `chia-sdk-client` | Connecting to full nodes with the wallet protocol through a [`Peer`](client::Peer). |
//! | [`coinset`] | `chia-sdk-coinset` | Calling the full node RPC API, such as [coinset.org](https://coinset.org), through [`ChiaRpcClient`](coinset::ChiaRpcClient). |
//! | [`daemon`] | `chia-sdk-daemon` | Connecting to the Chia daemon over its websocket. |
//! | [`utils`] | `chia-sdk-utils` | Addresses, coin selection, and hex parsing. |
//!
//! The lower level crates from Chia that the SDK is built on are re-exported as well:
//!
//! | Module | Crate | Purpose |
//! | --- | --- | --- |
//! | [`clvmr`] | `clvmr` | The CLVM runtime, including the [`Allocator`](clvmr::Allocator). |
//! | [`clvm_traits`] | `clvm-traits` | Converting Rust types to and from CLVM with [`ToClvm`](clvm_traits::ToClvm) and [`FromClvm`](clvm_traits::FromClvm). |
//! | [`clvm_utils`] | `clvm-utils` | Tree hashing and currying. |
//! | [`puzzles`] | `chia-puzzles` | The compiled bytecode and hashes of Chia's standard puzzles. |
//! | [`chia::protocol`] | `chia-protocol` | Core types such as [`Coin`](chia::protocol::Coin), [`CoinSpend`](chia::protocol::CoinSpend), and [`SpendBundle`](chia::protocol::SpendBundle), and the network protocol messages. |
//! | [`chia::bls`] | `chia-bls` | BLS keys and signatures. |
//! | [`chia::secp`] | `chia-secp` | secp256k1 and secp256r1 keys and signatures. |
//! | [`chia::puzzle_types`] | `chia-puzzle-types` | The curried arguments and solutions of Chia's standard puzzles. |
//! | [`chia::consensus`] | `chia-consensus` | Consensus constants and spend validation. |
//! | [`chia::traits`] | `chia-traits` | The [`Streamable`](chia::traits::Streamable) serialization format. |
//! | [`chia::sha2`] | `chia-sha2` | SHA-256 hashing. |
//! | [`chia::ssl`] | `chia-ssl` | TLS certificates for connecting to Chia services. |
//! | [`chia::serde`] | `chia-serde` | Serde helpers for Chia types. |
//!
//! # Prelude
//!
//! The [`prelude`] re-exports the most commonly used types and functions from all of these,
//! so most applications can start with:
//!
//! ```
//! use chia_wallet_sdk::prelude::*;
//! ```
//!
//! # Feature flags
//!
//! None of these are enabled by default.
//!
//! * `native-tls`: Enables connecting to peers, full node RPC servers, and the daemon using the
//!   platform's TLS library.
//! * `rustls`: The same as `native-tls`, but uses `rustls` instead.
//! * `chip-0035`: Enables the data layer store primitives from CHIP-0035, such as `Datastore`.
//! * `chip-0037`: Enables the `P2ControllerPuzzleLayer` and `P2Eip712MessageLayer` puzzle layers
//!   from CHIP-0037.
//! * `action-layer`: Enables the action layer and the primitives built on it, such as
//!   `RewardDistributor` and `CatalogRegistry`. This also enables `chip-0035`.
//! * `offer-compression`: Enables compressing and encoding offers as `offer1...` strings, with
//!   functions such as `encode_offer` and `decode_offer`.
//! * `peer-simulator`: Enables `PeerSimulator`, a local full node that a [`Peer`](client::Peer)
//!   can connect to in tests.
//!
//! # Quick start
//!
//! Send XCH from one key to another in the [`Simulator`](test::Simulator), signing the
//! transaction the same way you would for a real network:
//!
//! ```
//! use chia_wallet_sdk::chia::bls::sign;
//! use chia_wallet_sdk::prelude::*;
//!
//! let mut sim = Simulator::new();
//! let mut ctx = SpendContext::new();
//!
//! // Alice has a key with a coin worth 1,000 mojos, locked with the standard puzzle.
//! let alice = sim.bls(1_000);
//! let alice_p2 = StandardLayer::new(alice.pk);
//!
//! // Bob gave Alice his address.
//! let bob_address = Address::new(Bytes32::new([1; 32]), "txch".to_string()).encode()?;
//! let bob_puzzle_hash = Address::decode(&bob_address)?.expect_prefix("txch")?;
//!
//! // Send 300 mojos to Bob, pay a 50 mojo fee, and send the change back to Alice.
//! // The hints let wallets find the coins by puzzle hash.
//! let bob_hint = ctx.hint(bob_puzzle_hash)?;
//! let alice_hint = ctx.hint(alice.puzzle_hash)?;
//!
//! alice_p2.spend(
//!     &mut ctx,
//!     alice.coin,
//!     Conditions::new()
//!         .create_coin(bob_puzzle_hash, 300, bob_hint)
//!         .create_coin(alice.puzzle_hash, 650, alice_hint)
//!         .reserve_fee(50),
//! )?;
//!
//! let coin_spends = ctx.take();
//!
//! // Calculate the required signatures, and sign each of them with Alice's key.
//! let mut allocator = Allocator::new();
//! let constants = AggSigConstants::from(&*TESTNET11_CONSTANTS);
//! let required = RequiredSignature::from_coin_spends(&mut allocator, &coin_spends, &constants)?;
//!
//! let mut signature = Signature::default();
//!
//! for required in required {
//!     let RequiredSignature::Bls(required) = required else {
//!         unreachable!("the standard puzzle only uses BLS signatures");
//!     };
//!     assert_eq!(required.public_key, alice.pk);
//!     signature += &sign(&alice.sk, required.message());
//! }
//!
//! // On a real network, this spend bundle would be submitted to a peer or an RPC API.
//! sim.new_transaction(SpendBundle::new(coin_spends, signature))?;
//!
//! let bob_coins = sim.unspent_coins(bob_puzzle_hash, false);
//! assert_eq!(bob_coins.len(), 1);
//! assert_eq!(bob_coins[0].amount, 300);
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Issue a CAT, and let the simulator sign the transaction:
//!
//! ```
//! use chia_wallet_sdk::prelude::*;
//!
//! let mut sim = Simulator::new();
//! let mut ctx = SpendContext::new();
//!
//! let alice = sim.bls(1_000);
//! let alice_p2 = StandardLayer::new(alice.pk);
//!
//! // Issue 1,000 mojos of a new CAT to Alice. Its asset id is derived from Alice's coin id,
//! // so no more of it can ever be issued.
//! let hint = ctx.hint(alice.puzzle_hash)?;
//! let (issue_cat, cats) = Cat::single_issuance(
//!     &mut ctx,
//!     alice.coin.coin_id(),
//!     None,
//!     1_000,
//!     Conditions::new().create_coin(alice.puzzle_hash, 1_000, hint),
//! )?;
//! alice_p2.spend(&mut ctx, alice.coin, issue_cat)?;
//!
//! sim.spend_coins(ctx.take(), &[alice.sk])?;
//!
//! let cat = cats[0];
//! assert_eq!(cat.coin.amount, 1_000);
//! assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
//! assert!(sim.coin_state(cat.coin.coin_id()).is_some_and(|state| state.spent_height.is_none()));
//! # Ok::<(), anyhow::Error>(())
//! ```

#![cfg_attr(not(test), warn(missing_docs))]

pub mod prelude;

/// Connecting to full nodes using the wallet protocol. Re-export of `chia-sdk-client`.
pub use chia_sdk_client as client;
/// A client for the full node RPC API, such as coinset.org. Re-export of `chia-sdk-coinset`.
pub use chia_sdk_coinset as coinset;
/// A websocket client for the Chia daemon. Re-export of `chia-sdk-daemon`.
pub use chia_sdk_daemon as daemon;
/// Building and parsing spends of Chia puzzles. Re-export of `chia-sdk-driver`.
pub use chia_sdk_driver as driver;
/// Calculating the signatures required by coin spends. Re-export of `chia-sdk-signer`.
pub use chia_sdk_signer as signer;
/// A blockchain simulator and helpers for testing spends. Re-export of `chia-sdk-test`.
pub use chia_sdk_test as test;
/// Conditions, puzzle types, and consensus constants. Re-export of `chia-sdk-types`.
pub use chia_sdk_types as types;
/// Addresses, coin selection, and hex parsing. Re-export of `chia-sdk-utils`.
pub use chia_sdk_utils as utils;

/// The compiled bytecode and tree hashes of Chia's standard puzzles. Re-export of
/// `chia-puzzles`.
pub use chia_puzzles as puzzles;
/// Converting Rust types to and from CLVM. Re-export of `clvm-traits`.
pub use clvm_traits;
/// Tree hashing and currying of CLVM programs. Re-export of `clvm-utils`.
pub use clvm_utils;
/// The CLVM runtime. Re-export of `clvmr`.
pub use clvmr;

/// Re-exports of the crates from Chia's `chia_rs` repository that the SDK is built on.
pub mod chia {
    /// BLS keys and signatures. Re-export of `chia-bls`.
    pub use chia_bls as bls;
    /// Consensus constants and spend validation. Re-export of `chia-consensus`.
    pub use chia_consensus as consensus;
    /// Core blockchain types and network protocol messages. Re-export of `chia-protocol`.
    pub use chia_protocol as protocol;
    /// Curried arguments and solutions of Chia's standard puzzles. Re-export of
    /// `chia-puzzle-types`.
    pub use chia_puzzle_types as puzzle_types;
    /// secp256k1 and secp256r1 keys and signatures. Re-export of `chia-secp`.
    pub use chia_secp as secp;
    /// Serde helpers for Chia types. Re-export of `chia-serde`.
    pub use chia_serde as serde;
    /// SHA-256 hashing. Re-export of `chia-sha2`.
    pub use chia_sha2 as sha2;
    /// TLS certificates for connecting to Chia services. Re-export of `chia-ssl`.
    pub use chia_ssl as ssl;
    /// The `Streamable` serialization format. Re-export of `chia-traits`.
    pub use chia_traits as traits;
}
