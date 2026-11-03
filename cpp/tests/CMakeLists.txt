cmake_minimum_required(VERSION 3.20)
project(chia_wallet_sdk_cpp_tests CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Directory holding the generated bindings + staged native library, produced by
# `../local-build.sh`. Override with -DBINDINGS_DIR=... if generated elsewhere.
set(BINDINGS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../chia_wallet_sdk"
    CACHE PATH "Directory with generated chia_wallet_sdk.{hpp,cpp} and the native library")

if(NOT EXISTS "${BINDINGS_DIR}/chia_wallet_sdk.cpp")
  message(FATAL_ERROR
    "Generated bindings not found in ${BINDINGS_DIR}.\n"
    "Run ../local-build.sh first (from the cpp/ directory).")
endif()

add_executable(sdk_tests
  sdk_tests.cpp
  "${BINDINGS_DIR}/chia_wallet_sdk.cpp")

target_include_directories(sdk_tests PRIVATE "${BINDINGS_DIR}")

# Link the staged native shared library (libchia_wallet_sdk.{dylib,so} / chia_wallet_sdk.dll).
find_library(CHIA_WALLET_SDK_LIB
  NAMES chia_wallet_sdk
  PATHS "${BINDINGS_DIR}"
  NO_DEFAULT_PATH)

if(NOT CHIA_WALLET_SDK_LIB)
  message(FATAL_ERROR "Native library not found in ${BINDINGS_DIR}. Run ../local-build.sh first.")
endif()

target_link_libraries(sdk_tests PRIVATE "${CHIA_WALLET_SDK_LIB}")

enable_testing()

# One ctest entry per logical test, kept in sync with kTests in sdk_tests.cpp.
set(TEST_NAMES
  to_hex_from_hex_roundtrip
  bytes_equal_equal
  bytes_equal_not_equal
  coin_id_known_value
  atom_roundtrip
  string_roundtrip
  int_roundtrip
  pair_roundtrip
  public_key_roundtrip
  clvm_serialization
  curry_roundtrip
  alloc_multiple_types
  create_and_parse_condition)

if(APPLE)
  set(LOADER_ENV "DYLD_LIBRARY_PATH=${BINDINGS_DIR}")
else()
  set(LOADER_ENV "LD_LIBRARY_PATH=${BINDINGS_DIR}")
endif()

foreach(test_name IN LISTS TEST_NAMES)
  add_test(NAME ${test_name} COMMAND sdk_tests ${test_name})
  set_tests_properties(${test_name} PROPERTIES ENVIRONMENT "${LOADER_ENV}")
endforeach()

# Async integration tests (network-dependent, run manually via build-and-test-async.sh)
add_executable(async_sdk_tests
  async_tests.cpp
  "${BINDINGS_DIR}/chia_wallet_sdk.cpp")

target_include_directories(async_sdk_tests PRIVATE "${BINDINGS_DIR}")
target_link_libraries(async_sdk_tests PRIVATE "${CHIA_WALLET_SDK_LIB}")

add_test(NAME async_get_blockchain_state COMMAND async_sdk_tests)
set_tests_properties(async_get_blockchain_state PROPERTIES
  ENVIRONMENT "${LOADER_ENV}"
  LABELS "integration")
