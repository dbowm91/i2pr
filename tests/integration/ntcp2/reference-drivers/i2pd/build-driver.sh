#!/usr/bin/env bash
# Plan 099 i2pd direct NTCP2 driver build script.
#
# Plan 099 WP1.1-WP1.3: this script performs three ordered library
# build stages followed by a dual-target driver build:
#
#   1. Configure and build the **unmodified** pinned i2pd 2.60.0
#      source tree as static libraries (``libi2pd``,
#      ``libi2pdclient``, ``libi2pdlang``) via the pinned i2pd
#      CMake project. The pristine tree is *never* mutated. The
#      pristine library archives feed the control driver binary;
#      they contain no observer call sites.
#
#   2. Apply the Plan 076 observer patch to a private copy of the
#      pinned tree, copy the observer header/source into the
#      patched tree, and configure + build the i2pd CMake project
#      **against the patched tree** with ``-DI2PD_INTEROP_OBSERVER=1``
#      visible to the patched ``NTCP2.cpp`` compile. The
#      instrumented library archives feed the instrumented driver
#      binary; they contain the real observer call sites that
#      fire after AEAD verification, block bounds validation,
#      and FromNTCP2 conversion.
#
#   3. Configure the Plan 099 driver CMake project with the
#      explicit instrumented and pristine library directories,
#      and build both driver binaries against their respective
#      library sets. ``I2PD_INSTRUMENTED_LIB_DIR`` is consumed by
#      the instrumented binary; ``I2PD_PRISTINE_LIB_DIR`` is
#      consumed by the control binary. Header include-path
#      differences alone do not satisfy WP1.3.
#
#   4. Emit object-level proof: ``nm -C`` and ``objdump -d`` show
#      that the instrumented NTCP2 object/archive carries
#      ``i2pr::i2pdinterop::Observe*`` references and the pristine
#      NTCP2 object/archive carries no such references.
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
#                       pinned i2pd archive SHAs, the linked library
#                       manifest, both driver binaries, both build
#                       manifests, and the object-level proof file.

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

# Phase 1: compute provenance digests. Plan 098: the canonical
# tracked-source identity must match the workflow's
# ``record-source-tree-digest`` step and the wrapper's
# ``_canonical_tracked_tree_digest`` helper. The algorithm walks
# ``git ls-files -z`` (excluding the ``.git`` administrative tree)
# and hashes each tracked file's bytes; the digests are
# concatenated in stable git order with a single NUL separator
# before the final SHA-256 is computed. Drift between the
# workflow, the build script, and the wrapper would break the
# cross-reference identity, so the same Python helper may be
# reused from the wrapper when available.
if command -v python3 >/dev/null 2>&1; then
    PRISTINE_TREE_SHA="$(python3 -c '
import hashlib, subprocess, sys
tree = sys.argv[1]
out = subprocess.run(
    ["git", "-C", tree, "ls-files", "-z"],
    check=True, capture_output=True,
).stdout
stream = bytearray()
for entry in out.split(b"\x00"):
    if not entry:
        continue
    stream.extend(entry)
    stream.extend(b"\x00")
    file_path = tree + "/" + entry.decode("utf-8")
    try:
        with open(file_path, "rb") as handle:
            stream.extend(hashlib.sha256(handle.read()).digest())
            stream.extend(b"\x00")
    except OSError:
        pass
print(hashlib.sha256(bytes(stream)).hexdigest())
' "$I2PD_SOURCE_DIR")"
else
    # POSIX fallback: enumerate tracked paths via git, hash each
    # file's bytes, aggregate a single SHA-256. The fallback is
    # slower but keeps the algorithm deterministic across hosts.
    PRISTINE_TREE_SHA="$(cd "$I2PD_SOURCE_DIR" && git ls-files -z | tr '\0' '\n' \
        | while IFS= read -r path; do
            printf '%s\0' "$path"
            sha256sum "$path" | awk '{printf "%s\0", $1}'
          done \
        | sha256sum \
        | awk '{print $1}')"
fi
SOURCE_LOCK_SHA="$(sha256sum "$SOURCE_LOCK" | awk '{print $1}')"
OBSERVER_PATCH_SHA="$(sha256sum "$OBSERVER_PATCH" | awk '{print $1}')"
HELPER_SOURCE_SHA="$(sha256sum "$HELPER_SOURCE" | awk '{print $1}')"
OBSERVER_HEADER_SHA="$(sha256sum "$OBSERVER_HEADER" | awk '{print $1}')"
OBSERVER_SOURCE_SHA="$(sha256sum "$OBSERVER_SOURCE" | awk '{print $1}')"

