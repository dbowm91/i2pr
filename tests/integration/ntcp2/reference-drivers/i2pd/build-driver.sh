#!/usr/bin/env bash
# Plan 076 i2pd direct NTCP2 driver build script.
#
# This script performs two ordered stages:
#
#   1. Configure and build the unmodified pinned i2pd 2.60.0 source
#      tree as static libraries (``libi2pd``, ``libi2pdclient``,
#      ``libi2pdlang``) via the pinned i2pd CMake project. The pinned
#      tree digest is measured before the build and recorded in the
#      build manifest; the pinned tree is *never* mutated.
#
#   2. Apply the Plan 076 observer patch to a private copy of the
#      pinned tree, configure the Plan 076 driver CMake project
#      against the freshly built i2pd libraries, and produce both the
#      instrumented and the uninstrumented control driver binaries.
#
# The script writes only into the ``--output-dir`` directory. The
# pinned cache directory remains untouched.
#
# Usage:
#
#     bash tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh \
#         --repo-root <repo-root> \
#         --i2pd-source-dir <pinned-i2pd-source-dir> \
#         --output-dir <owned-output-dir>
#
# Required flags:
#
#     --repo-root       Repository root containing the pinned reference
#                       driver artifacts.
#     --i2pd-source-dir Directory containing the pristine pinned i2pd
#                       2.60.0 source tree (must match the locked
#                       revision).
#     --output-dir      Owned output directory; the script writes the
#                       pinned i2pd archive SHA, the linked library
#                       manifest, both driver binaries, and the two
#                       build manifests.

set -euo pipefail

REPO_ROOT=""
I2PD_SOURCE_DIR=""
OUTPUT_DIR=""
DRIVER_DIR_NAME="reference-drivers/i2pd"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repo-root)
            REPO_ROOT="$2"
            shift 2
            ;;
        --i2pd-source-dir)
            I2PD_SOURCE_DIR="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --help|-h)
            sed -n '2,38p' "$0"
            exit 0
            ;;
        *)
            echo "build-driver.sh: unknown argument: $1" >&2
            exit 64
            ;;
    esac
done

if [[ -z "$REPO_ROOT" || -z "$I2PD_SOURCE_DIR" || -z "$OUTPUT_DIR" ]]; then
    echo "build-driver.sh: --repo-root, --i2pd-source-dir, --output-dir are required" >&2
    exit 64
fi

REPO_ROOT="$(cd "$REPO_ROOT" && pwd)"
I2PD_SOURCE_DIR="$(cd "$I2PD_SOURCE_DIR" && pwd)"
OUTPUT_DIR="$(mkdir -p "$OUTPUT_DIR" && cd "$OUTPUT_DIR" && pwd)"

HELPER_DIR="$REPO_ROOT/tests/integration/ntcp2/$DRIVER_DIR_NAME"
HELPER_SOURCE="$HELPER_DIR/src/i2pd_ntcp2_interop_driver.cpp"
OBSERVER_HEADER="$HELPER_DIR/src/interop_observer.h"
OBSERVER_SOURCE="$HELPER_DIR/src/interop_observer.cpp"
SOURCE_LOCK="$HELPER_DIR/source-lock.json"
OBSERVER_PATCH="$HELPER_DIR/patches/i2pd-2.60.0-interop-observer.patch"
BUILD_SCHEMA="$HELPER_DIR/build-manifest.schema.json"

if [[ ! -f "$HELPER_SOURCE" ]]; then
    echo "build-driver.sh: helper source missing at $HELPER_SOURCE" >&2
    exit 64
fi
if [[ ! -f "$OBSERVER_HEADER" ]]; then
    echo "build-driver.sh: observer header missing at $OBSERVER_HEADER" >&2
    exit 64
fi
if [[ ! -f "$OBSERVER_SOURCE" ]]; then
    echo "build-driver.sh: observer source missing at $OBSERVER_SOURCE" >&2
    exit 64
fi
if [[ ! -f "$SOURCE_LOCK" ]]; then
    echo "build-driver.sh: source-lock record missing at $SOURCE_LOCK" >&2
    exit 64
fi
if [[ ! -f "$OBSERVER_PATCH" ]]; then
    echo "build-driver.sh: observer patch missing at $OBSERVER_PATCH" >&2
    exit 64
fi
if [[ ! -f "$BUILD_SCHEMA" ]]; then
    echo "build-driver.sh: build manifest schema missing at $BUILD_SCHEMA" >&2
    exit 64
fi
if [[ ! -d "$I2PD_SOURCE_DIR/libi2pd" ]]; then
    echo "build-driver.sh: pinned i2pd source $I2PD_SOURCE_DIR/libi2pd is missing" >&2
    exit 64
fi

