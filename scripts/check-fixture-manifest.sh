#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$root/tests/fixtures/i2np/manifest.tsv"
status=0
declare -A seen_ids=()
declare -A seen_paths=()

while IFS='|' read -r id path classification expected _source _revision _generator _input _outcome _license independence; do
    [[ -z "$id" || "$id" == \#* ]] && continue
    if [[ -n "${seen_ids[$id]+present}" ]]; then
        printf 'fixture id repeated: %s\n' "$id" >&2
        status=1
    fi
    if [[ -n "${seen_paths[$path]+present}" ]]; then
        printf 'fixture path repeated: %s\n' "$path" >&2
        status=1
    fi
    seen_ids["$id"]=1
    seen_paths["$path"]=1
    if [[ "$classification" != positive && "$classification" != negative ]]; then
        printf 'fixture classification invalid: %s (%s)\n' "$id" "$classification" >&2
        status=1
    fi
    if [[ -z "$path" || -z "$expected" || -z "$_source" || -z "$_revision" || -z "$_generator" || -z "$_input" || -z "$_outcome" || -z "$_license" ]]; then
        printf 'fixture metadata incomplete: %s\n' "$id" >&2
        status=1
    fi
    if [[ "$independence" != locally-authored && "$independence" != independently-produced ]]; then
        printf 'fixture provenance invalid: %s (%s)\n' "$id" "$independence" >&2
        status=1
    fi
    if [[ ! -f "$root/$path" ]]; then
        printf 'fixture missing: %s (%s)\n' "$id" "$path" >&2
        status=1
        continue
    fi
    actual=$(sha256sum "$root/$path" | awk '{print $1}')
    if [[ "$expected" == REPLACE_* || "$actual" != "$expected" ]]; then
        printf 'fixture hash mismatch: %s (%s)\n' "$id" "$path" >&2
        status=1
    fi
done < "$manifest"

while IFS= read -r fixture; do
    relative="${fixture#"$root"/}"
    if [[ -z "${seen_paths[$relative]+present}" ]]; then
        printf 'fixture is not listed in manifest: %s\n' "$relative" >&2
        status=1
    fi
done < <(find "$root/tests/fixtures/i2np" -type f -name '*.hex' -print | sort)

exit "$status"