# Phase 2: configure and build the pinned i2pd libraries **twice**:
# once from the pristine tree (untouched) and once from the patched
# private copy with I2PD_INTEROP_OBSERVER=1 visible to the patched
# NTCP2.cpp compile. Plan 099 WP1.1-WP1.3 requires separate pristine
# and instrumented library directories; the driver CMake consumes
# the two directories via I2PD_PRISTINE_LIB_DIR and
# I2PD_INSTRUMENTED_LIB_DIR respectively.

# Pristine library build (untouched pinned tree).
PRISTINE_LIB_BUILD="$(mktemp -d -t plan099-i2pd-pristine-XXXXXX)"

cmake \
    -S "$I2PD_SOURCE_DIR/build" \
    -B "$PRISTINE_LIB_BUILD" \
    -DCMAKE_BUILD_TYPE=Release \
    -DWITH_HARDENING=OFF \
    -DWITH_BINARY=OFF \
    -DWITH_LIBRARY=ON \
    -DBUILD_TESTING=OFF \
    -DWITH_UPNP=OFF >/dev/null

cmake --build "$PRISTINE_LIB_BUILD" --parallel 2 >/dev/null

if [[ ! -f "$PRISTINE_LIB_BUILD/libi2pd.a" || \
      ! -f "$PRISTINE_LIB_BUILD/libi2pdclient.a" || \
      ! -f "$PRISTINE_LIB_BUILD/libi2pdlang.a" ]]; then
    echo "build-driver.sh: pristine i2pd libraries were not produced" >&2
    exit 64
fi

# Phase 3: apply the observer patch to a private copy of the pinned
# tree and copy the observer header/source into the patched tree so
# the patched NTCP2.cpp sees them via the i2pd build include path.
PATCHED_SRC="$(mktemp -d -t plan099-i2pd-patched-src-XXXXXX)"
trap 'rm -rf "$PRISTINE_LIB_BUILD" "$INSTRUMENTED_LIB_BUILD" "$PATCHED_SRC"' EXIT

cp -R "$I2PD_SOURCE_DIR/." "$PATCHED_SRC/"
cp "$OBSERVER_HEADER" "$PATCHED_SRC/libi2pd/interop_observer.h"
cp "$OBSERVER_SOURCE" "$PATCHED_SRC/libi2pd/interop_observer.cpp"
(cd "$PATCHED_SRC" && patch -p1 --fuzz=0 --dry-run < "$OBSERVER_PATCH" >/dev/null)
(cd "$PATCHED_SRC" && patch -p1 --fuzz=0 < "$OBSERVER_PATCH" >/dev/null)

# Instrumented library build (patched tree with observer macro
# visible). The NTCP2.cpp translation unit in the patched tree
# activates the observer call sites when I2PD_INTEROP_OBSERVER=1 is
# exported into the build environment, so the resulting static
# archives carry real observer references.
INSTRUMENTED_LIB_BUILD="$(mktemp -d -t plan099-i2pd-instrumented-XXXXXX)"

cmake \
    -S "$PATCHED_SRC/build" \
    -B "$INSTRUMENTED_LIB_BUILD" \
    -DCMAKE_BUILD_TYPE=Release \
    -DWITH_HARDENING=OFF \
    -DWITH_BINARY=OFF \
    -DWITH_LIBRARY=ON \
    -DBUILD_TESTING=OFF \
    -DWITH_UPNP=OFF \
    -DCMAKE_CXX_FLAGS="-DI2PD_INTEROP_OBSERVER=1" >/dev/null

cmake --build "$INSTRUMENTED_LIB_BUILD" --parallel 2 >/dev/null

if [[ ! -f "$INSTRUMENTED_LIB_BUILD/libi2pd.a" || \
      ! -f "$INSTRUMENTED_LIB_BUILD/libi2pdclient.a" || \
      ! -f "$INSTRUMENTED_LIB_BUILD/libi2pdlang.a" ]]; then
    echo "build-driver.sh: instrumented i2pd libraries were not produced" >&2
    exit 64
fi

