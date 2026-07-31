#!/usr/bin/env bash
# Plan 063 Java I2P stripped-router direct NTCP2 driver runtime script.
#
# This script is the runnable seam for the Java direct driver. It does
# not perform any build or fetch operations; the helper jar must
# already exist alongside the build manifest.
#
# Usage:
#
#     bash tests/integration/ntcp2/reference-drivers/java/run-driver.sh \
#         --driver-jar <driver.jar> \
#         --java-cache <pinned-java-cache-dir> \
#         --mode <listen|dial|inspect> \
#         --config <strict-config.json> \
#         [--output-dir <owned-output-dir>]
#
# Required flags:
#
#     --driver-jar    Path to the compiled Java direct driver jar.
#     --java-cache    Directory containing the pinned Java I2P 2.12.0
#                     install tree (must match the locked revision).
#     --mode          Mode selector: ``listen``, ``dial``, or ``inspect``.
#     --config        Strict JSON config rendered by the harness.
#
# Optional flags:
#
#     --output-dir    Owned output directory; default is the directory
#                     containing the rendered config.

set -euo pipefail

DRIVER_JAR=""
JAVA_CACHE=""
MODE=""
CONFIG=""
OUTPUT_DIR=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --driver-jar)
            DRIVER_JAR="$2"
            shift 2
            ;;
        --java-cache)
            JAVA_CACHE="$2"
            shift 2
            ;;
        --mode)
            MODE="$2"
            shift 2
            ;;
        --config)
            CONFIG="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --help|-h)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        *)
            echo "run-driver.sh: unknown argument: $1" >&2
            exit 64
            ;;
    esac
done

if [[ -z "$DRIVER_JAR" || -z "$JAVA_CACHE" || -z "$MODE" || -z "$CONFIG" ]]; then
    echo "run-driver.sh: --driver-jar, --java-cache, --mode, --config are required" >&2
    exit 64
fi

if [[ "$MODE" != "listen" && "$MODE" != "dial" && "$MODE" != "inspect" ]]; then
    echo "run-driver.sh: --mode must be one of listen|dial|inspect" >&2
    exit 64
fi

DRIVER_JAR="$(cd "$(dirname "$DRIVER_JAR")" && pwd)/$(basename "$DRIVER_JAR")"
JAVA_CACHE="$(cd "$JAVA_CACHE" && pwd)"
if [[ -z "$OUTPUT_DIR" ]]; then
    OUTPUT_DIR="$(cd "$(dirname "$CONFIG")" && pwd)"
fi
mkdir -p "$OUTPUT_DIR"

mapfile -t CLASSPATH_JARS < <(find "$JAVA_CACHE/lib" -maxdepth 1 -type f -name '*.jar' | LC_ALL=C sort)
if [[ "${#CLASSPATH_JARS[@]}" -lt 2 ]]; then
    echo "run-driver.sh: pinned Java lib/ has fewer than two jars" >&2
    exit 64
fi

CLASS_PATH="$DRIVER_JAR:$(IFS=:; echo "${CLASSPATH_JARS[*]}")"

java \
    -Xmx512m \
    -Djava.awt.headless=true \
    --enable-native-access=ALL-UNNAMED \
    -Di2p.dir.base="$JAVA_CACHE" \
    -Di2p.dir.config="$JAVA_CACHE" \
    -Di2p.dir.router="$OUTPUT_DIR" \
    -classpath "$CLASS_PATH" \
    i2pr.ntcp2.JavaNtcp2InteropDriver "$MODE" --config "$CONFIG"
