#!/usr/bin/env bash
set -euo pipefail
RD=$(ls -d /home/i2ptest/i2pr/target/interop/runs/mixed-*/reference-data | tail -1)
rm -rf /tmp/check-ri2
mkdir -p /tmp/check-ri2
cp "$RD/router.info" /tmp/check-ri2/router.info
chmod 600 /tmp/check-ri2/router.info
/home/i2ptest/i2pr/target/debug/i2pr-interop ntcp2 inspect --state-dir /tmp/check-ri2