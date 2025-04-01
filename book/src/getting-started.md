# Installation

The Wallet SDK is written in Rust, so that's what you should use if you want to get the most out of it. However, there are also bindings provided for WebAssembly, Node.js, and Python. These bindings are roughly the same (aside from naming conventions and language differences), but they do not cover 100% of the API surface of the Rust crate. If anything is missing, feel free to submit an issue or PR on GitHub.

## Rust

First, make sure you have [installed Rust](https://rustup.rs/).

```bash
cargo add chia-wallet-sdk chia clvmr
```

## WebAssembly

Currently, in order to use the WASM bindings, you will need a bundler that supports loading `.wasm` files. For example, [Vite](https://vite.dev/) supports this well, with the [WASM plugin](https://www.npmjs.com/package/vite-plugin-wasm).

In the future, a plain web version of the WASM bindings may be released, but they are not at this time. You can build from source if you need that sooner.

{{#tabs global="npm" }}
{{#tab name="npm" }}

```bash
npm install chia-wallet-sdk-wasm
```

{{#endtab }}
{{#tab name="yarn" }}

```bash
yarn add chia-wallet-sdk-wasm
```

{{#endtab }}
{{#tab name="pnpm" }}

```bash
pnpm add chia-wallet-sdk-wasm
```

{{#endtab }}
{{#tab name="bun" }}

```bash
bun add chia-wallet-sdk-wasm
```

{{#endtab }}
{{#endtabs }}

## Node.js

The minimum supported version is Node.js v10.7.0.

{{#tabs global="npm" }}
{{#tab name="npm" }}

```bash
npm install chia-wallet-sdk
```

{{#endtab }}
{{#tab name="yarn" }}

```bash
yarn add chia-wallet-sdk
```

{{#endtab }}
{{#tab name="pnpm" }}

```bash
pnpm add chia-wallet-sdk
```

{{#endtab }}
{{#tab name="bun" }}

```bash
bun add chia-wallet-sdk
```

{{#endtab }}
{{#endtabs }}

## Python

The minimum supported version is Python 3.8.

```bash
pip install chia-wallet-sdk
```
