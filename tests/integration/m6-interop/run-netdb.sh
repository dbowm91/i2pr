#!/usr/bin/env bash
# Plan 186 — run the NetDB lookup/publication lane end-to-end.
#
# Local rows execute the daemon-owned NetDB-over-tunnels coordinator
# suites (unit + live two-role trajectory + liveness). The external row
# provisions one ephemeral exact-pinned i2pd 2.61.0 process on loopback
# with transit + floodfill enabled (the only Plan 185 settings that
# change for Plan 186) and runs the single fail-closed driver through
# its explicit `--ignored --exact` selection.
#
# The lane is unprivileged and loopback-only. Required failures
# make this script fail. Sanitized evidence defaults below
# target/interop; set I2PR_M6_NETDB_EVIDENCE_DIR to retain it
# elsewhere. Private router keys stay in the ephemeral scratch
# directory and are never copied to evidence.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M6_NETDB_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m6-netdb-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
I2PD_PORT="${I2PR_I2PD_NETDB_PORT:-43885}"
I2PR_PORT="${I2PR_SSU2_NETDB_PORT:-44083}"
DRIVER_TIMEOUT="600s"

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m6-plan186.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"

# ---- i2pd cache verification (fail closed before any network use) --------
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified Plan 186 source revision" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if "${I2PD_BIN}" --version 2>&1 | grep -Fq "${I2PD_VERSION}"; then
  echo "==> i2pd reference: ${I2PD_VERSION} (${I2PD_PIN})"
else
  echo "i2pd binary does not report ${I2PD_VERSION}" >&2
  exit 1
fi

# ---- ephemeral i2pd provisioning (fresh datadir: no stale netDb) ---------
# Plan 186 differs from Plan 185 in `floodfill = true` so the reference
# acts as the controlled floodfill for the private dataset. Transit
# stays enabled so one-hop exploratory builds are accepted. All other
# settings stay identical to the Plan 184 preflight.
I2PD_HOME="${SCRATCH}/i2pd"
I2PD_DATA="${I2PD_HOME}/data"
I2PD_LOG="${EVIDENCE_DIR}/i2pd.log"
mkdir -p "${I2PD_DATA}"
cat > "${I2PD_HOME}/i2pd.conf" <<EOF
daemon = false
loglevel = debug
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = ${I2PD_PORT}
ipv4 = true
ipv6 = false
nat = false
notransit = false
floodfill = true
reservedrange = false
bandwidth = L
[ssu2]
enabled = true
published = true
port = ${I2PD_PORT}
[ntcp2]
enabled = false
published = false
[http]
enabled = false
[httpproxy]
enabled = false
[socksproxy]
enabled = false
[sam]
enabled = false
[i2cp]
enabled = false
[i2pcontrol]
enabled = false
[upnp]
enabled = false
[reseed]
verify = true
urls =
threshold = 0
EOF
: > "${I2PD_LOG}"
setsid "${I2PD_BIN}" "--conf=${I2PD_HOME}/i2pd.conf" "--datadir=${I2PD_DATA}" \
  --log=file "--logfile=${I2PD_LOG}" >/dev/null 2>&1 < /dev/null &
I2PD_PID=$!
CHILD_PIDS=("${I2PD_PID}")

stop_group() {
  local pid="${1:-}"
  [[ -z "${pid}" ]] && return 0
  kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
}

