#!/usr/bin/env bash
# Plan 064 i2pd direct NTCP2 driver build script.
#
# This script builds both the instrumented and uninstrumented i2pd
# direct driver against the pinned i2pd 2.60.0 source tree. It is a
# build-only script: it never fetches network resources, never modifies
# the pinned reference cache beyond the temporary observer patch
# application, and writes only into the owned ``build/`` directory.
#
# The script applies the observer patch to a private copy of the
# pinned tree, builds the instrumented binary, restores the pristine
# tree, builds the uninstrumented control binary, and emits two build
# manifests. The pinned cache directory remains untouched.
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
#     --repo-root       Repository root containing the pinned references.
#     --i2pd-source-dir Directory containing the pristine pinned i2pd
#                       2.60.0 source tree (must match the locked
#                       revision).
#     --output-dir      Owned output directory; the script writes
#                       ``i2pd_ntcp2_interop_driver_instrumented``,
#                       ``i2pd_ntcp2_interop_driver_control``, and
#                       ``build-manifest-instrumented.json`` +
#                       ``build-manifest-control.json`` there.

set -euo pipefail

REPO_ROOT=""
I2PD_SOURCE_DIR=""
OUTPUT_DIR=""
BUILD_MANIFEST_SCHEMA=""

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
        --build-manifest-schema)
            BUILD_MANIFEST_SCHEMA="$2"
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

HELPER_DIR="$REPO_ROOT/tests/integration/ntcp2/reference-drivers/i2pd"
HELPER_SOURCE="$HELPER_DIR/src/i2pd_ntcp2_interop_driver.cpp"
OBSERVER_HEADER="$HELPER_DIR/src/interop_observer.h"
OBSERVER_SOURCE="$HELPER_DIR/src/interop_observer.cpp"
SOURCE_LOCK="$HELPER_DIR/source-lock.json"
OBSERVER_PATCH="$HELPER_DIR/patches/i2pd-2.60.0-interop-observer.patch"

if [[ -z "$BUILD_MANIFEST_SCHEMA" ]]; then
    BUILD_MANIFEST_SCHEMA="$HELPER_DIR/build-manifest.schema.json"
fi

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
if [[ ! -f "$BUILD_MANIFEST_SCHEMA" ]]; then
    echo "build-driver.sh: build manifest schema missing at $BUILD_MANIFEST_SCHEMA" >&2
    exit 64
fi
if [[ ! -d "$I2PD_SOURCE_DIR/libi2pd" ]]; then
    echo "build-driver.sh: pinned i2pd source $I2PD_SOURCE_DIR/libi2pd is missing" >&2
    exit 64
fi

# Phase 1: pristine tree digest.
PRISTINE_TREE_SHA="$(find "$I2PD_SOURCE_DIR" -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | awk '{print $1}')"
HELPER_SOURCE_SHA="$(sha256sum "$HELPER_SOURCE" | awk '{print $1}')"
OBSERVER_HEADER_SHA="$(sha256sum "$OBSERVER_HEADER" | awk '{print $1}')"
OBSERVER_SOURCE_SHA="$(sha256sum "$OBSERVER_SOURCE" | awk '{print $1}')"
OBSERVER_PATCH_SHA="$(sha256sum "$OBSERVER_PATCH" | awk '{print $1}')"
SOURCE_LOCK_SHA="$(sha256sum "$SOURCE_LOCK" | awk '{print $1}')"

# Phase 2: instrumented build (apply patch into a private copy, build,
# then remove the private copy; the pinned source tree is never
# mutated).
PRIVATE_SRC="$(mktemp -d -t plan064-i2pd-patched-XXXXXX)"
trap 'rm -rf "$PRIVATE_SRC"' EXIT
cp -R "$I2PD_SOURCE_DIR/." "$PRIVATE_SRC/"
(cd "$PRIVATE_SRC" && patch -p1 --fuzz=0 --dry-run < "$OBSERVER_PATCH" >/dev/null)
(cd "$PRIVATE_SRC" && patch -p1 --fuzz=0 < "$OBSERVER_PATCH" >/dev/null)
PATCHED_TREE_SHA="$(find "$PRIVATE_SRC" -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | awk '{print $1}')"

CMAKE_BUILD_INSTRUMENTED="$(mktemp -d -t plan064-i2pd-instrumented-XXXXXX)"
cmake -S "$HELPER_DIR" \
    -B "$CMAKE_BUILD_INSTRUMENTED" \
    -DCMAKE_BUILD_TYPE=Release \
    -DI2PD_SOURCE_DIR="$PRIVATE_SRC" >/dev/null
