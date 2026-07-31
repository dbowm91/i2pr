#!/usr/bin/env bash
# Plan 064 i2pd direct NTCP2 driver runtime script.
#
# This script is the runnable seam for the i2pd direct driver. It does
# not perform any build or fetch operations; the helper binary must
# already exist alongside the build manifest.
#
# Usage:
#
#     bash tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh \
#         --driver-binary <instrumented-binary> \
#         --strict-config <strict-driver-config.json>
#
# Required flags:
#
#     --driver-binary  Path to the compiled i2pd direct driver binary
#                      (instrumented or uninstrumented control).
#     --strict-config  Path to the strict config JSON rendered by the
#                      harness.

set -euo pipefail

DRIVER_BINARY=""
STRICT_CONFIG=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --driver-binary)
            DRIVER_BINARY="$2"
            shift 2
            ;;
        --strict-config)
            STRICT_CONFIG="$2"
            shift 2
            ;;
        --help|-h)
            sed -n '2,24p' "$0"
            exit 0
            ;;
        *)
            echo "run-driver.sh: unknown argument: $1" >&2
            exit 64
            ;;
    esac
done

if [[ -z "$DRIVER_BINARY" || -z "$STRICT_CONFIG" ]]; then
    echo "run-driver.sh: --driver-binary and --strict-config are required" >&2
    exit 64
fi

DRIVER_BINARY="$(cd "$(dirname "$DRIVER_BINARY")" && pwd)/$(basename "$DRIVER_BINARY")"
STRICT_CONFIG="$(cd "$(dirname "$STRICT_CONFIG")" && pwd)/$(basename "$STRICT_CONFIG")"

if [[ ! -x "$DRIVER_BINARY" ]]; then
    echo "run-driver.sh: driver binary $DRIVER_BINARY is not executable" >&2
    exit 64
fi
if [[ ! -f "$STRICT_CONFIG" ]]; then
    echo "run-driver.sh: strict config $STRICT_CONFIG is not a regular file" >&2
    exit 64
fi

# The driver parses the strict config and self-determines the mode.
"$DRIVER_BINARY" --config "$STRICT_CONFIG"