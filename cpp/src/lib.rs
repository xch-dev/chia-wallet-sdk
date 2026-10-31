#![allow(clippy::too_many_arguments)]
// UniFFI's setup_scaffolding! macro emits raw FFI extern blocks which require unsafe.
#![allow(unsafe_code)]

use std::sync::Arc;

// UniFFI scaffolding. The namespace stays `chia_wallet_sdk` (FFI symbol and generated
// file names) even though the [lib] name in Cargo.toml is crate-specific.
uniffi::setup_scaffolding!("chia_wallet_sdk");

// Generate all bindings from the JSON schemas.
// The C++ backend uses the `_sync` variant because uniffi-bindgen-cpp does not support
// async functions. Async methods (e.g. RpcClient request methods, Peer) are therefore
// exposed as blocking calls that drive the future to completion on a shared Tokio runtime.
// Bound them with the client-level timeouts (RpcClientOptions / PeerOptions); there is no
// global backstop.
bindy_macro::bindy_uniffi_sync!("bindings.json");

// Hand-written `Clvm::alloc`, shared across the uniffi crates.
include!("../../uniffi/clvm_alloc.rs");