# Phase 1: compute provenance digests.
PRISTINE_TREE_SHA="$(find "$I2PD_SOURCE_DIR" -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | awk '{print $1}')"
SOURCE_LOCK_SHA="$(sha256sum "$SOURCE_LOCK" | awk '{print $1}')"
OBSERVER_PATCH_SHA="$(sha256sum "$OBSERVER_PATCH" | awk '{print $1}')"
HELPER_SOURCE_SHA="$(sha256sum "$HELPER_SOURCE" | awk '{print $1}')"
OBSERVER_HEADER_SHA="$(sha256sum "$OBSERVER_HEADER" | awk '{print $1}')"
OBSERVER_SOURCE_SHA="$(sha256sum "$OBSERVER_SOURCE" | awk '{print $1}')"

# Phase 2: configure and build the pinned i2pd libraries. The pinned
# CMake project is invoked via its own build/ directory with the
# library option enabled and the binary option disabled. The pinned
# tree is never mutated.
I2PD_LIB_BUILD="$(mktemp -d -t plan076-i2pd-build-XXXXXX)"
trap 'rm -rf "$I2PD_LIB_BUILD"' EXIT

cmake \
    -S "$I2PD_SOURCE_DIR/build" \
    -B "$I2PD_LIB_BUILD" \
    -DCMAKE_BUILD_TYPE=Release \
    -DWITH_HARDENING=OFF \
    -DWITH_BINARY=OFF \
    -DWITH_LIBRARY=ON \
    -DBUILD_TESTING=OFF \
    -DWITH_UPNP=OFF >/dev/null

cmake --build "$I2PD_LIB_BUILD" --parallel >/dev/null

if [[ ! -f "$I2PD_LIB_BUILD/libi2pd.a" || \
      ! -f "$I2PD_LIB_BUILD/libi2pdclient.a" || \
      ! -f "$I2PD_LIB_BUILD/libi2pdlang.a" ]]; then
    echo "build-driver.sh: pinned i2pd libraries were not produced" >&2
    exit 64
fi

# Phase 3: apply the observer patch to a private copy of the pinned
# tree, then build the Plan 076 driver against the freshly built
# libraries.
PATCHED_SRC="$(mktemp -d -t plan076-i2pd-patched-XXXXXX)"
trap 'rm -rf "$I2PD_LIB_BUILD" "$PATCHED_SRC"' EXIT

cp -R "$I2PD_SOURCE_DIR/." "$PATCHED_SRC/"
cp "$OBSERVER_HEADER" "$PATCHED_SRC/libi2pd/interop_observer.h"
cp "$OBSERVER_SOURCE" "$PATCHED_SRC/libi2pd/interop_observer.cpp"
(cd "$PATCHED_SRC" && patch -p1 --fuzz=0 --dry-run < "$OBSERVER_PATCH" >/dev/null)
(cd "$PATCHED_SRC" && patch -p1 --fuzz=0 < "$OBSERVER_PATCH" >/dev/null)

DRIVER_BUILD="$(mktemp -d -t plan076-driver-build-XXXXXX)"

cmake \
    -S "$HELPER_DIR" \
    -B "$DRIVER_BUILD" \
    -DCMAKE_BUILD_TYPE=Release \
    -DI2PD_PATCHED_TREE="$PATCHED_SRC" \
    -DI2PD_PRISTINE_TREE="$I2PD_SOURCE_DIR" \
    -DI2PD_LIB_DIR="$I2PD_LIB_BUILD" >/dev/null

cmake --build "$DRIVER_BUILD" \
    --target i2pd_ntcp2_interop_driver_instrumented \
    --target i2pd_ntcp2_interop_driver_control \
    --parallel >/dev/null

INSTRUMENTED_BINARY="$DRIVER_BUILD/i2pd_ntcp2_interop_driver_instrumented"
CONTROL_BINARY="$DRIVER_BUILD/i2pd_ntcp2_interop_driver_control"

if [[ ! -x "$INSTRUMENTED_BINARY" ]]; then
    echo "build-driver.sh: instrumented binary was not produced" >&2
    exit 64
fi
if [[ ! -x "$CONTROL_BINARY" ]]; then
    echo "build-driver.sh: uninstrumented control binary was not produced" >&2
    exit 64
fi

cp "$INSTRUMENTED_BINARY" "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented"
cp "$CONTROL_BINARY" "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_control"

# Phase 4: linked library provenance manifest.
LINK_MANIFEST="$OUTPUT_DIR/linked-library-manifest.txt"
{
    echo "# Plan 076 i2pd direct driver linked-library manifest"
    echo "# generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "# compiler: $(g++ --version | head -n1)"
    echo "# cmake: $(cmake --version | head -n1)"
    echo
    echo "# pinned i2pd libraries (built from the pinned i2pd CMake project):"
    sha256sum "$I2PD_LIB_BUILD/libi2pd.a" "$I2PD_LIB_BUILD/libi2pdclient.a" "$I2PD_LIB_BUILD/libi2pdlang.a"
    echo
    echo "# instrumented binary ldd:"
    ldd "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented" | awk '$2 == "=>" {print}'
    echo
    echo "# control binary ldd:"
    ldd "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_control" | awk '$2 == "=>" {print}'
} > "$LINK_MANIFEST"
LINK_MANIFEST_SHA="$(sha256sum "$LINK_MANIFEST" | awk '{print $1}')"

