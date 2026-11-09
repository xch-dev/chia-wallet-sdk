# UniFFI bindings

This crate exposes the Chia Wallet SDK through [UniFFI](https://mozilla.github.io/uniffi-rs/), which is used to generate bindings for languages other than JavaScript and Python:

- [Go](../go)
- [.NET](../dotnet)
- [C++](../cpp)

The bindings are generated from the same JSON definitions as the other bindings (see `bindings.json` and the `bindings` directory) by the `bindy_uniffi!` macro. The only hand-written part is `Clvm::alloc`.

## Building

```bash
cargo build --profile release-uniffi -p chia-wallet-sdk-uniffi
```

The `release-uniffi` profile is the same as `release`, except that it keeps the symbol table, which the generators read the interface from.

Async methods are exported as async functions, except when the `blocking` feature is enabled. This is needed for generators that don't support async functions (C++).

## Other languages

UniFFI has built-in support for Kotlin, Swift, and Python, which can be generated with the `uniffi-bindgen` binary. These aren't tested or packaged yet (and the Swift bindings currently fail to compile, since fields named `handle` conflict with a method that UniFFI generates):

```bash
cargo run -p chia-wallet-sdk-uniffi --features cli --bin uniffi-bindgen -- generate --library target/release-uniffi/libchia_wallet_sdk.dylib --language kotlin --out-dir out
```

The generator settings for each language are in [uniffi.toml](uniffi.toml).
