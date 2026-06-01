// Mirrors go/tests/bindings_test.go (TestAlloc) and the C# BasicTests: builds a
// heterogeneous CLVM list and checks its serialization against a known-good hex string.

#include <cstdint>
#include <iostream>
#include <memory>
#include <string>
#include <vector>

#include "chia_wallet_sdk.hpp"

using namespace chia_wallet_sdk;

namespace {

std::string bytes_to_hex(const std::vector<uint8_t> &bytes) {
    static const char *digits = "0123456789abcdef";
    std::string out;
    out.reserve(bytes.size() * 2);
    for (uint8_t b : bytes) {
        out.push_back(digits[b >> 4]);
        out.push_back(digits[b & 0x0f]);
    }
    return out;
}

} // namespace

int main() {
    auto clvm = Clvm::init();

    auto nil_prog = clvm->nil();

    auto pk = PublicKey::infinity();
    auto pk_prog = clvm->alloc(ClvmType(ClvmType::kPublicKey{pk}));

    auto hello_prog = clvm->string("Hello, world!");
    auto forty_two = clvm->int_("42");
    auto hundred = clvm->int_("100");
    auto true_prog = clvm->bool_(true);
    auto atom_prog = clvm->atom({1, 2, 3});
    auto zeroes_prog = clvm->atom(std::vector<uint8_t>(32, 0));
    auto nil2 = clvm->nil();
    auto nil3 = clvm->nil();

    auto rc_nil1 = clvm->nil();
    auto rc_nil2 = clvm->nil();
    auto run_cat_tail = RunCatTail::init(rc_nil1, rc_nil2);
    auto run_cat_tail_prog = clvm->alloc(ClvmType(ClvmType::kRunCatTail{run_cat_tail}));

    auto program = clvm->list(std::vector<std::shared_ptr<Program>>{
        nil_prog,
        pk_prog,
        hello_prog,
        forty_two,
        hundred,
        true_prog,
        atom_prog,
        zeroes_prog,
        nil2,
        nil3,
        run_cat_tail_prog,
    });

    // Known-good serialization, identical to go/tests/bindings_test.go.
    const std::string expected =
        "ff80ffb0c0000000000000000000000000000000000000000000000000000000000000000000000000"
        "0000000000000000000000ff8d48656c6c6f2c20776f726c6421ff2aff64ff01ff83010203ffa00000"
        "000000000000000000000000000000000000000000000000000000000000ff80ff80ffff33ff80ff81"
        "8fff80ff808080";

    const std::string got = bytes_to_hex(program->serialize());

    if (got != expected) {
        std::cerr << "serialization mismatch\n  got:  " << got << "\n  want: " << expected
                  << "\n";
        return 1;
    }

    std::cout << "alloc_test passed\n";
    return 0;
}