INSTRUMENTED_BINARY_SHA="$(sha256sum "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented" | awk '{print $1}')"
CONTROL_BINARY_SHA="$(sha256sum "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_control" | awk '{print $1}')"
I2PD_LIB_SHA="$(sha256sum "$I2PD_LIB_BUILD/libi2pd.a" "$I2PD_LIB_BUILD/libi2pdclient.a" "$I2PD_LIB_BUILD/libi2pdlang.a" \
    | awk '{print $1}' | paste -sd' ' -)"
TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
CMAKE_VERSION="$(cmake --version | head -n1)"
COMPILER_VERSION="$(g++ --version | head -n1)"

cat > "$OUTPUT_DIR/build-manifest-instrumented.json" <<EOF
{
  "schema": "i2pr-i2pd-direct-driver-build-manifest-v1",
  "schema_version": 1,
  "reference_name": "i2pd",
  "reference_version": "2.60.0",
  "reference_revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
  "reference_source_tree_sha256": "$PRISTINE_TREE_SHA",
  "observer_patch_sha256": "$OBSERVER_PATCH_SHA",
  "driver_source_sha256": "$HELPER_SOURCE_SHA",
  "observer_header_sha256": "$OBSERVER_HEADER_SHA",
  "observer_source_sha256": "$OBSERVER_SOURCE_SHA",
  "source_lock_sha256": "$SOURCE_LOCK_SHA",
  "driver_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "uninstrumented_binary_sha256": "$CONTROL_BINARY_SHA",
  "linked_library_manifest_sha256": "$LINK_MANIFEST_SHA",
  "i2pd_libraries_sha256": "$I2PD_LIB_SHA",
  "cmake_version": "$CMAKE_VERSION",
  "compiler_version": "$COMPILER_VERSION",
  "cmake_build_type": "Release",
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "linked_i2pd_sources": true,
  "observer_compile_time_gated": true
}
EOF

cat > "$OUTPUT_DIR/build-manifest-control.json" <<EOF
{
  "schema": "i2pr-i2pd-direct-driver-build-manifest-v1",
  "schema_version": 1,
  "reference_name": "i2pd",
  "reference_version": "2.60.0",
  "reference_revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
  "reference_source_tree_sha256": "$PRISTINE_TREE_SHA",
  "observer_patch_sha256": "$OBSERVER_PATCH_SHA",
  "driver_source_sha256": "$HELPER_SOURCE_SHA",
  "observer_header_sha256": "$OBSERVER_HEADER_SHA",
  "observer_source_sha256": "$OBSERVER_SOURCE_SHA",
  "source_lock_sha256": "$SOURCE_LOCK_SHA",
  "driver_binary_sha256": "$CONTROL_BINARY_SHA",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "uninstrumented_binary_sha256": "$CONTROL_BINARY_SHA",
  "linked_library_manifest_sha256": "$LINK_MANIFEST_SHA",
  "i2pd_libraries_sha256": "$I2PD_LIB_SHA",
  "cmake_version": "$CMAKE_VERSION",
  "compiler_version": "$COMPILER_VERSION",
  "cmake_build_type": "Release",
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "linked_i2pd_sources": true,
  "observer_compile_time_gated": true
}
EOF

cat > "$OUTPUT_DIR/inspect-instrumented.json" <<EOF
{
  "schema": "i2pr-i2pd-direct-driver-inspect-v1",
  "schema_version": 1,
  "build_manifest_sha256": "$(sha256sum "$OUTPUT_DIR/build-manifest-instrumented.json" | awk '{print $1}')",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "control_binary_sha256": "$CONTROL_BINARY_SHA",
  "i2pd_libraries_sha256": "$I2PD_LIB_SHA",
  "observer_compile_time_gated": true,
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "plan": "076"
}
EOF

cat > "$OUTPUT_DIR/inspect-control.json" <<EOF
{
  "schema": "i2pr-i2pd-direct-driver-inspect-v1",
  "schema_version": 1,
  "build_manifest_sha256": "$(sha256sum "$OUTPUT_DIR/build-manifest-control.json" | awk '{print $1}')",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "control_binary_sha256": "$CONTROL_BINARY_SHA",
  "i2pd_libraries_sha256": "$I2PD_LIB_SHA",
  "observer_compile_time_gated": true,
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "plan": "076"
}
EOF

echo "build-driver.sh: instrumented binary at $OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented"
echo "build-driver.sh: uninstrumented control binary at $OUTPUT_DIR/i2pd_ntcp2_interop_driver_control"
echo "build-driver.sh: linked library manifest at $LINK_MANIFEST"
echo "build-driver.sh: instrumented build manifest at $OUTPUT_DIR/build-manifest-instrumented.json"
echo "build-driver.sh: control build manifest at $OUTPUT_DIR/build-manifest-control.json"
echo "build-driver.sh: inspect-instrumented record at $OUTPUT_DIR/inspect-instrumented.json"
echo "build-driver.sh: inspect-control record at $OUTPUT_DIR/inspect-control.json"
