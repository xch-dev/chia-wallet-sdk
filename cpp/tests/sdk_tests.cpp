#include <chiawalletsdk.hpp>

#include <cstdint>
#include <functional>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

namespace sdk = chiawalletsdk;

namespace {

std::vector<std::pair<std::string, std::function<void()>>> &tests() {
    static std::vector<std::pair<std::string, std::function<void()>>> tests;
    return tests;
}

struct Register {
    Register(const std::string &name, std::function<void()> test) { tests().emplace_back(name, std::move(test)); }
};

#define TEST(name)                               \
    void name();                                 \
    const Register register_##name(#name, name); \
    void name()

#define CHECK(condition)                                                                                  \
    if (!(condition)) {                                                                                   \
        throw std::runtime_error(std::string(__FILE__) + ":" + std::to_string(__LINE__) + ": " #condition); \
    }

std::vector<uint8_t> from_hex(const std::string &hex) {
    std::vector<uint8_t> bytes;
    for (size_t i = 0; i < hex.size(); i += 2) {
        bytes.push_back(static_cast<uint8_t>(std::stoul(hex.substr(i, 2), nullptr, 16)));
    }
    return bytes;
}

TEST(coin_id) {
    auto coin = sdk::Coin::init(from_hex("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
                                from_hex("dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986"), 100);

    CHECK(coin->coin_id() == from_hex("fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a"));
}

TEST(setters_mutate_in_place) {
    auto coin = sdk::Coin::init(std::vector<uint8_t>(32), std::vector<uint8_t>(32), 100);
    auto clone = coin->clone();

    coin->set_amount(UINT64_MAX);

    CHECK(coin->amount() == UINT64_MAX);
    CHECK(clone->amount() == 100);
}

TEST(wrong_length_throws) {
    try {
        sdk::Coin::init(std::vector<uint8_t>(31), std::vector<uint8_t>(32), 100);
    } catch (const sdk::ChiaError &error) {
        CHECK(std::string(error.what()).find("expected 32 bytes, found 31") != std::string::npos);
        return;
    }

    throw std::runtime_error("expected an exception");
}

TEST(bigint_roundtrip) {
    auto clvm = sdk::Clvm::init();

    for (const auto &expected : {"0", "-1", "-421489719874198729487129847",
                                 "4384723984791283749823764732649187498237483927482"}) {
        CHECK(clvm->int_(expected)->to_int() == expected);
    }
}

TEST(clvm_value_allocation) {
    auto clvm = sdk::Clvm::init();
    auto pk = sdk::PublicKey::infinity();

    CHECK(clvm->alloc(sdk::ClvmType::kInt{"42"})->tree_hash() == clvm->int_("42")->tree_hash());
    CHECK(clvm->alloc(sdk::ClvmType::kText{"hello"})->tree_hash() == clvm->string("hello")->tree_hash());
    CHECK(clvm->alloc(sdk::ClvmType::kPublicKey{pk})->tree_hash() == clvm->atom(pk->to_bytes())->tree_hash());
}

TEST(curry_add_function) {
    auto clvm = sdk::Clvm::init();

    auto add_mod = clvm->deserialize(from_hex("ff10ff02ff0580"));
    auto add_to_ten = add_mod->curry({clvm->int_("10")});
    auto output = add_to_ten->run(clvm->list({clvm->int_("5")}), 10'000'000, true);

    CHECK(output->value()->to_int() == "15");
    CHECK(output->cost() == 1082);
}

TEST(static_methods) {
    auto clvm = sdk::Clvm::init();
    auto program = clvm->deserialize(sdk::constants_p2_delegated_puzzle_or_hidden_puzzle());

    CHECK(program->tree_hash() == sdk::constants_p2_delegated_puzzle_or_hidden_puzzle_hash());
}

TEST(simulator_transaction) {
    auto sim = sdk::Simulator::init();
    auto clvm = sdk::Clvm::init();
    auto alice = sim->bls(1000);

    auto spend = clvm->delegated_spend({clvm->create_coin(alice->puzzle_hash(), 900, nullptr), clvm->reserve_fee(100)});
    clvm->spend_standard_coin(alice->coin(), alice->pk(), spend);
    sim->spend_coins(clvm->coin_spends(), {alice->sk()});

    auto coins = sim->unspent_coins(alice->puzzle_hash(), false);
    CHECK(coins.size() == 1);
    CHECK(coins[0]->amount() == 900);
}

TEST(blocking_rpc_against_full_node_simulator) {
    auto sim = sdk::FullNodeSimulator::init();
    auto server = sim->start_server();
    auto rpc = sdk::RpcClient::init(server->url());

    auto info = rpc->get_network_info();
    server->close();

    CHECK(info->success());
    CHECK(info->network_name() == "simulator0");
}

}  // namespace

int main() {
    int failures = 0;

    for (const auto &[name, test] : tests()) {
        try {
            test();
            std::cout << "PASS " << name << std::endl;
        } catch (const std::exception &error) {
            std::cout << "FAIL " << name << ": " << error.what() << std::endl;
            failures++;
        }
    }

    std::cout << tests().size() - failures << " passed, " << failures << " failed" << std::endl;
    return failures == 0 ? 0 : 1;
}
