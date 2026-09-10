#!/usr/bin/env bash
# Plan 181 §4.2/§7 — fetch and verify the exact-pinned independent
# IRC client source (jaraco/irc) used by the M10 external lane.
#
# Usage:
#   bash scripts/interop/fetch-service-tunnel-clients.sh [--rebuild]
#
# The optional source override is useful on a constrained host:
#   I2PR_JARACO_IRC_SRC=/path/to/irc
#
# Every source directory must be a Git checkout at the exact
# revision below. A mismatch is a hard error. The cache is
# disposable and lives below target/interop; no third-party source
# is committed and no third-party source is ever patched (the
# static checker rejects vendored/patched clients).
#
# curl/nc/python3 are system tools: the lane records their versions
# at runtime (see run-independent.sh) instead of fetching them.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_ROOT="${REPO_ROOT}/target/interop/cache/service-tunnels"
WORK_ROOT="${REPO_ROOT}/target/interop/service-tunnel-sources"
REBUILD="${1:-}"

JARACO_REPO="https://github.com/jaraco/irc.git"
JARACO_PIN="90e10e690da2c7bf60de21be4e36d24c9ffd7474"

if [[ -n "${REBUILD}" && "${REBUILD}" != "--rebuild" ]]; then
  echo "usage: $0 [--rebuild]" >&2
  exit 64
fi

mkdir -p "${CACHE_ROOT}" "${WORK_ROOT}"

command -v git >/dev/null 2>&1 || {
  echo "fetch-service-tunnel-clients.sh: required command missing: git" >&2
  exit 1
}

prepare_source() {
  local name="$1"
  local repository="$2"
  local pin="$3"
  local source_override="$4"
  local default_source="$5"
  local source="${source_override:-${default_source}}"

  if [[ -n "${source_override}" ]]; then
    if [[ ! -d "${source}/.git" ]]; then
      echo "${name} source override is not a Git checkout: ${source}" >&2
      exit 1
    fi
  elif [[ ! -d "${source}/.git" ]]; then
    echo "==> cloning ${name}" >&2
    git clone "${repository}" "${source}"
  elif [[ "${REBUILD}" == "--rebuild" ]]; then
    echo "==> refreshing ${name} source" >&2
    git -C "${source}" fetch --tags --force origin
  fi

  if [[ -z "${source_override}" ]]; then
    # Fresh clones land on the remote default branch, not the pinned
    # revision; detach at the exact pin before verification so a
    # hosted runner with an empty cache cannot run an unpinned tip.
    git -C "${source}" checkout --detach "${pin}"
  fi

  local actual
  actual="$(git -C "${source}" rev-parse HEAD)"
  if [[ "${actual}" != "${pin}" ]]; then
    echo "${name} pin mismatch: expected ${pin}, got ${actual}" >&2
    echo "use --rebuild or provide a correctly pinned source override" >&2
    exit 1
  fi
  # The lane never patches third-party sources: the checkout must be
  # clean apart from untracked build artifacts.
  if [[ -n "$(git -C "${source}" status --porcelain --untracked-files=no)" ]]; then
    echo "${name} checkout has tracked modifications (patching forbidden)" >&2
    git -C "${source}" status --porcelain --untracked-files=no >&2
    exit 1
  fi
  printf '%s\n' "${source}"
}

JARACO_CACHE="${CACHE_ROOT}/jaraco_irc/${JARACO_PIN}"
if [[ -n "${I2PR_JARACO_IRC_SRC:-}" || "${REBUILD}" == "--rebuild" ||
      ! -f "${JARACO_CACHE}/source-revision.txt" ]]; then
  JARACO_SRC="$(prepare_source jaraco_irc "${JARACO_REPO}" "${JARACO_PIN}" \
    "${I2PR_JARACO_IRC_SRC:-}" "${WORK_ROOT}/jaraco-irc-${JARACO_PIN}")"
  mkdir -p "${JARACO_CACHE}"
  printf '%s\n' "${JARACO_PIN}" > "${JARACO_CACHE}/source-revision.txt"
  printf '%s\n' "${JARACO_REPO}" > "${JARACO_CACHE}/source-repository.txt"
  printf '%s\n' "${JARACO_SRC}" > "${JARACO_CACHE}/source-path.txt"
  cat > "${JARACO_CACHE}/build-metadata.txt" <<EOF
schema=1
reference=jaraco_irc
source_revision=${JARACO_PIN}
host_contract=ubuntu-24.04-amd64
source_repository=${JARACO_REPO}
launcher=venv-pip-install-from-verified-source
execution_network=loopback-only
patching=forbidden
EOF
else
  if [[ "$(<"${JARACO_CACHE}/source-revision.txt")" != "${JARACO_PIN}" ]]; then
    echo "jaraco_irc cache has no verified source revision: ${JARACO_CACHE}" >&2
    echo "run scripts/interop/fetch-service-tunnel-clients.sh --rebuild" >&2
    exit 1
  fi
  echo "jaraco_irc cache present at ${JARACO_CACHE}"
fi

cat <<EOF
==> service-tunnel external-client cache ready
    jaraco/irc pin: ${JARACO_PIN}
    cache root:     ${CACHE_ROOT}
EOF
