// C++ binding tests. Mirrors dotnet/tests/BasicTests.cs (the canonical suite) so that
// the Go, C#, C++, and Python bindings all exercise equivalent behavior. Each test is a
// function registered in `kTests`; CMake/ctest runs each one by name.

#include <cstdint>
#include <functional>
#include <iostream>
#include <map>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include "chia_wallet_sdk.hpp"

using namespace chia_wallet_sdk;

namespace {

void require(bool condition, const std::string &message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

void require_eq_str(const std::string &actual, const std::string &expected,
                    const std::string &what) {
    if (actual != expected) {
        throw std::runtime_error(what + " mismatch\n  got:  " + actual + "\n  want: " + expected);
    }
}

// -- Tests ----------------------------------------------------------------------------

void to_hex_from_hex_roundtrip() {
    auto bytes = from_hex("ff");
    require_eq_str(to_hex(bytes), "ff", "to_hex(from_hex)");
}

void bytes_equal_equal() {
    require(bytes_equal({1, 2, 3}, {1, 2, 3}), "bytes_equal should be true");
}

void bytes_equal_not_equal() {
    require(!bytes_equal({1, 2, 3}, {1, 2, 4}), "bytes_equal should be false");
}

void coin_id_known_value() {
    auto coin = Coin::init(
        from_hex("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
        from_hex("dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986"),
        "100");
    require_eq_str(to_hex(coin->coin_id()),
                   "fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a",
                   "coin_id");
}

void atom_roundtrip() {
    auto clvm = Clvm::init();
    std::vector<uint8_t> expected{1, 2, 3};
    auto program = clvm->atom(expected);
    auto atom = program->to_atom();
    require(atom.has_value(), "to_atom should be present");
    require(*atom == expected, "atom roundtrip");
}

void string_roundtrip() {
    auto clvm = Clvm::init();
    const std::string expected = "hello world";
    auto program = clvm->atom(std::vector<uint8_t>(expected.begin(), expected.end()));
    auto str = program->to_string();
    require(str.has_value(), "to_string should be present");
    require_eq_str(*str, expected, "string roundtrip");
}

void int_roundtrip() {
    auto clvm = Clvm::init();
    for (const std::string &value : {"0", "1", "420", "-1", "-100", "67108863"}) {
        auto program = clvm->int_(value);
        auto got = program->to_int();
        require(got.has_value(), "to_int should be present");
        require_eq_str(*got, value, "int roundtrip");
    }
}

void pair_roundtrip() {
    auto clvm = Clvm::init();
    auto first = clvm->int_("1");
    auto rest = clvm->int_("100");
    auto pair = clvm->pair(first, rest);
    auto result = pair->to_pair();
    require(result != nullptr, "to_pair should be present");
    require_eq_str(result->get_first()->to_int().value(), "1", "pair first");
    require_eq_str(result->get_rest()->to_int().value(), "100", "pair rest");
}

void public_key_roundtrip() {
    auto original = PublicKey::infinity();
    auto bytes = original->to_bytes();
    auto restored = PublicKey::from_bytes(bytes);
    require(bytes_equal(original->to_bytes(), restored->to_bytes()), "public key roundtrip");
}

void clvm_serialization() {
    auto clvm = Clvm::init();
    struct Case {
        std::shared_ptr<Program> program;
        std::string hex;
    };
    std::vector<Case> cases{
        {clvm->atom({1, 2, 3}), "83010203"},
        {clvm->int_("420"), "8201a4"},
        {clvm->int_("100"), "64"},
        {clvm->pair(clvm->atom({1, 2, 3}), clvm->int_("100")), "ff8301020364"},
    };
    for (auto &c : cases) {
        auto serialized = c.program->serialize();
        auto deserialized = clvm->deserialize(serialized);
        require_eq_str(to_hex(serialized), c.hex, "serialize");
        require(bytes_equal(c.program->tree_hash(), deserialized->tree_hash()), "tree_hash");
    }
}

void curry_roundtrip() {
    auto clvm = Clvm::init();
    std::vector<std::shared_ptr<Program>> items;
    for (int i = 0; i < 10; ++i) {
        items.push_back(clvm->int_(std::to_string(i)));
    }
    auto curried = clvm->nil()->curry(items);
    auto uncurried = curried->uncurry();
    require(uncurried != nullptr, "uncurry should be present");
    require(bytes_equal(clvm->nil()->tree_hash(), uncurried->get_program()->tree_hash()),
            "uncurried program");
    auto args = uncurried->get_args();
    require(args.size() == 10, "uncurried args count");
    for (int i = 0; i < 10; ++i) {
        require_eq_str(args[i]->to_int().value(), std::to_string(i), "uncurried arg");
    }
}

void alloc_multiple_types() {
    auto clvm = Clvm::init();
    auto run_cat_tail = RunCatTail::init(clvm->nil(), clvm->nil());
    auto program = clvm->list(std::vector<std::shared_ptr<Program>>{
        clvm->nil(),
        clvm->alloc(ClvmType(ClvmType::kPublicKey{PublicKey::infinity()})),
        clvm->atom([] {
            const std::string s = "Hello, world!";
            return std::vector<uint8_t>(s.begin(), s.end());
        }()),
        clvm->int_("42"),
        clvm->int_("100"),
        clvm->bool_(true),
        clvm->atom({1, 2, 3}),
        clvm->atom(std::vector<uint8_t>(32, 0)),
        clvm->nil(),
        clvm->nil(),
        clvm->alloc(ClvmType(ClvmType::kRunCatTail{run_cat_tail})),
    });
    require_eq_str(
        to_hex(program->serialize()),
        "ff80ffb0c0000000000000000000000000000000000000000000000000000000000000000000000000"
        "0000000000000000000000ff8d48656c6c6f2c20776f726c6421ff2aff64ff01ff83010203ffa00000"
        "000000000000000000000000000000000000000000000000000000000000ff80ff80ffff33ff80ff81"
        "8fff80ff808080",
        "alloc serialization");
}

void create_and_parse_condition() {
    auto clvm = Clvm::init();
    std::vector<uint8_t> puzzle_hash(32, 0xff);
    auto memos = clvm->list(std::vector<std::shared_ptr<Program>>{clvm->atom(puzzle_hash)});
    auto condition = clvm->create_coin(puzzle_hash, "1", memos);
    auto parsed = condition->parse_create_coin();
    require(parsed != nullptr, "parse_create_coin should be present");
    require(bytes_equal(puzzle_hash, parsed->get_puzzle_hash()), "parsed puzzle hash");
    require_eq_str(parsed->get_amount(), "1", "parsed amount");
}

const std::map<std::string, std::function<void()>> kTests{
    {"to_hex_from_hex_roundtrip", to_hex_from_hex_roundtrip},
    {"bytes_equal_equal", bytes_equal_equal},
    {"bytes_equal_not_equal", bytes_equal_not_equal},
    {"coin_id_known_value", coin_id_known_value},
    {"atom_roundtrip", atom_roundtrip},
    {"string_roundtrip", string_roundtrip},
    {"int_roundtrip", int_roundtrip},
    {"pair_roundtrip", pair_roundtrip},
    {"public_key_roundtrip", public_key_roundtrip},
    {"clvm_serialization", clvm_serialization},
    {"curry_roundtrip", curry_roundtrip},
    {"alloc_multiple_types", alloc_multiple_types},
    {"create_and_parse_condition", create_and_parse_condition},
};

int run_one(const std::string &name) {
    auto it = kTests.find(name);
    if (it == kTests.end()) {
        std::cerr << "unknown test: " << name << "\n";
        return 2;
    }
    try {
        it->second();
    } catch (const std::exception &e) {
        std::cerr << name << " FAILED: " << e.what() << "\n";
        return 1;
    }
    std::cout << name << " passed\n";
    return 0;
}

} // namespace

int main(int argc, char **argv) {
    if (argc > 1) {
        return run_one(argv[1]);
    }
    int failures = 0;
    for (const auto &[name, _] : kTests) {
        failures += run_one(name) == 0 ? 0 : 1;
    }
    return failures == 0 ? 0 : 1;
}
