// Async integration smoke test for the C++ binding's Tokio async runtime bridge.
// Calls RpcClient::mainnet()->get_blockchain_state() to verify that the block_on
// wrapper works correctly for Rust async functions exposed as synchronous C++ calls.
//
// Exit codes:
//   0 — test passed, or skipped due to network unavailability
//   1 — assertion failure or unexpected exception

#include <algorithm>
#include <iostream>
#include <stdexcept>
#include <string>

#include "chia_wallet_sdk.hpp"

using namespace chia_wallet_sdk;

namespace {

// Heuristic: treat failures that look like real network unavailability as a skip,
// not a test failure. Kept narrow on purpose — overly broad substrings (like "request")
// would mask genuine programming bugs that incidentally surface a similar word.
bool is_network_error(const std::string& msg) {
    auto lower = msg;
    std::transform(lower.begin(), lower.end(), lower.begin(), ::tolower);
    return lower.find("timeout:") != std::string::npos ||
           lower.find("connect") != std::string::npos ||
           lower.find("network") != std::string::npos ||
           lower.find("dns") != std::string::npos ||
           lower.find("host") != std::string::npos;
}

} // namespace

int main() {
    try {
        // Bound the request so a hung endpoint surfaces as a timeout (treated as a
        // network skip below) instead of blocking forever.
        auto options = RpcClientOptions::init(/*request_timeout_ms=*/30000, /*connect_timeout_ms=*/10000);
        auto rpc = RpcClient::mainnet()->with_options(options);
        auto response = rpc->get_blockchain_state();

        if (!response->get_success()) {
            std::cerr << "FAILED: get_blockchain_state returned success=false\n";
            return 1;
        }

        // get_blockchain_state() returns std::shared_ptr<BlockchainState> (not optional)
        auto state = response->get_blockchain_state();
        if (state == nullptr) {
            std::cerr << "FAILED: blockchain_state is null\n";
            return 1;
        }

        std::cout << "async_get_blockchain_state passed\n";

        // Second Coinset call over the same timeout-bounded client.
        auto network_info = rpc->get_network_info();
        if (!network_info->get_success()) {
            std::cerr << "FAILED: get_network_info returned success=false\n";
            return 1;
        }
        if (!network_info->get_network_name().has_value()) {
            std::cerr << "FAILED: network_name is empty\n";
            return 1;
        }

        std::cout << "async_get_network_info passed\n";
        return 0;
    } catch (const std::exception& e) {
        if (is_network_error(e.what())) {
            std::cout << "SKIP: network unavailable: " << e.what() << "\n";
            return 0;
        }
        std::cerr << "FAILED: " << e.what() << "\n";
        return 1;
    }
}
