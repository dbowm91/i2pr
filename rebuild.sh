#!/usr/bin/env bash
set -euo pipefail
cd /home/i2ptest/i2pr
exec bash scripts/interop/build-references.sh --force-rebuild
