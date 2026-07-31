cmake_minimum_required(VERSION 3.20)
project(i2pd_ntcp2_interop_driver LANGUAGES CXX)

# Plan 076 i2pd direct NTCP2 driver build contract.
#
# The driver is a test-only, source-locked helper that links against
# the pinned i2pd 2.60.0 libraries. The build script below performs
# two additional steps before the driver build:
#
#   1. Configure and build the unmodified pinned i2pd source tree
#      against a private build directory using its own CMake project
#      with ``WITH_LIBRARY=ON`` and ``WITH_BINARY=OFF``. The pinned
#      tree is *never* mutated; the build is reproducible because the
#      pinned tree digest is recorded in the build manifest.
#   2. Link the test driver against the freshly built ``libi2pd``,
#      ``libi2pdclient``, and ``libi2pdlang`` static archives. The
#      Build script consumes the produced archives via the
#      ``I2PD_LIB_DIR`` cache variable.
#
# Two driver binaries are produced from the same pinned tree:
#
#   i2pd_ntcp2_interop_driver_instrumented
#     compiled with -DI2PD_INTEROP_OBSERVER=1 and -DI2PD_PLAN076_LINKED=1
#
#   i2pd_ntcp2_interop_driver_control
#     compiled without -DI2PD_INTEROP_OBSERVER and without the linked
#     marker (the build script applies the observer patch to a
#     private copy of the pinned tree for the instrumented binary
#     only, then restores the pristine tree for the control build).
#
# The driver is source-locked to:
#
#     i2pd 2.60.0 @ f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
#
# recorded in source-lock.json. The Plan 076 closure boundary does
# not require a mixed-router pass; the closure is a real binary with
# verifiable source linkage and locally testable inspect/control
# behaviour.

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(I2PR_CXX_FLAGS
    "-O2;-g;-fvisibility=hidden;-fstack-protector-strong;-Wall;-Wextra;-Werror=return-type;-Werror=switch"
)
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    list(APPEND I2PR_CXX_FLAGS "-O0")
endif()

set(I2PR_INTEROP_OBSERVER_INCLUDE
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
)

# Build-driver.sh must supply these variables. The driver build is
# never invoked directly by ``cmake``; ``build-driver.sh`` is the
# single entry point.

if(NOT DEFINED I2PD_PATCHED_TREE)
    message(FATAL_ERROR "I2PD_PATCHED_TREE must point at the patched pinned i2pd source tree (instrumented build)")
endif()
if(NOT DEFINED I2PD_PRISTINE_TREE)
    message(FATAL_ERROR "I2PD_PRISTINE_TREE must point at the pristine pinned i2pd source tree (control build)")
endif()
if(NOT DEFINED I2PD_LIB_DIR)
    message(FATAL_ERROR "I2PD_LIB_DIR must point at the directory containing the prebuilt libi2pd archives")
endif()

# The driver reuses the pinned i2pd CMake build for Boost + OpenSSL +
# Threads + ZLIB discovery. The prebuilt archives for ``libi2pd``,
# ``libi2pdclient``, and ``libi2pdlang`` are linked by absolute path
# via ``I2PD_LINK_LIBRARIES`` so no system installation of i2pd is
# ever consulted.
find_package(Boost 1.83 REQUIRED COMPONENTS system thread filesystem program_options)
find_package(OpenSSL REQUIRED)
find_package(Threads REQUIRED)
find_package(ZLIB REQUIRED)

file(GLOB I2PD_ARCHIVES "${I2PD_LIB_DIR}/libi2pd*.a")
if(NOT I2PD_ARCHIVES)
    message(FATAL_ERROR "no i2pd archives found under ${I2PD_LIB_DIR}; run build-driver.sh first")
endif()

set(I2PR_LINK_LIBRARIES
    ${I2PD_ARCHIVES}
    ${Boost_LIBRARIES}
    OpenSSL::SSL
    OpenSSL::Crypto
    ZLIB::ZLIB
    Threads::Threads
)

# Driver sources: the strict config parser, the structured event
# writer, the i2pd initialisation sequence, the inspection/dial
# modes, and the passive observer.

set(I2PR_DRIVER_SOURCES
    src/i2pd_ntcp2_interop_driver.cpp
    src/interop_observer.cpp
)

# Instrumented binary (default helper). The observer macro is defined
# at compile time, and the observer patch applied to the pinned i2pd
# tree activates the post-AEAD and post-FromNTCP2 receive seam and
# the successful frame-write send seam. ``I2PD_PLAN076_LINKED`` is
# defined only when the binary is built against the prebuilt pinned
# i2pd libraries (which is always on the supported build path).

add_executable(i2pd_ntcp2_interop_driver_instrumented
    ${I2PR_DRIVER_SOURCES}
)
target_compile_options(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE ${I2PR_CXX_FLAGS}
)
target_compile_definitions(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE
        I2PD_INTEROP_OBSERVER=1
        I2PD_PLAN076_LINKED=1
)
target_include_directories(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE
        "${I2PD_PATCHED_TREE}/libi2pd"
        "${I2PD_PATCHED_TREE}/libi2pd_client"
        "${I2PR_INTEROP_OBSERVER_INCLUDE}"
)
target_link_libraries(i2pd_ntcp2_interop_driver_instrumented
    PRIVATE ${I2PR_LINK_LIBRARIES}
)

# Uninstrumented control binary. Same pinned tree, no observer macro,
# no linked marker, no observer patch applied. The control binary
# must contain no reachable observer call sites; the build script
# enforces this with a parallel build that uses the pristine tree.

add_executable(i2pd_ntcp2_interop_driver_control
    ${I2PR_DRIVER_SOURCES}
)
target_compile_options(i2pd_ntcp2_interop_driver_control
    PRIVATE ${I2PR_CXX_FLAGS}
)
target_compile_definitions(i2pd_ntcp2_interop_driver_control
    PRIVATE
        I2PD_PLAN076_LINKED=1
)
target_include_directories(i2pd_ntcp2_interop_driver_control
    PRIVATE
        "${I2PD_PRISTINE_TREE}/libi2pd"
        "${I2PD_PRISTINE_TREE}/libi2pd_client"
        "${I2PR_INTEROP_OBSERVER_INCLUDE}"
)
target_link_libraries(i2pd_ntcp2_interop_driver_control
    PRIVATE ${I2PR_LINK_LIBRARIES}
)

# The binaries have no public install surface; they live next to the
# source for the Plan 076 helper controls. The build script stages
# both binaries into the owned build directory.

set_target_properties(
    i2pd_ntcp2_interop_driver_instrumented
    i2pd_ntcp2_interop_driver_control
    PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)
