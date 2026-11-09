# Chia Wallet SDK for C++

C++ bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk), generated with [UniFFI](https://mozilla.github.io/uniffi-rs/) and [uniffi-bindgen-cpp](https://github.com/NordSecurity/uniffi-bindgen-cpp).

## Building

Install [uniffi-bindgen-cpp](https://github.com/NordSecurity/uniffi-bindgen-cpp) and run the generation script, which builds the library and writes the bindings to `cpp/generated`:

```bash
cargo install uniffi-bindgen-cpp --git https://github.com/NordSecurity/uniffi-bindgen-cpp --tag v0.10.0+v0.31.0
./cpp/generate.sh
```

This produces:

| File                                       | Description                                 |
| ------------------------------------------ | ------------------------------------------- |
| `chiawalletsdk.hpp`                        | The header to include                       |
| `chiawalletsdk.cpp`                        | The implementation, which must be compiled  |
| `chiawalletsdk_scaffolding.hpp`            | The FFI declarations used by the above      |
| `lib/libchia_wallet_sdk.{so,dylib,a}`      | The Rust library, as shared and static libraries |

Compile `chiawalletsdk.cpp` with C++20 and link it against the library. See [CMakeLists.txt](CMakeLists.txt), which builds and runs the tests:

```bash
cmake -S cpp -B cpp/build
cmake --build cpp/build
ctest --test-dir cpp/build
```

## Example

```cpp
#include <chiawalletsdk.hpp>
#include <iostream>

namespace sdk = chiawalletsdk;

int main() {
    auto sim = sdk::Simulator::init();
    auto clvm = sdk::Clvm::init();
    auto alice = sim->bls(1000);

    auto spend = clvm->delegated_spend({clvm->create_coin(alice->puzzle_hash(), 900, nullptr), clvm->reserve_fee(100)});
    clvm->spend_standard_coin(alice->coin(), alice->pk(), spend);
    sim->spend_coins(clvm->coin_spends(), {alice->sk()});

    auto coins = sim->unspent_coins(alice->puzzle_hash(), false);
    std::cout << "Created a coin with " << coins[0]->amount() << " mojos" << std::endl;
}
```

## Usage notes

- Objects are `std::shared_ptr`s, and constructors are named `init`. Optional objects are `nullptr` when absent.
- Errors are thrown as `chiawalletsdk::ChiaError`, which derives from `std::runtime_error`.
- Setters (such as `coin->set_amount(...)`) mutate the object in place. Use `clone()` to copy an object.
- `u64` values are `uint64_t`. Values that can exceed 64 bits (`BigInt` and `u128`) are decimal strings.
- Static methods are exported as functions prefixed with the class name, such as `chiawalletsdk::constants_cat_puzzle_hash()`.
- The generator doesn't support async functions, so the library is built with the `blocking` feature, which turns async methods (such as `RpcClient` requests) into blocking calls.
- Methods whose names are C++ keywords have an underscore appended, such as `clvm->int_("100")`. This and a few other generator bugs are worked around by `fix-bindings.py`.
