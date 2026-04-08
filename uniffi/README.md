# chia-wallet-sdk C# Bindings

This crate generates a native library for use with C# via [UniFFI](https://mozilla.github.io/uniffi-rs/).

## Building

```bash
cargo build -p chia-wallet-sdk-cs --release
```

The compiled library will be at `target/release/libchia_wallet_sdk.{dylib,so,dll}` depending on platform.

## Generating C# Bindings

Install `uniffi-bindgen-cs` matching the UniFFI version in use (`0.28`):

```bash
cargo install uniffi-bindgen-cs \
  --git https://github.com/NordSecurity/uniffi-bindgen-cs \
  --tag v0.8.0+v0.28.0
```

Then generate the C# source:

```bash
# macOS
uniffi-bindgen-cs generate \
  --library target/release/libchia_wallet_sdk.dylib \
  --out-dir uniffi/cs \
  --config uniffi/uniffi.toml

# Linux
uniffi-bindgen-cs generate \
  --library target/release/libchia_wallet_sdk.so \
  --out-dir uniffi/cs \
  --config uniffi/uniffi.toml
```

The generated C# file at `uniffi/cs/chia_wallet_sdk.cs` can be included in any .NET project.

## Type Mapping

| Rust type | C# type |
|-----------|---------|
| `Vec<u8>` (bytes) | `byte[]` |
| `u64`, `u128`, `BigInt` | `string` (parse with `BigInteger.Parse()`) |
| `bool` | `bool` |
| `String` | `string` |
| `Option<T>` | nullable |
| `Vec<T>` | `List<T>` |
| Rust struct (Class) | C# class |
| Rust enum | C# enum |

## Known Limitations

- `BigInt`/`u128`/`u64` are passed as strings; use `System.Numerics.BigInteger.Parse()` on the C# side.
- `Clvm.Alloc()` requires a typed `ClvmType` enum value (unlike Python which accepts dynamic types).
- Field setters (`SetField()`) return a new object with the field changed (immutable update pattern).
- `uniffi-bindgen-cs` version must stay in sync with the `uniffi` crate version (currently `0.28`).