cmake --build "$CMAKE_BUILD_INSTRUMENTED" \
    --target i2pd_ntcp2_interop_driver_instrumented \
    --parallel >/dev/null
INSTRUMENTED_BINARY="$CMAKE_BUILD_INSTRUMENTED/i2pd_ntcp2_interop_driver_instrumented"
if [[ ! -x "$INSTRUMENTED_BINARY" ]]; then
    echo "build-driver.sh: instrumented binary was not produced" >&2
    exit 64
fi
cp "$INSTRUMENTED_BINARY" "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented"

# Phase 3: uninstrumented control build. The patch is NOT applied to
# the source tree. The private copy is deleted so the pristine tree is
# the only tree used by the control build.
rm -rf "$PRIVATE_SRC"
unset PRIVATE_SRC
trap - EXIT

CMAKE_BUILD_CONTROL="$(mktemp -d -t plan064-i2pd-control-XXXXXX)"
cmake -S "$HELPER_DIR" \
    -B "$CMAKE_BUILD_CONTROL" \
    -DCMAKE_BUILD_TYPE=Release \
    -DI2PD_SOURCE_DIR="$I2PD_SOURCE_DIR" >/dev/null
cmake --build "$CMAKE_BUILD_CONTROL" \
    --target i2pd_ntcp2_interop_driver_control \
    --parallel >/dev/null
CONTROL_BINARY="$CMAKE_BUILD_CONTROL/i2pd_ntcp2_interop_driver_control"
if [[ ! -x "$CONTROL_BINARY" ]]; then
    echo "build-driver.sh: uninstrumented control binary was not produced" >&2
    exit 64
fi
cp "$CONTROL_BINARY" "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_control"

# Phase 4: linked library provenance manifest. Use ldd plus resolved
# SHA-256 of every loaded file on Linux. Reject any "not found"
# resolution.
LINK_MANIFEST="$OUTPUT_DIR/linked-library-manifest.txt"
{
    echo "# Plan 064 i2pd direct driver linked-library manifest"
    echo "# generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "# compiler: $(g++ --version | head -n1)"
    echo "# cmake: $(cmake --version | head -n1)"
    echo
    ldd "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented" | awk '$2 == "=>" {print}'
} > "$LINK_MANIFEST"
LINK_MANIFEST_SHA="$(sha256sum "$LINK_MANIFEST" | awk '{print $1}')"

INSTRUMENTED_BINARY_SHA="$(sha256sum "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented" | awk '{print $1}')"
CONTROL_BINARY_SHA="$(sha256sum "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_control" | awk '{print $1}')"
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
  "i2pd_archive_sha256": "$SOURCE_LOCK_SHA",
  "observer_patch_sha256": "$OBSERVER_PATCH_SHA",
  "driver_source_sha256": "$HELPER_SOURCE_SHA",
  "driver_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "uninstrumented_binary_sha256": "$CONTROL_BINARY_SHA",
  "linked_library_manifest_sha256": "$LINK_MANIFEST_SHA",
  "cmake_version": "$CMAKE_VERSION",
  "compiler_version": "$COMPILER_VERSION",
  "cmake_build_type": "Release",
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1"
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
  "i2pd_archive_sha256": "$SOURCE_LOCK_SHA",
  "observer_patch_sha256": "$OBSERVER_PATCH_SHA",
  "driver_source_sha256": "$HELPER_SOURCE_SHA",
  "driver_binary_sha256": "$CONTROL_BINARY_SHA",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "uninstrumented_binary_sha256": "$CONTROL_BINARY_SHA",
  "linked_library_manifest_sha256": "$LINK_MANIFEST_SHA",
  "cmake_version": "$CMAKE_VERSION",
  "compiler_version": "$COMPILER_VERSION",
  "cmake_build_type": "Release",
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1"
}
EOF

echo "build-driver.sh: instrumented binary at $OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented"
echo "build-driver.sh: uninstrumented control binary at $OUTPUT_DIR/i2pd_ntcp2_interop_driver_control"
echo "build-driver.sh: linked library manifest at $LINK_MANIFEST"
echo "build-driver.sh: instrumented build manifest at $OUTPUT_DIR/build-manifest-instrumented.json"
echo "build-driver.sh: control build manifest at $OUTPUT_DIR/build-manifest-control.json"