cleanup() {
  local pid
  for pid in "${CHILD_PIDS[@]:-}"; do
    stop_group "${pid}"
  done
  for pid in "${CHILD_PIDS[@]:-}"; do
    wait "${pid}" 2>/dev/null || true
  done
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

echo "==> waiting for ephemeral i2pd on 127.0.0.1:${I2PD_PORT}"
I2PD_RI=""
for _ in $(seq 1 120); do
  if [[ -f "${I2PD_DATA}/router.info" ]] &&
     grep -Fq "Start listening on 127.0.0.1:${I2PD_PORT}" "${I2PD_LOG}" 2>/dev/null; then
    I2PD_RI="${I2PD_DATA}/router.info"
    break
  fi
  if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
    echo "ephemeral i2pd exited during startup" >&2
    sed -n '1,40p' "${I2PD_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ -z "${I2PD_RI}" ]]; then
  echo "ephemeral i2pd did not publish router.info / SSU2 listener" >&2
  sed -n '1,40p' "${I2PD_LOG}" >&2 || true
  exit 2
fi
echo "    i2pd: 127.0.0.1:${I2PD_PORT} ($(wc -c <"${I2PD_RI}")-byte router.info)"

REQUIRED_FAILED=0
record() {
  local label="$1"
  local status="$2"
  local detail="${3:-}"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  printf '%s\t%s\t%s\n' "${label}" "${status}" "${detail}" >> "${RESULTS_FILE}"
  [[ "${status}" == "passed" ]] || REQUIRED_FAILED=1
}

record_guarded() {
  local label="$1"
  local detail="$2"
  local rc="$3"
  if [[ "${rc}" -eq 0 ]]; then
    record "${label}" passed "${detail}"
  else
    record "${label}" failed "${detail} (exit ${rc})"
  fi
}

echo "==> local Plan 186 rows (coordinator + live trajectory suites)"
UNIT_LOG="${EVIDENCE_DIR}/local-netdb-tunnel-unit.log"
: > "${UNIT_LOG}"
unit_rc=0
cargo test --locked -p i2pr-daemon --test netdb_tunnel_unit -- \
  --test-threads=1 >>"${UNIT_LOG}" 2>&1 || unit_rc=$?
record_guarded "local-netdb-tunnel-unit" \
  "authoritative store + bounded matrix rows (cargo test -p i2pr-daemon --test netdb_tunnel_unit)" \
  "${unit_rc}"

LIVE_LOG="${EVIDENCE_DIR}/local-netdb-tunnel-live.log"
: > "${LIVE_LOG}"
live_rc=0
cargo test --locked -p i2pr-daemon --test netdb_tunnel_live -- \
  --test-threads=1 >>"${LIVE_LOG}" 2>&1 || live_rc=$?
record_guarded "local-netdb-tunnel-live" \
  "outbound/inbound TunnelData trajectory + publication rows (cargo test -p i2pr-daemon --test netdb_tunnel_live)" \
  "${live_rc}"

LIVENESS_LOG="${EVIDENCE_DIR}/local-tunnel-liveness.log"
: > "${LIVENESS_LOG}"
liveness_rc=0
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- \
  --test-threads=1 >>"${LIVENESS_LOG}" 2>&1 || liveness_rc=$?
record_guarded "local-tunnel-liveness" \
  "liveness scheduler unit rows (cargo test -p i2pr-daemon --lib tunnel_liveness)" \
  "${liveness_rc}"

echo "==> external NetDB lane against exact-pinned i2pd (explicit ignored selection)"
DRIVER_EVIDENCE="${EVIDENCE_DIR}/driver"
mkdir -p "${DRIVER_EVIDENCE}"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0
if I2PD_ROUTER_INFO="${I2PD_RI}" \
   I2PD_SSU2_ENDPOINT="127.0.0.1:${I2PD_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   EVIDENCE_DIR="${DRIVER_EVIDENCE}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test netdb_tunnel_external \
   netdb_tunnels_against_i2pd -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${DRIVER_EVIDENCE}/driver-evidence.tsv"
m6_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  if [[ "${driver_rc}" -eq 0 && -f "${DRIVER_TSV}" ]] &&
     grep -Fq "${key}" "${DRIVER_TSV}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}
m6_row "external-daemon-strict-profile" "daemon-strict-profile" \
  "daemon starts with strict SSU2 controlled profile (explicit --ignored --exact driver)"
m6_row "external-reference-verified" "reference-routerinfo-verified" \
  "exact-pinned RouterInfo parsed/verified through the documented file path"
m6_row "external-reference-floodfill" "reference-floodfill-capable" \
  "reference RouterInfo advertises floodfill and bootstraps the authoritative store"
m6_row "external-session-established" "session-established" \
  "authenticated SSU2 session establishes via daemon-owned runtime"
m6_row "external-outbound-lookup-tunnel" "outbound-lookup-via-tunnel" \
  "RouterInfo DatabaseLookup traverses outbound TunnelData (first-hop + floodfill proof)"
m6_row "external-publication-tunnel" "publication-via-tunnel" \
  "RouterInfo publication traverses outbound TunnelData with protocol-derived ack path"
m6_row "external-direct-rejected" "direct-rejected" \
  "direct SSU2 NetDB delivery is rejected as a counted path"
m6_row "external-liveness-first-test" "liveness-first-test" \
  "creator-side liveness scheduler first test succeeds during NetDB activity"

echo "==> workspace gates slice"
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-fixture-manifest \
           check-ntcp2-vectors check-ssu2-vectors check-ntcp2-interoperability \
           check-constrained-host-lane-boundary check-sam-acceptance-evidence \
           check-ssu2-acceptance-evidence; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
record_guarded "workspace-gates" \
  "fmt + workspace check --all-targets + static boundary scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${gates_rc}"

python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PD_PIN}" "${I2PD_VERSION}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root = sys.argv[1:4]
i2pd_pin, i2pd_version = sys.argv[4:6]
rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, status, detail = line.rstrip("\n").split("\t", 2)
        rows.append({"label": label, "status": status, "detail": detail})

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
driver_keys = []
driver_tsv = Path(evidence_dir) / "driver" / "driver-evidence.tsv"
if driver_tsv.exists():
    for line in driver_tsv.read_text(encoding="utf-8").splitlines():
        label = line.split("\t", 1)[0]
        driver_keys.append(label)
evidence = {
    "schema": "i2pr-m6-netdb-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m6-netdb-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "role": "mandatory independent NetDB floodfill reference, unmodified",
        "transit": "notransit=false, floodfill=true (Plan 186 only)",
    },
    "driver_evidence_keys": driver_keys,
    "results": rows,
    "m6_netdb": "passed-via-i2pd-2.61.0" if all(
        row["status"] == "passed" for row in rows
    ) else "failed",
    "known_limitations": [
        "one-hop exploratory tunnels only; no multi-hop tunnel build",
        "loopback-only i2pd reference; no public I2P participation",
        "full decryptable inbound round-trip proven in local two-role suite; external proves session + build acceptance + tunnel composition/admission (OTBRM unwrap limitation retained from Plan 185)",
        "no destination LeaseSet2 or Streaming claim; Plan 187 owns destination routing",
        "no per-tunnel task/timer design; one central scheduler owns every attempt",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 186 NetDB evidence (local suites + independent i2pd matrix)\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n")
    stream.write("- i2pd profile: `notransit=false`, `floodfill=true` (Plan 186 only)\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 186 NetDB lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 186 NetDB lane passed; sanitized evidence: ${EVIDENCE_DIR}"
