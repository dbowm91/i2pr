cmake_minimum_required(VERSION 3.20)
project(i2pd_ntcp2_interop_driver LANGUAGES CXX)

# Plan 064 i2pd direct NTCP2 driver build contract.
#
# The driver is a test-only, source-locked helper that links against the
# pinned i2pd 2.60.0 libraries and exercises the source-verified
# ``i2p::transport::Transports::SendMessage`` and
# ``i2p::data::CreateDeliveryStatusMsg`` call graph. It is
# source-locked to:
#
#     i2pd 2.60.0 @ f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
#
# recorded in source-lock.json. The CMake configuration never modifies
# the pinned i2pd source tree and never alters i2pd transport
# behaviour.
#
# The build produces two binaries from the same pinned tree:
#
#   i2pd_ntcp2_interop_driver_instrumented
#     compiled with -DI2PD_INTEROP_OBSERVER
#
#   i2pd_ntcp2_interop_driver_control
#     compiled without I2PD_INTEROP_OBSERVER
#
# The two binaries must produce identical protocol outcomes; only the
# observer output and the observer source digest differ.

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(I2PR_CXX_FLAGS
    "-O2 -g -fvisibility=hidden -fstack-protector-strong -Wall -Wextra -Werror=return-type -Werror=switch"
)
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    list(APPEND I2PR_CXX_FLAGS "-O0")
endif()

set(I2PR_INTEROP_OBSERVER_INCLUDE
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
)

# The pinned i2pd libraries and headers must be supplied through the
# I2PD_SOURCE_DIR, I2PD_LINK_DIRS, and I2PD_LIBRARIES variables. The
# pinned build metadata (target/interop/cache/build-metadata.txt) records
# the exact compiler, flags, and link inputs. The driver builds from
# the source tree at ${I2PD_SOURCE_DIR} which must equal the pinned
# revision.

if(NOT DEFINED I2PD_SOURCE_DIR)
    message(FATAL_ERROR "I2PD_SOURCE_DIR must point at the pinned i2pd source tree")
endif()

# i2pd links several Boost components and an OpenSSL/crypto provider.
# The exact list of linked library digests is recorded in
# source-lock.json so the harness can refuse a drift between the declared
# helper build and the locked runtime inputs.

find_package(Boost 1.83 REQUIRED COMPONENTS system thread filesystem)
find_package(OpenSSL REQUIRED)
find_package(Threads REQUIRED)

set(I2PR_LINK_LIBRARIES
    ${Boost_LIBRARIES}
    OpenSSL::SSL
    OpenSSL::Crypto
    Threads::Threads
)

# Driver sources: a single translation unit that embeds the observer
# implementation and the strict config parser.

set(I2PR_DRIVER_SOURCES
    src/i2pd_ntcp2_interop_driver.cpp
    src/interop_observer.cpp
)

# Instrumented binary (default helper). The observer macro is defined
# at compile time, and the observer patch applied to the pinned i2pd
# tree activates the post-AEAD and post-FromNTCP2 receive seam and the
# successful frame-write send seam.

add_executable(i2pd_ntcp2_interop_driver_instrumented
    ${I2PR_DRIVER_SOURCES}
)
target_compile_options(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE ${I2PR_CXX_FLAGS}
)
target_compile_definitions(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE I2PD_INTEROP_OBSERVER=1
)
target_include_directories(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE
        "${I2PD_SOURCE_DIR}/libi2pd"
        "${I2PD_SOURCE_DIR}/libi2pd_client"
        "${I2PR_INTEROP_OBSERVER_INCLUDE}"
)
target_link_libraries(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE ${I2PR_LINK_LIBRARIES}
)

# Uninstrumented control binary. Same source, same pinned libraries,
# without the observer macro. The patch must be reverted before this
# build so that no observer call site is reachable.

add_executable(i2pd_ntcp2_interop_driver_control
    ${I2PR_DRIVER_SOURCES}
)
target_compile_options(i2pd_ntcp2_interop_driver_control
    PRIVATE ${I2PR_CXX_FLAGS}
)
target_include_directories(i2pd_ntcp2_interop_driver_control
    PRIVATE
        "${I2PD_SOURCE_DIR}/libi2pd"
        "${I2PD_SOURCE_DIR}/libi2pd_client"
        "${I2PR_INTEROP_OBSERVER_INCLUDE}"
)
target_link_libraries(i2pd_ntcp2_interop_driver_control
    PRIVATE ${I2PR_LINK_LIBRARIES}
)

# The binaries have no public install surface; they live next to the
# source for the Plan 064 helper controls. The build script stages
# both binaries into the owned build directory.

set_target_properties(
    i2pd_ntcp2_interop_driver_instrumented
    i2pd_ntcp2_interop_driver_control
    PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)