# Phase 3b: emit object-level proof per WP1.4. ``nm -C`` walks the
# static archives and shows whether the i2pd NTCP2 transport object
# references the i2pr observer API. The pristine NTCP2 object must
# carry no such reference; the instrumented NTCP2 object must carry
# them.
PROOF_PATH="$OUTPUT_DIR/object-level-proof.txt"
{
    echo "# Plan 099 i2pd direct driver object-level proof"
    echo "# generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo
    echo "# pristine i2pd libi2pd.a: i2pr::i2pdinterop::Observe* references (must be empty)"
    if command -v nm >/dev/null 2>&1; then
        nm -C "$PRISTINE_LIB_BUILD/libi2pd.a" 2>/dev/null \
            | rtk rg 'i2pr::i2pdinterop::Observe' || true
    else
        echo "(nm not available)"
    fi
    echo
    echo "# instrumented i2pd libi2pd.a: i2pr::i2pdinterop::Observe* references (must be non-empty)"
    if command -v nm >/dev/null 2>&1; then
        nm -C "$INSTRUMENTED_LIB_BUILD/libi2pd.a" 2>/dev/null \
            | rtk rg 'i2pr::i2pdinterop::Observe' || true
    else
        echo "(nm not available)"
    fi
    echo
    echo "# pristine NTCP2.o object (must be empty):"
    pristine_ntcp2="$(find "$PRISTINE_LIB_BUILD" -name 'NTCP2.cpp.o' -path '*/CMakeFiles/libi2pd.dir/*' 2>/dev/null | head -n1)"
    if [[ -n "$pristine_ntcp2" && -f "$pristine_ntcp2" ]]; then
        nm -C "$pristine_ntcp2" 2>/dev/null \
            | rtk rg 'i2pr::i2pdinterop::Observe' || echo "(none)"
    else
        echo "(NTCP2.cpp.o not found in pristine build)"
    fi
    echo
    echo "# instrumented NTCP2.cpp.o object (must reference i2pr::i2pdinterop::Observe*):"
    instrumented_ntcp2="$(find "$INSTRUMENTED_LIB_BUILD" -name 'NTCP2.cpp.o' -path '*/CMakeFiles/libi2pd.dir/*' 2>/dev/null | head -n1)"
    if [[ -n "$instrumented_ntcp2" && -f "$instrumented_ntcp2" ]]; then
        nm -C "$instrumented_ntcp2" 2>/dev/null \
            | rtk rg 'i2pr::i2pdinterop::Observe' || echo "(none)"
    else
        echo "(NTCP2.cpp.o not found in instrumented build)"
    fi
} > "$PROOF_PATH"
PROOF_PATH_SHA="$(sha256sum "$PROOF_PATH" | awk '{print $1}')"

# Phase 4: build the Plan 099 driver against the freshly built
# instrumented and pristine libraries. The driver CMake enforces
# separate I2PD_INSTRUMENTED_LIB_DIR and I2PD_PRISTINE_LIB_DIR cache
# variables; mixing the two sets is impossible.
DRIVER_BUILD="$(mktemp -d -t plan099-driver-build-XXXXXX)"

cmake \
    -S "$HELPER_DIR" \
    -B "$DRIVER_BUILD" \
    -DCMAKE_BUILD_TYPE=Release \
    -DI2PD_PATCHED_TREE="$PATCHED_SRC" \
    -DI2PD_PRISTINE_TREE="$I2PD_SOURCE_DIR" \
    -DI2PD_INSTRUMENTED_LIB_DIR="$INSTRUMENTED_LIB_BUILD" \
    -DI2PD_PRISTINE_LIB_DIR="$PRISTINE_LIB_BUILD" >/dev/null

cmake --build "$DRIVER_BUILD" \
    --target i2pd_ntcp2_interop_driver_instrumented \
    --target i2pd_ntcp2_interop_driver_control \
    --parallel 2 >/dev/null

INSTRUMENTED_BINARY="$DRIVER_BUILD/i2pd_ntcp2_interop_driver_instrumented"
CONTROL_BINARY="$DRIVER_BUILD/i2pd_ntcp2_interop_driver_control"
PRISTINE_LIB_DIR="$PRISTINE_LIB_BUILD"
INSTRUMENTED_LIB_DIR="$INSTRUMENTED_LIB_BUILD"

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
# Plan 098: bare ``cp`` defaults to the umask mode (typically 0644
# in the Ubuntu 24.04 GitHub-hosted runner), which strips the
# executable bit the source binary carries. The downstream
# ``actions/upload-artifact`` zips the file with its current mode
# and the ``actions/download-artifact`` step extracts the same
# mode on the consumer job; the live jobs then fail the
# ``test -x`` guard with a typed ``ci_build_blocked``. Restore the
# executable bit explicitly so the upload/download round-trip
# preserves the binary's runnability.
chmod 0755 \
    "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented" \
    "$OUTPUT_DIR/i2pd_ntcp2_interop_driver_control"

