#!/usr/bin/env bash
set -euo pipefail

INTEROP_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
REPO_ROOT=$(cd "$INTEROP_ROOT/../.." && pwd)
LOCK_MANIFEST="$REPO_ROOT/tests/integration/ntcp2/references.lock.toml"
INTEROP_TARGET="$REPO_ROOT/target/interop"
BUILD_ROOT="$INTEROP_TARGET/build"
CACHE_ROOT="$INTEROP_TARGET/cache"
RUNS_ROOT="$INTEROP_TARGET/runs"
LOCK_SHA256="$(sha256sum "$LOCK_MANIFEST" | awk '{print $1}')"
HOST_CONTRACT="ubuntu-24.04-amd64"

JAVA_REFERENCE="java_i2p"
I2PD_REFERENCE="i2pd"
JAVA_REVISION="2800040deee9bb376567b671ef2e9c34cf3e30b6"
I2PD_REVISION="f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
JAVA_REPOSITORY="https://github.com/i2p/i2p.i2p.git"
I2PD_REPOSITORY="https://github.com/PurpleI2P/i2pd.git"
IZPACK_URL="https://repo1.maven.org/maven2/org/codehaus/izpack/izpack-dist/5.2.4/izpack-dist-5.2.4-installer.jar"
IZPACK_SHA256="a3f2c85afea82e04ebca5ebb1b9b5c95ea770c4d35a7635de312370e14a44d43"

die() {
  printf 'interop error: %s\n' "$*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || die "required file is missing: $1"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command is missing: $1"
}

root_run() {
  if [[ "${EUID}" -eq 0 ]]; then
    "$@"
  else
    require_command sudo
    sudo -n "$@"
  fi
}

ensure_target_dirs() {
  install -d -m 0755 "$INTEROP_TARGET" "$BUILD_ROOT" "$CACHE_ROOT" "$RUNS_ROOT" \
    "$INTEROP_TARGET/evidence"
}

assert_lock_contract() {
  require_file "$LOCK_MANIFEST"
  grep -Fq 'schema = 2' "$LOCK_MANIFEST" \
    || die "reference lock schema drifted"
  grep -Fq "source_revision = \"$JAVA_REVISION\"" "$LOCK_MANIFEST" \
    || die "Java I2P lock revision drifted"
  grep -Fq "source_revision = \"$I2PD_REVISION\"" "$LOCK_MANIFEST" \
    || die "i2pd lock revision drifted"
  grep -Fq "sha256 = \"$IZPACK_SHA256\"" "$LOCK_MANIFEST" \
    || die "IzPack lock hash drifted"
}

cache_key_for() {
  local reference=$1
  local revision=$2
  local command_version=$3
  validate_full_revision "$revision"
  printf '%s\n' "$reference" "$revision" "$command_version" "$LOCK_SHA256" "$HOST_CONTRACT" \
    | sha256sum | awk '{print $1}'
}

validate_full_revision() {
  [[ "$1" =~ ^[0-9a-f]{40}$ ]] || die "source revision is not a full Git object ID: $1"
}

hash_tree() {
  local directory=$1
  [[ -d "$directory" ]] || die "cannot hash missing tree: $directory"
  (
    cd "$directory"
    find . -type f ! -path './build-metadata.txt' -printf '%P\0' | sort -z | while IFS= read -r -d '' path; do
      sha256sum "$path"
    done
  ) | sha256sum | awk '{print $1}'
}

write_metadata_header() {
  local output=$1
  local reference=$2
  local revision=$3
  local command_version=$4
  install -d -m 0700 "$(dirname "$output")"
  {
    printf 'schema=2\n'
    printf 'reference=%s\n' "$reference"
    printf 'source_revision=%s\n' "$revision"
    printf 'lock_sha256=%s\n' "$LOCK_SHA256"
    printf 'build_command_version=%s\n' "$command_version"
    printf 'host_contract=%s\n' "$HOST_CONTRACT"
  } >"$output"
}

verify_git_revision() {
  local source=$1
  local revision=$2
  validate_full_revision "$revision"
  [[ -d "$source/.git" ]] || die "source is not a git checkout: $source"
  [[ -z "$(git -C "$source" status --porcelain)" ]] \
    || die "source checkout is dirty: $source"
  [[ "$(git -C "$source" rev-parse HEAD)" == "$revision" ]] \
    || die "source checkout is not pinned to $revision: $source"
}

verify_git_remote() {
  local source=$1
  local repository=$2
  [[ "$(git -C "$source" remote get-url origin 2>/dev/null || true)" == "$repository" ]] \
    || die "source checkout remote does not match lock: $source"
}

metadata_value() {
  local metadata=$1
  local key=$2
  sed -n "s/^${key}=//p" "$metadata" | head -n 1
}

validate_cache_metadata() {
  local metadata=$1
  local expected_reference=$2
  local cache_dir
  cache_dir=$(cd "$(dirname "$metadata")" && pwd)
  require_file "$metadata"
  require_command python3
  python3 "$REPO_ROOT/tests/integration/ntcp2/harness/metadata.py" \
    --metadata "$metadata" --cache-root "$cache_dir" --reference "$expected_reference" \
    --lock-sha256 "$LOCK_SHA256" --host-contract "$HOST_CONTRACT" >/dev/null \
    || die "invalid cache metadata: $metadata"
}

verify_sha256() {
  local file=$1
  local expected=$2
  [[ -f "$file" ]] || die "hash input is missing: $file"
  local actual
  actual=$(sha256sum "$file" | awk '{print $1}')
  [[ "$actual" == "$expected" ]] || die "sha256 mismatch for $(basename "$file")"
}
