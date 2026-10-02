#!/usr/bin/env bash
# Plan 298 evidence-shape and reference-lock guard.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lock="${root}/tests/integration/anonymity/references.lock.toml"
matrix="${root}/tests/integration/anonymity/streaming-scenarios.toml"
source="${root}/crates/i2pr-testkit/src/streaming_fingerprint.rs"

for required in \
  '635b013a612ff47278ef02acf8580a28e10e26c5' \
  '9134f808337b401e8e53c73734c81fab04280c9d'; do
  grep -Fq "${required}" "${lock}" || {
    echo "Streaming fingerprint reference lock missing ${required}" >&2
    exit 1
  }
done

for scenario in clean_handshake small_payload send_window_saturation delayed_first_ack \
  ack_withheld single_loss deterministic_reorder constrained_window choke_unchoke \
  orderly_close abrupt_close; do
  grep -Fq "name = \"${scenario}\"" "${matrix}" || {
    echo "Streaming fingerprint scenario missing ${scenario}" >&2
    exit 1
  }
done

if grep -REni 'payload_bytes|payload_sha256|destination_hash|peer_hash|peer_address|private_key|private_identity' \
  "${root}/tests/integration/anonymity" --include='*.tsv' --include='*.json' >/dev/null 2>&1; then
  echo "Streaming fingerprint evidence contains a forbidden identity/payload field" >&2
  exit 1
fi

# Seed one forbidden field and prove the evidence scan expression matches it.
seed="${TMPDIR:-/tmp}/i2pr-stream-fingerprint-check-$$"
trap 'rm -f "$seed"' EXIT
printf '%s\n' 'payload_sha256=seeded-forbidden-marker' >"$seed"
if ! grep -Ein 'payload_bytes|payload_sha256|destination_hash|peer_hash|peer_address|private_key|private_identity' "$seed" >/dev/null; then
  echo "Streaming fingerprint evidence checker failed its seeded negative self-check" >&2
  exit 1
fi

grep -Fq 'MAX_FINGERPRINT_EVENTS: usize = 4096' "${source}"
echo "Streaming fingerprint evidence guard passed (reference captures remain separately gated)"
