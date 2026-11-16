#!/usr/bin/env bash
set -euo pipefail

I2PD_SOURCE_DIR=""
OUTPUT_DIR=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --i2pd-source-dir) I2PD_SOURCE_DIR="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --help|-h)
            echo "usage: build.sh --i2pd-source-dir <pinned-source> --output-dir <owned-output>"
            exit 0
            ;;
        *) echo "build.sh: unknown argument: $1" >&2; exit 64 ;;
    esac
done

if [[ -z "$I2PD_SOURCE_DIR" || -z "$OUTPUT_DIR" ]]; then
    echo "build.sh: --i2pd-source-dir and --output-dir are required" >&2
    exit 64
fi
I2PD_SOURCE_DIR="$(cd "$I2PD_SOURCE_DIR" && pwd)"
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
EXPECTED_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
ACTUAL_PIN="$(git -C "$I2PD_SOURCE_DIR" rev-parse HEAD)"
if [[ "$ACTUAL_PIN" != "$EXPECTED_PIN" ]]; then
    echo "build.sh: i2pd pin mismatch: expected $EXPECTED_PIN got $ACTUAL_PIN" >&2
    exit 1
fi
if [[ -n "$(git -C "$I2PD_SOURCE_DIR" status --porcelain --untracked-files=no)" ]]; then
    echo "build.sh: refusing a tracked-modified i2pd source tree" >&2
    exit 1
fi
REFERENCE_TREE_SHA="$(python3 -c '
import hashlib, subprocess, sys
tree = sys.argv[1]
entries = subprocess.run(["git", "-C", tree, "ls-files", "-z"], check=True, capture_output=True).stdout
stream = bytearray()
for entry in entries.split(b"\0"):
    if not entry:
        continue
    stream.extend(entry + b"\0")
    path = tree + "/" + entry.decode("utf-8")
    with open(path, "rb") as handle:
        stream.extend(hashlib.sha256(handle.read()).digest() + b"\0")
print(hashlib.sha256(stream).hexdigest())
' "$I2PD_SOURCE_DIR")"

LIB_BUILD="$OUTPUT_DIR/i2pd-library-build"
DRIVER_BUILD="$OUTPUT_DIR/driver-build"
cmake -S "$I2PD_SOURCE_DIR/build" -B "$LIB_BUILD" \
    -DCMAKE_BUILD_TYPE=Release -DWITH_HARDENING=OFF \
    -DWITH_BINARY=OFF -DWITH_LIBRARY=ON -DBUILD_TESTING=OFF \
    -DWITH_UPNP=OFF
cmake --build "$LIB_BUILD" --parallel 2
for archive in libi2pd.a libi2pdclient.a libi2pdlang.a; do
    [[ -f "$LIB_BUILD/$archive" ]] || {
        echo "build.sh: missing expected archive $archive" >&2
        exit 1
    }
done
cmake -S "$(dirname "$0")" -B "$DRIVER_BUILD" \
    -DCMAKE_BUILD_TYPE=Release \
    -DI2PD_SOURCE_DIR="$I2PD_SOURCE_DIR" \
    -DI2PD_LIBRARY_DIR="$LIB_BUILD"
cmake --build "$DRIVER_BUILD" --parallel 2
install -m 0755 "$DRIVER_BUILD/i2pd_current_ntcp2_driver" \
    "$OUTPUT_DIR/i2pd-current-ntcp2-driver"
{
    printf 'reference_revision=%s\n' "$EXPECTED_PIN"
    printf 'reference_tree_sha256=%s\n' "$REFERENCE_TREE_SHA"
    sha256sum "$(dirname "$0")/src/i2pd_current_ntcp2_driver.cpp" \
        "$(dirname "$0")/CMakeLists.txt" \
        "$(dirname "$0")/build.sh" \
        "$OUTPUT_DIR/i2pd-current-ntcp2-driver"
} > "$OUTPUT_DIR/build-manifest.txt"
