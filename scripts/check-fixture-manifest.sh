#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$root/tests/fixtures/i2np/manifest.tsv"
status=0

while IFS='|' read -r id path expected _source _revision _generator _seed _outcome _license; do
    [[ -z "$id" || "$id" == \#* ]] && continue
    actual=$(sha256sum "$root/$path" | awk '{print $1}')
    if [[ "$expected" == REPLACE_* || "$actual" != "$expected" ]]; then
        printf 'fixture hash mismatch: %s (%s)\n' "$id" "$path" >&2
        status=1
    fi
done < "$manifest"

exit "$status"
