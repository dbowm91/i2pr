cmake_minimum_required(VERSION 3.20)
project(i2pd_ntcp2_interop_driver LANGUAGES CXX)

# Plan 099 i2pd direct NTCP2 driver build contract.
#
# The driver is a test-only, source-locked helper that links against
# the pinned i2pd 2.60.0 libraries. The build script below performs
# three ordered library build stages before the driver build:
#
#   1. Build the unmodified pinned i2pd source tree against a
#      private build directory using its own CMake project with
#      ``WITH_LIBRARY=ON`` and ``WITH_BINARY=OFF``. The pristine
#      archives feed the control driver binary; the pristine tree
#      is *never* mutated.
#   2. Apply the observer patch to a private copy of the pinned
#      tree and build the i2pd CMake project **against the
#      patched tree** with ``-DI2PD_INTEROP_OBSERVER=1`` visible to
#      the patched ``NTCP2.cpp`` compile. The instrumented archives
#      carry the real observer call sites; they feed the
#      instrumented driver binary.
#   3. Link the test driver against the freshly built archives
#      through the explicit ``I2PD_INSTRUMENTED_LIB_DIR`` and
#      ``I2PD_PRISTINE_LIB_DIR`` cache variables. Header
#      include-path differences alone do not satisfy Plan 099
#      WP1.3; mixing the two archive sets at link time would link
#      a stale ``NTCP2.cpp`` object into the instrumented binary.
#
# Two driver binaries are produced from the same pinned tree:
#
#   i2pd_ntcp2_interop_driver_instrumented
#     compiled with -DI2PD_INTEROP_OBSERVER=1 and -DI2PD_PLAN076_LINKED=1
#     linked against the instrumented archives
#
#   i2pd_ntcp2_interop_driver_control
#     compiled without -DI2PD_INTEROP_OBSERVER and with
#     -DI2PD_PLAN076_LINKED=1; linked against the pristine archives;
#     no reachable observer call sites
#
# The driver is source-locked to:
#
#     i2pd 2.60.0 @ f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
#
# recorded in source-lock.json. The Plan 099 closure boundary does
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
if(NOT DEFINED I2PD_INSTRUMENTED_LIB_DIR)
    message(FATAL_ERROR "I2PD_INSTRUMENTED_LIB_DIR must point at the directory containing the prebuilt instrumented libi2pd archives")
endif()
if(NOT DEFINED I2PD_PRISTINE_LIB_DIR)
    message(FATAL_ERROR "I2PD_PRISTINE_LIB_DIR must point at the directory containing the prebuilt pristine libi2pd archives")
endif()

# The driver reuses the pinned i2pd CMake build for Boost + OpenSSL +
# Threads + ZLIB discovery. The prebuilt archives for ``libi2pd``,
# ``libi2pdclient``, and ``libi2pdlang`` are linked by absolute path
# so no system installation of i2pd is ever consulted. Plan 099
# WP1.3 enforces separate directory variables for the two archive
# sets; a stale ``I2PD_LIB_DIR`` variable is rejected.
find_package(Boost 1.83 REQUIRED COMPONENTS system thread filesystem program_options)
find_package(OpenSSL REQUIRED)
find_package(Threads REQUIRED)
find_package(ZLIB REQUIRED)

file(GLOB I2PD_INSTRUMENTED_ARCHIVES "${I2PD_INSTRUMENTED_LIB_DIR}/libi2pd*.a")
if(NOT I2PD_INSTRUMENTED_ARCHIVES)
    message(FATAL_ERROR "no instrumented i2pd archives found under ${I2PD_INSTRUMENTED_LIB_DIR}; run build-driver.sh first")
endif()
file(GLOB I2PD_PRISTINE_ARCHIVES "${I2PD_PRISTINE_LIB_DIR}/libi2pd*.a")
if(NOT I2PD_PRISTINE_ARCHIVES)
    message(FATAL_ERROR "no pristine i2pd archives found under ${I2PD_PRISTINE_LIB_DIR}; run build-driver.sh first")
endif()

set(I2PR_INSTRUMENTED_LINK_LIBRARIES
    ${I2PD_INSTRUMENTED_ARCHIVES}
    ${Boost_LIBRARIES}
    OpenSSL::SSL
    OpenSSL::Crypto
    ZLIB::ZLIB
    Threads::Threads
)
set(I2PR_PRISTINE_LINK_LIBRARIES
    ${I2PD_PRISTINE_ARCHIVES}
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

# Instrumented binary. The observer macro is defined at compile
# time, and the patched NTCP2.cpp translation unit (compiled into
# the instrumented archives with -DI2PD_INTEROP_OBSERVER=1) emits
# the post-AEAD and post-FromNTCP2 receive seam and the successful
# frame-write send seam. ``I2PD_PLAN076_LINKED`` is defined only
# when the binary is built against the prebuilt pinned i2pd
# libraries (which is always on the supported build path).

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
    PRIVATE ${I2PR_INSTRUMENTED_LINK_LIBRARIES}
)

# Uninstrumented control binary. No observer macro, no patched
# tree in the include path; the linked archives are the pristine
# set, so no reachable observer call sites exist. Plan 099 WP2
# forbids waiting on observer APIs in the control branch.

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
    PRIVATE ${I2PR_PRISTINE_LINK_LIBRARIES}
)

# The binaries have no public install surface; they live next to the
# source for the Plan 099 helper controls. The build script stages
# both binaries into the owned build directory.

set_target_properties(
    i2pd_ntcp2_interop_driver_instrumented
    i2pd_ntcp2_interop_driver_control
    PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)
