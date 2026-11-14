# chia-wallet-sdk

## Bindings

New feature work must be supported in the bindings in the same change, not left Rust-only or deferred to a follow-up. When you add or change public API (a puzzle, driver, primitive, helper, or a new option on an existing one), also:

- declare it in `bindings/<area>.json` and implement it in `crates/chia-sdk-bindings/src/<area>.rs`, registered in `lib.rs`;
- if it sits behind a Cargo feature, enable that feature in `crates/chia-sdk-bindings/Cargo.toml`;
- regenerate and commit `napi/index.d.ts`, `napi/index.js` (`pnpm install && pnpm build:debug` in `napi/`) and `pyo3/chia_wallet_sdk.pyi` (`cargo run -p pyo3-stub-generator`);
- test it from at least one binding, usually `napi/__test__/<area>.spec.ts`.

The only exceptions are items that can't be expressed in the bindings (generic traits, lifetimes, internal plumbing) or that bindings users have no use for. Say which ones you left out and why in the PR description.
