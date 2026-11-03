#![allow(clippy::too_many_arguments)]
// UniFFI's setup_scaffolding! macro emits raw FFI extern blocks which require unsafe.
#![allow(unsafe_code)]

use std::sync::Arc;

// UniFFI scaffolding. The namespace stays `chia_wallet_sdk` (FFI symbol and generated
// file names) even though the [lib] name in Cargo.toml is crate-specific.
uniffi::setup_scaffolding!("chia_wallet_sdk");

// Generate all bindings from the JSON schemas
bindy_macro::bindy_uniffi!("bindings.json");

// Hand-written `Clvm::alloc`, shared across the uniffi crates.
include!("../../uniffi/clvm_alloc.rs");
