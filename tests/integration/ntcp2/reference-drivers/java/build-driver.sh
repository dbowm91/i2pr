#!/usr/bin/env bash
# Plan 063 Java I2P stripped-router direct NTCP2 driver build script.
#
# This script builds the Java direct driver against the pinned Java I2P
# 2.12.0 jar set. It is a build-only script: it never fetches network
# resources, never modifies the pinned reference cache, and writes only
# into the owned ``build/`` and ``fixtures/`` directories.
#
# The script must be run from the repository root or from anywhere as
# long as the absolute path to the pinned Java cache is supplied.
#
# Usage:
#
#     bash tests/integration/ntcp2/reference-drivers/java/build-driver.sh \
#         --repo-root <repo-root> \
#         --java-cache <pinned-java-cache-dir> \
#         --output-dir <owned-output-dir>
#
# Required flags:
#
#     --repo-root       Repository root containing the pinned references.
#     --java-cache      Directory containing the pinned Java I2P 2.12.0
#                       install tree (must match the locked revision).
#     --output-dir      Owned output directory; the script writes
#                       ``driver.jar`` plus the build manifest there.
#
# Optional flags:
#
#     --classpath-manifest  Override the classpath manifest path.
#     --build-manifest      Override the build manifest output path.

set -euo pipefail

REPO_ROOT=""
JAVA_CACHE=""
OUTPUT_DIR=""
CLASSPATH_MANIFEST=""
BUILD_MANIFEST=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repo-root)
            REPO_ROOT="$2"
            shift 2
            ;;
        --java-cache)
            JAVA_CACHE="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --classpath-manifest)
            CLASSPATH_MANIFEST="$2"
            shift 2
            ;;
        --build-manifest)
            BUILD_MANIFEST="$2"
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

if [[ -z "$REPO_ROOT" || -z "$JAVA_CACHE" || -z "$OUTPUT_DIR" ]]; then
    echo "build-driver.sh: --repo-root, --java-cache, --output-dir are required" >&2
    exit 64
fi

REPO_ROOT="$(cd "$REPO_ROOT" && pwd)"
JAVA_CACHE="$(cd "$JAVA_CACHE" && pwd)"
OUTPUT_DIR="$(mkdir -p "$OUTPUT_DIR" && cd "$OUTPUT_DIR" && pwd)"
mkdir -p "$OUTPUT_DIR/classes"

if [[ -z "$CLASSPATH_MANIFEST" ]]; then
    CLASSPATH_MANIFEST="$REPO_ROOT/tests/integration/ntcp2/reference-drivers/java/classpath-manifest.json"
fi
if [[ -z "$BUILD_MANIFEST" ]]; then
    BUILD_MANIFEST="$OUTPUT_DIR/build-manifest.json"
fi

HELPER_DIR="$REPO_ROOT/tests/integration/ntcp2/reference-drivers/java"
HELPER_SOURCE="$HELPER_DIR/src/JavaNtcp2InteropDriver.java"
SOURCE_LOCK="$HELPER_DIR/source-lock.json"

if [[ ! -f "$HELPER_SOURCE" ]]; then
    echo "build-driver.sh: helper source missing at $HELPER_SOURCE" >&2
    exit 64
fi
if [[ ! -f "$SOURCE_LOCK" ]]; then
    echo "build-driver.sh: source-lock record missing at $SOURCE_LOCK" >&2
    exit 64
fi
if [[ ! -f "$CLASSPATH_MANIFEST" ]]; then
    echo "build-driver.sh: classpath manifest missing at $CLASSPATH_MANIFEST" >&2
    exit 64
fi
if [[ ! -d "$JAVA_CACHE/lib" ]]; then
    echo "build-driver.sh: pinned Java cache $JAVA_CACHE/lib is missing" >&2
    exit 64
fi

PINS_JAR="$JAVA_CACHE/lib/i2p.jar"
ROUTER_JAR="$JAVA_CACHE/lib/router.jar"
if [[ ! -f "$PINS_JAR" ]]; then
    echo "build-driver.sh: pinned i2p.jar missing at $PINS_JAR" >&2
    exit 64
fi
if [[ ! -f "$ROUTER_JAR" ]]; then
    echo "build-driver.sh: pinned router.jar missing at $ROUTER_JAR" >&2
    exit 64
fi

JAVAC_VERSION="$(javac -version 2>&1 | head -n1 | tr -d '\n')"
JAVA_VERSION="$(java -version 2>&1 | head -n1 | tr -d '\n')"

# Build a deterministic sorted classpath of every pinned jar in lib/.
mapfile -t CLASSPATH_JARS < <(find "$JAVA_CACHE/lib" -maxdepth 1 -type f -name '*.jar' | LC_ALL=C sort)
if [[ "${#CLASSPATH_JARS[@]}" -lt 2 ]]; then
    echo "build-driver.sh: pinned Java lib/ has fewer than two jars" >&2
    exit 64
fi

CLASS_PATH="$(IFS=:; echo "${CLASSPATH_JARS[*]}")"

javac -d "$OUTPUT_DIR/classes" -classpath "$CLASS_PATH" "$HELPER_SOURCE"

JAR_PATH="$OUTPUT_DIR/driver.jar"
(cd "$OUTPUT_DIR/classes" && jar --create --file="$JAR_PATH" --no-compress .)

I2P_JAR_SHA="$(sha256sum "$PINS_JAR" | awk '{print $1}')"
ROUTER_JAR_SHA="$(sha256sum "$ROUTER_JAR" | awk '{print $1}')"
ALL_JARS_SHA="$(printf '%s\n' "${CLASSPATH_JARS[@]}" | xargs -I{} sha256sum {} | awk '{print $1}' | LC_ALL=C sort | paste -sd' ' -)"
DRIVER_SOURCE_SHA="$(sha256sum "$HELPER_SOURCE" | awk '{print $1}')"
DRIVER_BINARY_SHA="$(sha256sum "$JAR_PATH" | awk '{print $1}')"
CLASSPATH_MANIFEST_SHA="$(sha256sum "$CLASSPATH_MANIFEST" | awk '{print $1}')"
SOURCE_LOCK_SHA="$(sha256sum "$SOURCE_LOCK" | awk '{print $1}')"
TREE_SHA="$SOURCE_LOCK_SHA"

TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

cat > "$BUILD_MANIFEST" <<EOF
{
  "schema": "i2pr-java-helper-build-manifest-v1",
  "schema_version": 1,
  "reference_name": "java_i2p",
  "reference_version": "2.12.0",
  "reference_revision": "2800040deee9bb376567b671ef2e9c34cf3e30b6",
  "reference_source_tree_sha256": "$TREE_SHA",
  "i2p_jar_sha256": "$I2P_JAR_SHA",
  "router_jar_sha256": "$ROUTER_JAR_SHA",
  "all_runtime_jar_sha256_values": [$ALL_JARS_SHA],
  "driver_source_sha256": "$DRIVER_SOURCE_SHA",
  "driver_binary_sha256": "$DRIVER_BINARY_SHA",
  "classpath_manifest_sha256": "$CLASSPATH_MANIFEST_SHA",
  "javac_version": "$JAVAC_VERSION",
  "java_version": "$JAVA_VERSION",
  "build_command_version": "build-driver.sh-v1",
  "build_timestamp_utc": "$TIMESTAMP"
}
EOF

echo "build-driver.sh: driver built at $JAR_PATH"
echo "build-driver.sh: build manifest at $BUILD_MANIFEST"
