cmake_minimum_required(VERSION 3.20)
project(i2pd_direct_connect LANGUAGES CXX)

# Plan 059 Workstream B: i2pd direct connect helper.
#
# The helper is a separately-built test executable that links against the
# pinned i2pd libraries and exercises the
# `i2pd::transport::Transports::SendMessage` seam against a single imported
# RouterInfo. It is source-locked to:
#
#     i2pd 2.60.0 @ f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
#
# recorded in source-lock.json. The build never modifies the pinned
# libraries and never alters i2pd transport behaviour. The helper uses
# the existing `Transports::SendMessage` -> `ConnectToPeer` -> NTCP2
# server call graph documented in
# tests/integration/ntcp2/reference-trigger-contracts.md.

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(I2PR_CXX_FLAGS "-O2 -g -fvisibility=hidden -fstack-protector-strong -Wall -Wextra -Werror=return-type")
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    list(APPEND I2PR_CXX_FLAGS "-O0")
endif()

set(I2PR_DIRECT_CONNECT_SOURCES
    i2pd_direct_connect.cpp
)

add_executable(i2pd_direct_connect ${I2PR_DIRECT_CONNECT_SOURCES})
target_compile_options(i2pd_direct_connect PRIVATE ${I2PR_CXX_FLAGS})

# The pinned i2pd libraries and headers must be supplied through the
# I2PD_INCLUDE_DIR and I2PD_LINK_DIRS / I2PD_LIBRARIES variables. The
# pinned build metadata (`target/interop/cache/build-metadata.txt`)
# records the exact compiler, flags, and link inputs. The driver
# builds from the source tree at
# `${I2PD_SOURCE_DIR}` which must equal the pinned revision.

if(NOT DEFINED I2PD_SOURCE_DIR)
    message(FATAL_ERROR "I2PD_SOURCE_DIR must point at the pinned i2pd source tree")
endif()

target_include_directories(i2pd_direct_connect PRIVATE
    "${I2PD_SOURCE_DIR}/libi2pd"
    "${I2PD_SOURCE_DIR}/libi2pd_client"
)

# i2pd links several Boost components and an OpenSSL/crypto provider.
# The exact list of linked library digests is recorded in
# `source-lock.json` so the harness can refuse a drift between the
# declared helper build and the locked runtime inputs.

find_package(Boost 1.83 REQUIRED COMPONENTS system thread filesystem)
find_package(OpenSSL REQUIRED)
find_package(Threads REQUIRED)

target_link_libraries(i2pd_direct_connect PRIVATE
    ${Boost_LIBRARIES}
    OpenSSL::SSL
    OpenSSL::Crypto
    Threads::Threads
)

# The helper has no public install surface; the binary lives next to
# the source for the Plan 059 helper controls.
set_target_properties(i2pd_direct_connect PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)
