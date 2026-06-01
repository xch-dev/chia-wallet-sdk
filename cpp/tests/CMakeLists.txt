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

add_executable(alloc_test
  alloc_test.cpp
  "${BINDINGS_DIR}/chia_wallet_sdk.cpp")

target_include_directories(alloc_test PRIVATE "${BINDINGS_DIR}")

# Link the staged native shared library (libchia_wallet_sdk.{dylib,so} / chia_wallet_sdk.dll).
find_library(CHIA_WALLET_SDK_LIB
  NAMES chia_wallet_sdk
  PATHS "${BINDINGS_DIR}"
  NO_DEFAULT_PATH)

if(NOT CHIA_WALLET_SDK_LIB)
  message(FATAL_ERROR "Native library not found in ${BINDINGS_DIR}. Run ../local-build.sh first.")
endif()

target_link_libraries(alloc_test PRIVATE "${CHIA_WALLET_SDK_LIB}")

enable_testing()
add_test(NAME alloc_test COMMAND alloc_test)

# Ensure the loader can find the shared library at test time.
if(APPLE)
  set_tests_properties(alloc_test PROPERTIES
    ENVIRONMENT "DYLD_LIBRARY_PATH=${BINDINGS_DIR}")
else()
  set_tests_properties(alloc_test PROPERTIES
    ENVIRONMENT "LD_LIBRARY_PATH=${BINDINGS_DIR}")
endif()
