#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
corpus="$root/tests/fixtures/i2cp"
manifest="$corpus/manifest.tsv"

test -f "$manifest"
declare -A seen_ids=()
declare -A seen_paths=()
listed_paths=()

while IFS=$'\t' read -r id path category provenance expected_hash extra; do
    [[ -z "${id:-}" || "$id" == \#* ]] && continue
    if [[ -n "${extra:-}" || -z "${path:-}" || -z "${category:-}" || -z "${provenance:-}" || -z "${expected_hash:-}" ]]; then
        echo "I2CP fixture manifest row is malformed: $id" >&2
        exit 1
    fi
    [[ -z "${seen_ids[$id]+x}" ]] || { echo "I2CP fixture ID repeated: $id" >&2; exit 1; }
    [[ -z "${seen_paths[$path]+x}" ]] || { echo "I2CP fixture path repeated: $path" >&2; exit 1; }
    seen_ids[$id]=1
    seen_paths[$path]=1
    [[ "$category" == positive || "$category" == malformed ]] || {
        echo "I2CP fixture category invalid: $id" >&2
        exit 1
    }
    [[ "$expected_hash" =~ ^[0-9a-f]{64}$ ]] || {
        echo "I2CP fixture hash invalid: $id" >&2
        exit 1
    }
    [[ "$path" == tests/fixtures/i2cp/* ]] || {
        echo "I2CP fixture escapes corpus: $id" >&2
        exit 1
    }
    test -f "$root/$path" || { echo "I2CP fixture missing: $id" >&2; exit 1; }
    actual_hash=$(sha256sum "$root/$path" | awk '{print $1}')
    [[ "$actual_hash" == "$expected_hash" ]] || {
        echo "I2CP fixture hash mismatch: $id" >&2
        exit 1
    }
    listed_paths+=("$path")
done < "$manifest"

while IFS= read -r fixture; do
    relative="${fixture#"$root/"}"
    found=false
    for listed in "${listed_paths[@]}"; do
        [[ "$listed" == "$relative" ]] && found=true
    done
    $found || { echo "I2CP fixture is not listed: $relative" >&2; exit 1; }
done < <(find "$corpus" -type f ! -name README.md ! -name manifest.tsv -print | sort)

required=(
    protocol-byte
    get-date
    set-date
    create-session
    session-status-created
    request-variable-leaseset
    create-leaseset2
    send-message
    send-message-expires
    message-payload
    message-status-accepted
    get-bandwidth-limits
    bandwidth-limits
    dest-lookup
    dest-reply-destination
    dest-reply-hash
    disconnect
    host-lookup-hostname
    host-reply-success
    malformed-oversize-length
    malformed-truncated-body
    malformed-unknown-type
)
for id in "${required[@]}"; do
    if ! grep -Eq "^${id}[[:space:]]" "$manifest"; then
        echo "I2CP fixture row missing: $id" >&2
        exit 1
    fi
done

cargo test --locked -p i2pr-api --test i2cp_vectors

echo "I2CP vector manifest is complete and hashes match."
