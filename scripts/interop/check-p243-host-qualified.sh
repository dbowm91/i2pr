#!/usr/bin/env bash
# Plan 243 §4 — host qualification gate for the M6 Java Streaming hosted
# stock-client-build qualification lane.
#
# The Plan 242 lane is registered-ready and source-locked, but its
# closure host lacked the required Java reference cache + i2pr daemon
# pair. Plan 243 must prove, before any counted attempt consumes the
# three-attempt budget, that the execution host carries every
# artifact the frozen Plan 242 lane depends on.
#
# Output: sanitized booleans / versions / paths / counts. The script
# is fail-closed: every missing artifact emits exactly one bounded
# reason token (`P243-H-HOST-NOT-QUALIFIED reason=<bounded-reason>`)
# and exits 70 (host-not-qualified). The script never consumes a
# counted interop attempt, never starts a router, never edits
# production code.
#
# Bounded reasons:
#   java-runtime-missing
#   javac-missing
#   java-reference-cache-missing
#   i2pr-daemon-missing
#   source-lock-input-missing
#   port-preflight-failed
#   workspace-sha-mismatch
#   filesystem-preflight-failed
#
# Usage:
#   bash scripts/interop/check-p243-host-qualified.sh [--expected-sha <40-char hex>]
#                                                     [--evidence-dir <path>]
#
# The expected SHA defaults to the current HEAD. A pinned SHA may be
# supplied to verify a frozen implementation SHA, exactly the way
# the Plan 242 §13 / Plan 243 §6 attempt discipline demands.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
JAVA_REPO="https://github.com/i2p/i2p.i2p.git"
JAVA_CACHE_DIR="${REPO_ROOT}/target/interop/cache/m6-java/${JAVA_PIN}"
JAVA_SOURCE_ROOT_DEFAULT="${REPO_ROOT}/target/interop/m6-java-sources/i2p.i2p-${JAVA_PIN}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"