# Phase 4: linked library provenance manifest.
LINK_MANIFEST="$OUTPUT_DIR/linked-library-manifest.txt"
{
    echo "# Plan 099 i2pd direct driver linked-library manifest"
    echo "# generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "# compiler: $(g++ --version | head -n1)"
    echo "# cmake: $(cmake --version | head -n1)"
    echo
    echo "# pristine i2pd libraries (built from the untouched pinned tree; linked by the control binary):"
    sha256sum "$PRISTINE_LIB_DIR/libi2pd.a" "$PRISTINE_LIB_DIR/libi2pdclient.a" "$PRISTINE_LIB_DIR/libi2pdlang.a"
    echo
    echo "# instrumented i2pd libraries (built from the patched pinned tree with I2PD_INTEROP_OBSERVER=1; linked by the instrumented binary):"
    sha256sum "$INSTRUMENTED_LIB_DIR/libi2pd.a" "$INSTRUMENTED_LIB_DIR/libi2pdclient.a" "$INSTRUMENTED_LIB_DIR/libi2pdlang.a"
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
PRISTINE_LIB_SHA="$(sha256sum "$PRISTINE_LIB_DIR/libi2pd.a" "$PRISTINE_LIB_DIR/libi2pdclient.a" "$PRISTINE_LIB_DIR/libi2pdlang.a" \
    | awk '{print $1}' | paste -sd' ' -)"
INSTRUMENTED_LIB_SHA="$(sha256sum "$INSTRUMENTED_LIB_DIR/libi2pd.a" "$INSTRUMENTED_LIB_DIR/libi2pdclient.a" "$INSTRUMENTED_LIB_DIR/libi2pdlang.a" \
    | awk '{print $1}' | paste -sd' ' -)"
# Backwards-compat field kept equal to the pristine digest so the
# static boundary checker and the historical build-manifest schema
# remain satisfied.
I2PD_LIB_SHA="$PRISTINE_LIB_SHA"
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
  "i2pd_libraries_sha256": "$PRISTINE_LIB_SHA",
  "instrumented_libraries_sha256": "$INSTRUMENTED_LIB_SHA",
  "object_level_proof_sha256": "$PROOF_PATH_SHA",
  "cmake_version": "$CMAKE_VERSION",
  "compiler_version": "$COMPILER_VERSION",
  "cmake_build_type": "Release",
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "linked_i2pd_sources": true,
  "observer_compile_time_gated": true,
  "plan": "099"
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
  "i2pd_libraries_sha256": "$PRISTINE_LIB_SHA",
  "instrumented_libraries_sha256": "$INSTRUMENTED_LIB_SHA",
  "object_level_proof_sha256": "$PROOF_PATH_SHA",
  "cmake_version": "$CMAKE_VERSION",
  "compiler_version": "$COMPILER_VERSION",
  "cmake_build_type": "Release",
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "linked_i2pd_sources": true,
  "observer_compile_time_gated": true,
  "plan": "099"
}
EOF

cat > "$OUTPUT_DIR/inspect-instrumented.json" <<EOF
{
  "schema": "i2pr-i2pd-direct-driver-inspect-v1",
  "schema_version": 1,
  "build_manifest_sha256": "$(sha256sum "$OUTPUT_DIR/build-manifest-instrumented.json" | awk '{print $1}')",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "control_binary_sha256": "$CONTROL_BINARY_SHA",
  "i2pd_libraries_sha256": "$PRISTINE_LIB_SHA",
  "instrumented_libraries_sha256": "$INSTRUMENTED_LIB_SHA",
  "object_level_proof_sha256": "$PROOF_PATH_SHA",
  "observer_compile_time_gated": true,
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "plan": "099"
}
EOF

cat > "$OUTPUT_DIR/inspect-control.json" <<EOF
{
  "schema": "i2pr-i2pd-direct-driver-inspect-v1",
  "schema_version": 1,
  "build_manifest_sha256": "$(sha256sum "$OUTPUT_DIR/build-manifest-control.json" | awk '{print $1}')",
  "instrumented_binary_sha256": "$INSTRUMENTED_BINARY_SHA",
  "control_binary_sha256": "$CONTROL_BINARY_SHA",
  "i2pd_libraries_sha256": "$PRISTINE_LIB_SHA",
  "instrumented_libraries_sha256": "$INSTRUMENTED_LIB_SHA",
  "object_level_proof_sha256": "$PROOF_PATH_SHA",
  "observer_compile_time_gated": true,
  "build_timestamp_utc": "$TIMESTAMP",
  "build_command_version": "build-driver.sh-v1",
  "plan": "099"
}
EOF

echo "build-driver.sh: instrumented binary at $OUTPUT_DIR/i2pd_ntcp2_interop_driver_instrumented"
echo "build-driver.sh: uninstrumented control binary at $OUTPUT_DIR/i2pd_ntcp2_interop_driver_control"
echo "build-driver.sh: linked library manifest at $LINK_MANIFEST"
echo "build-driver.sh: object-level proof at $PROOF_PATH"
echo "build-driver.sh: instrumented build manifest at $OUTPUT_DIR/build-manifest-instrumented.json"
echo "build-driver.sh: control build manifest at $OUTPUT_DIR/build-manifest-control.json"
echo "build-driver.sh: inspect-instrumented record at $OUTPUT_DIR/inspect-instrumented.json"
echo "build-driver.sh: inspect-control record at $OUTPUT_DIR/inspect-control.json"
