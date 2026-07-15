#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGETS=(
  date date32 hash mapping certificate key_certificate key_and_cert router_identity
  destination router_address router_info lease lease_set
  i2np_standard i2np_bodies i2np_short_ssu i2np_short_transport
  ntcp2_transcript ntcp2_storage ntcp2_handshake ntcp2_blocks ntcp2_frames
)

if ! command -v cargo-fuzz >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo-fuzz is required for fuzz smoke tests but was not found.
Install it with `cargo install cargo-fuzz` (and use a nightly Rust toolchain),
then rerun: bash scripts/fuzz-smoke.sh
EOF
  exit 127
fi

if ! command -v rustup >/dev/null 2>&1 || ! rustup toolchain list | grep -q '^nightly'; then
  cat >&2 <<'EOF'
A nightly Rust toolchain is required for libfuzzer-sys and cargo-fuzz.
Install one with `rustup toolchain install nightly`, then rerun:
`bash scripts/fuzz-smoke.sh`
EOF
  exit 127
fi

# The managed execution environment may run sanitizer binaries under ptrace,
# which makes LeakSanitizer abort before the parser is exercised. Full
# campaigns should re-enable leak detection in a normal terminal/CI runner.
export LSAN_OPTIONS="${LSAN_OPTIONS:-detect_leaks=0}"

for target in "${TARGETS[@]}"; do
  echo "fuzz smoke: $target"
  RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir "$ROOT_DIR/fuzz" "$target" -- -runs=32 -seed=1
done