EVIDENCE_DIR_DEFAULT="${REPO_ROOT}/target/interop/m6-java-evidence/p243-host-qualification"
EVIDENCE_DIR="${I2PR_P243_EVIDENCE_DIR:-${EVIDENCE_DIR_DEFAULT}}"
EXPECTED_SHA=""
while (($#)); do
  case "$1" in
    --expected-sha)
      (($# >= 2)) || { echo "--expected-sha requires a 40-char hex" >&2; exit 64; }
      EXPECTED_SHA="$2"
      shift 2
      ;;
    --evidence-dir)
      (($# >= 2)) || { echo "--evidence-dir requires a path" >&2; exit 64; }
      EVIDENCE_DIR="$2"
      shift 2
      ;;
    -h|--help)
      echo "usage: $0 [--expected-sha <40-char hex>] [--evidence-dir <path>]" >&2
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 64
      ;;
  esac
done

if [[ -n "${EXPECTED_SHA}" ]] && ! [[ "${EXPECTED_SHA}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "expected-sha must be a 40-char lowercase hex (got '${EXPECTED_SHA}')" >&2
  exit 64
fi

mkdir -p "${EVIDENCE_DIR}"
EVIDENCE_FILE="${EVIDENCE_DIR}/host-qualification.tsv"
: > "${EVIDENCE_FILE}"

emit() {
  local key="$1"
  local value="$2"
  printf '%s\t%s\n' "${key}" "${value}" >> "${EVIDENCE_FILE}"
}

emit_failure() {
  local reason="$1"
  local detail="$2"
  emit "host_qualified" "false"
  emit "not_qualified_reason" "${reason}"
  emit "not_qualified_detail" "${detail}"
  echo "P243-H-HOST-NOT-QUALIFIED reason=${reason}" >&2
  echo "    detail: ${detail}" >&2
  exit 70
}

# ---- 1. workspace SHA ------------------------------------------------------
CURRENT_SHA="$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || true)"
if [[ -z "${CURRENT_SHA}" ]]; then
  emit_failure "workspace-sha-mismatch" "git rev-parse failed"
fi
emit "workspace_sha" "${CURRENT_SHA}"
if [[ -n "${EXPECTED_SHA}" ]]; then
  emit "workspace_sha_expected" "${EXPECTED_SHA}"
  if [[ "${CURRENT_SHA}" != "${EXPECTED_SHA}" ]]; then
    emit_failure "workspace-sha-mismatch" "got ${CURRENT_SHA}, expected ${EXPECTED_SHA}"
  fi
fi

# ---- 2. cargo / Rust toolchain ---------------------------------------------
if ! command -v cargo >/dev/null 2>&1; then
  emit_failure "filesystem-preflight-failed" "cargo binary missing"
fi
CARGO_VERSION="$(cargo --version 2>/dev/null || true)"
[[ -n "${CARGO_VERSION}" ]] || emit_failure "filesystem-preflight-failed" "cargo --version returned empty"
emit "cargo_version" "${CARGO_VERSION}"

if ! command -v rustc >/dev/null 2>&1; then
  emit_failure "filesystem-preflight-failed" "rustc binary missing"
fi
RUSTC_VERSION="$(rustc --version 2>/dev/null || true)"
[[ -n "${RUSTC_VERSION}" ]] || emit_failure "filesystem-preflight-failed" "rustc --version returned empty"
emit "rustc_version" "${RUSTC_VERSION}"

if ! command -v python3 >/dev/null 2>&1; then
  emit_failure "filesystem-preflight-failed" "python3 binary missing"
fi
PYTHON_VERSION="$(python3 --version 2>/dev/null || true)"
emit "python_version" "${PYTHON_VERSION}"

# ---- 3. Java runtime -------------------------------------------------------
if ! command -v java >/dev/null 2>&1; then
  emit_failure "java-runtime-missing" "java binary not on PATH"
fi
JAVA_VERSION_LINE="$(java -version 2>&1 | head -n 1 || true)"
[[ -n "${JAVA_VERSION_LINE}" ]] || emit_failure "java-runtime-missing" "java -version returned empty"
emit "java_runtime" "${JAVA_VERSION_LINE}"

# ---- 4. javac --------------------------------------------------------------
if ! command -v javac >/dev/null 2>&1; then
  emit_failure "javac-missing" "javac binary not on PATH"
fi
JAVAC_VERSION_LINE="$(javac -version 2>&1 | head -n 1 || true)"
[[ -n "${JAVAC_VERSION_LINE}" ]] || emit_failure "javac-missing" "javac -version returned empty"
emit "javac_version" "${JAVAC_VERSION_LINE}"

# ---- 5. exact Java reference cache ----------------------------------------
if [[ ! -d "${JAVA_CACHE_DIR}/lib" ]]; then
  emit_failure "java-reference-cache-missing" "missing ${JAVA_CACHE_DIR}/lib"
fi
emit "java_cache_dir" "${JAVA_CACHE_DIR}"
if [[ ! -f "${JAVA_CACHE_DIR}/source-revision.txt" ]]; then
  emit_failure "java-reference-cache-missing" "missing source-revision.txt"
fi
SOURCE_REVISION="$(<"${JAVA_CACHE_DIR}/source-revision.txt")"
emit "java_cache_source_revision" "${SOURCE_REVISION}"
if [[ "${SOURCE_REVISION}" != "${JAVA_PIN}" ]]; then
  emit_failure "java-reference-cache-missing" \
    "source-revision mismatch (got ${SOURCE_REVISION}, want ${JAVA_PIN})"
fi
if [[ -f "${JAVA_CACHE_DIR}/source-repository.txt" ]]; then
  SOURCE_REPO="$(<"${JAVA_CACHE_DIR}/source-repository.txt")"
  emit "java_cache_source_repository" "${SOURCE_REPO}"
  if [[ "${SOURCE_REPO}" != "${JAVA_REPO}" && "${SOURCE_REPO}" != "${JAVA_REPO%.git}" ]]; then
    emit_failure "java-reference-cache-missing" \
      "source-repository mismatch (got ${SOURCE_REPO}, want ${JAVA_REPO})"
  fi
fi
if [[ -f "${JAVA_CACHE_DIR}/source-version.txt" ]]; then
  SOURCE_VERSION="$(<"${JAVA_CACHE_DIR}/source-version.txt")"
  emit "java_cache_source_version" "${SOURCE_VERSION}"
  if [[ "${SOURCE_VERSION}" != "${JAVA_VERSION}" ]]; then
    emit_failure "java-reference-cache-missing" \
      "source-version mismatch (got ${SOURCE_VERSION}, want ${JAVA_VERSION})"
  fi
fi
JAVA_LIB_JAR_COUNT="$(find "${JAVA_CACHE_DIR}/lib" -maxdepth 1 -name '*.jar' | wc -l | tr -d ' ')"
emit "java_cache_lib_jar_count" "${JAVA_LIB_JAR_COUNT}"
if [[ "${JAVA_LIB_JAR_COUNT}" -lt 10 ]]; then
  emit_failure "java-reference-cache-missing" \
    "lib/ jar count too low (${JAVA_LIB_JAR_COUNT})"
fi

# ---- 6. exact-pinned Java source checkout ---------------------------------
JAVA_SOURCE_ROOT="${I2PR_M6_JAVA_SOURCE_ROOT:-${JAVA_SOURCE_ROOT_DEFAULT}}"
emit "java_source_root" "${JAVA_SOURCE_ROOT}"
if [[ ! -d "${JAVA_SOURCE_ROOT}/.git" ]]; then
  emit_failure "source-lock-input-missing" "Java source checkout missing: ${JAVA_SOURCE_ROOT}"
fi
JAVA_HEAD_SHA="$(git -C "${JAVA_SOURCE_ROOT}" rev-parse HEAD 2>/dev/null || true)"
emit "java_source_head_sha" "${JAVA_HEAD_SHA:-unknown}"
if [[ "${JAVA_HEAD_SHA}" != "${JAVA_PIN}" ]]; then
  emit_failure "source-lock-input-missing" \
    "Java HEAD is not pinned (got ${JAVA_HEAD_SHA:-unknown}, want ${JAVA_PIN})"
fi
# Required selector source files for the Plan 242 source-lock checker.
for src_file in \
  "${JAVA_SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/pool/TunnelPeerSelector.java" \
  "${JAVA_SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/pool/ClientPeerSelector.java" \
  "${JAVA_SOURCE_ROOT}/apps/streaming/java/src/net/i2p/client/streaming/impl/Connection.java" \
  "${JAVA_SOURCE_ROOT}/router/java/src/net/i2p/router/networkdb/kademlia/IterativeSearchJob.java"; do
  if [[ ! -f "${src_file}" ]]; then
    emit_failure "source-lock-input-missing" "missing source file ${src_file}"
  fi
done
emit "java_source_lock_inputs" "ok"

# ---- 7. i2pr-daemon binary -------------------------------------------------
DAEMON_BIN_DEFAULT="${REPO_ROOT}/target/debug/i2pr"
DAEMON_BIN="${I2PR_DAEMON_BIN:-${DAEMON_BIN_DEFAULT}}"
emit "i2pr_daemon_path" "${DAEMON_BIN}"
if [[ ! -x "${DAEMON_BIN}" ]]; then
  emit "i2pr_daemon_present" "false"
  if [[ -z "${I2PR_DAEMON_BIN:-}" ]]; then
    echo "==> building i2pr-daemon (target/debug/i2pr not present)" >&2
    if ! cargo build --locked -p i2pr-daemon >"${EVIDENCE_DIR}/daemon-build.log" 2>&1; then
      tail -n 40 "${EVIDENCE_DIR}/daemon-build.log" >&2 || true
      emit_failure "i2pr-daemon-missing" "cargo build failed (see daemon-build.log)"
    fi
  else
    emit_failure "i2pr-daemon-missing" "binary not executable at ${DAEMON_BIN}"
  fi
fi
if [[ ! -x "${DAEMON_BIN}" ]]; then
  emit_failure "i2pr-daemon-missing" "binary still missing after build: ${DAEMON_BIN}"
fi
emit "i2pr_daemon_present" "true"

# ---- 8. loopback port preflight --------------------------------------------
# The run-java.sh reserves fresh loopback ports for SAM, I2CP, the three
# J219 diagnostic ports, and the i2pr SSU2 bind. Pre-flight a small set so
# the OS-side capacity is provable before any counted attempt.
if ! python3 - >> "${EVIDENCE_FILE}" 2>"${EVIDENCE_DIR}/port-preflight.log" <<'PY'
import socket
needed = 12
bound = []
socks = []
try:
    for _ in range(needed):
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.bind(("127.0.0.1", 0))
        socks.append(s)
        bound.append(s.getsockname()[1])
except OSError as exc:
    print(f"port-preflight-failed\tbound={bound} remaining={needed - len(bound)} err={exc}", file=sys.stderr)
    raise SystemExit(1)
finally:
    for s in socks:
        try:
            s.close()
        except OSError:
            pass
print(f"port-preflight\tloopback_tcp_capacity={len(bound)} needed={needed} sample={','.join(str(p) for p in bound)}")
PY
then
  echo "P243-H-HOST-NOT-QUALIFIED reason=port-preflight-failed detail: $(head -n1 "${EVIDENCE_DIR}/port-preflight.log" 2>/dev/null || echo no-detail)" >&2
  cat "${EVIDENCE_DIR}/port-preflight.log" >&2 || true
  exit 70
fi

emit "host_qualified" "true"
emit "qualification_complete" "ok"
echo "P243-H-HOST-QUALIFIED reason=ok" >&2
echo "    evidence: ${EVIDENCE_FILE}" >&2
