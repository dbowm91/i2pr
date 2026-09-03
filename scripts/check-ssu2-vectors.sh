#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
corpus="$root/tests/fixtures/ssu2"
manifest="$corpus/manifest.tsv"

test -f "$manifest"
declare -A seen_ids=()
declare -A seen_paths=()
listed_paths=()

while IFS=$'\t' read -r id path category provenance expected_hash extra; do
    [[ -z "${id:-}" || "$id" == \#* ]] && continue
    if [[ -n "${extra:-}" || -z "${path:-}" || -z "${category:-}" || -z "${provenance:-}" || -z "${expected_hash:-}" ]]; then
        echo "SSU2 fixture manifest row is malformed: $id" >&2
        exit 1
    fi
    [[ -z "${seen_ids[$id]+x}" ]] || { echo "SSU2 fixture ID repeated: $id" >&2; exit 1; }
    [[ -z "${seen_paths[$path]+x}" ]] || { echo "SSU2 fixture path repeated: $path" >&2; exit 1; }
    seen_ids[$id]=1
    seen_paths[$path]=1
    [[ "$category" == positive || "$category" == malformed ]] || {
        echo "SSU2 fixture category invalid: $id" >&2
        exit 1
    }
    [[ "$expected_hash" =~ ^[0-9a-f]{64}$ ]] || {
        echo "SSU2 fixture hash invalid: $id" >&2
        exit 1
    }
    [[ "$path" == tests/fixtures/ssu2/* ]] || {
        echo "SSU2 fixture escapes corpus: $id" >&2
        exit 1
    }
    test -f "$root/$path" || { echo "SSU2 fixture missing: $id" >&2; exit 1; }
    actual_hash=$(sha256sum "$root/$path" | awk '{print $1}')
    [[ "$actual_hash" == "$expected_hash" ]] || {
        echo "SSU2 fixture hash mismatch: $id" >&2
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
    $found || { echo "SSU2 fixture is not listed: $relative" >&2; exit 1; }
done < <(find "$corpus" -type f ! -name README.md ! -name manifest.tsv -print | sort)

required=(
    long-header
    short-header-data
    short-header-confirmed
    blocks-positive
    blocks-malformed
)
for id in "${required[@]}"; do
    if ! grep -Eq "^${id}[[:space:]]" "$manifest"; then
        echo "SSU2 fixture row missing: $id" >&2
        exit 1
    fi
done

echo "SSU2 vector manifest is complete and hashes match."
