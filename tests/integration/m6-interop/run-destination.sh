#!/usr/bin/env bash
# Plan 187 — run the destination LeaseSet2 / Garlic message-plane lane.
#
# Local rows execute the daemon-owned destination-over-tunnels
# coordinator suites (unit + live two-role trajectory + liveness).
# The external row provisions one ephemeral exact-pinned i2pd 2.61.0
# process on loopback with transit + floodfill + SAM enabled (the
# only Plan 186 settings that change for Plan 187) and runs the
# single fail-closed driver through its explicit
# `--ignored --exact` selection.
#
# The lane is unprivileged and loopback-only. Required failures
# make this script fail. Sanitized evidence defaults below
# target/interop; set I2PR_M6_DESTINATION_EVIDENCE_DIR to retain it
# elsewhere. Private router keys, destination secrets, and raw
# application payloads stay in the ephemeral scratch directory and
# are never copied to evidence (digests/lengths/counters only).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M6_DESTINATION_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m6-destination-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
I2PD_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
I2PD_SAM_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
I2PR_PORT="${I2PR_SSU2_DESTINATION_PORT:-44084}"
DRIVER_TIMEOUT="600s"

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m6-plan187.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"
# Hygiene: no stale secret-bearing or result-bearing file from a
# previous run may linger in evidence. The raw i2pd log is never
# evidence (it carries SAM session lines); only sanitized counts
# extracted below reach evidence.
rm -f "${EVIDENCE_DIR}/i2pd.log" \
  "${EVIDENCE_DIR}/driver/driver-evidence.tsv" \
  "${EVIDENCE_DIR}/reference-facts.tsv"

# ---- i2pd cache verification (fail closed before any network use) --------
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified Plan 187 source revision" >&2
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
# Plan 187 keeps `notransit = false, floodfill = true` from Plan 186
# so the reference accepts one-hop builds and acts as the controlled
# floodfill, and enables the loopback SAM surface so the harness can
# create the reference DATAGRAM service destination through i2pd's
# public client API. All other settings stay identical to the Plan
# 186 preflight.
I2PD_HOME="${SCRATCH}/i2pd"
I2PD_DATA="${I2PD_HOME}/data"
# The raw i2pd log stays in scratch: it carries SAM session lines
# that may embed reference key material. Only sanitized counts
# extracted below reach evidence.
I2PD_LOG="${SCRATCH}/i2pd.log"
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
enabled = true
address = 127.0.0.1
port = ${I2PD_SAM_PORT}
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

echo "==> waiting for ephemeral i2pd on 127.0.0.1:${I2PD_PORT} (SAM ${I2PD_SAM_PORT})"
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
SAM_READY=0
for _ in $(seq 1 60); do
  if (exec 3<>"/dev/tcp/127.0.0.1/${I2PD_SAM_PORT}") 2>/dev/null; then
    exec 3<&- 3>&- || true
    SAM_READY=1
    break
  fi
  if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
    echo "ephemeral i2pd exited before SAM came up" >&2
    sed -n '1,40p' "${I2PD_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ "${SAM_READY}" -ne 1 ]]; then
  echo "ephemeral i2pd SAM did not listen on 127.0.0.1:${I2PD_SAM_PORT}" >&2
  grep -i "sam bridge" "${I2PD_LOG}" | head -5 >&2 || true
  exit 2
fi
echo "    i2pd SAM: 127.0.0.1:${I2PD_SAM_PORT}"
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

echo "==> local Plan 187 rows (coordinator + live trajectory suites)"
UNIT_LOG="${EVIDENCE_DIR}/local-destination-tunnel-unit.log"
: > "${UNIT_LOG}"
unit_rc=0
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- \
  --test-threads=1 >>"${UNIT_LOG}" 2>&1 || unit_rc=$?
record_guarded "local-destination-tunnel-unit" \
  "authoritative store + LeaseSet2 matrix + material-proof rows (cargo test -p i2pr-daemon --test destination_tunnel_unit)" \
  "${unit_rc}"

LIVE_LOG="${EVIDENCE_DIR}/local-destination-tunnel-live.log"
: > "${LIVE_LOG}"
live_rc=0
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- \
  --test-threads=1 >>"${LIVE_LOG}" 2>&1 || live_rc=$?
record_guarded "local-destination-tunnel-live" \
  "LeaseSet2 lookup + ECIES/Garlic round-trip + publication rows (cargo test -p i2pr-daemon --test destination_tunnel_live)" \
  "${live_rc}"

LIVENESS_LOG="${EVIDENCE_DIR}/local-tunnel-liveness.log"
: > "${LIVENESS_LOG}"
liveness_rc=0
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- \
  --test-threads=1 >>"${LIVENESS_LOG}" 2>&1 || liveness_rc=$?
record_guarded "local-tunnel-liveness" \
  "liveness scheduler unit rows (cargo test -p i2pr-daemon --lib tunnel_liveness)" \
  "${liveness_rc}"

echo "==> external destination lane against exact-pinned i2pd (explicit ignored selection)"
DRIVER_EVIDENCE="${EVIDENCE_DIR}/driver"
mkdir -p "${DRIVER_EVIDENCE}"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0
if I2PD_ROUTER_INFO="${I2PD_RI}" \
   I2PD_SSU2_ENDPOINT="127.0.0.1:${I2PD_PORT}" \
   I2PD_SAM_ENDPOINT="127.0.0.1:${I2PD_SAM_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   EVIDENCE_DIR="${DRIVER_EVIDENCE}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test destination_tunnel_external \
   destination_message_plane_against_i2pd -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${DRIVER_EVIDENCE}/driver-evidence.tsv"
echo "==> sanitized reference-side facts (counts only, never key material)"
REFERENCE_FACTS="${EVIDENCE_DIR}/reference-facts.tsv"
: > "${REFERENCE_FACTS}"
{
  printf 'transit-endpoint-created\t%s\n' "$(grep -c 'TransitTunnel: endpoint .* created' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'transit-gateway-created\t%s\n' "$(grep -c 'TransitTunnel: gateway .* created' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'reference-leaseset-updated\t%s\n' "$(grep -c 'NetDb: LeaseSet2 updated' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'reference-tunnel-test-ok\t%s\n' "$(grep -c 'Tunnels: Test of .* successful' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'reference-sam-bridge-up\t%s\n' "$(grep -c 'Starting SAM bridge' "${I2PD_LOG}" 2>/dev/null || true)"
} >> "${REFERENCE_FACTS}"
ref_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  # Key-gated on the fresh per-run facts file (hygiene deletes it
  # at startup), independent of the driver exit code: reference
  # acceptance is reference-side evidence.
  if [[ -f "${REFERENCE_FACTS}" ]]; then
    local count
    count="$(grep -F "${key}" "${REFERENCE_FACTS}" 2>/dev/null | cut -f2 || true)"
    if [[ "${count:-0}" -ge 1 ]]; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}
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
# Key-gated on the fresh per-run driver TSV (hygiene deletes it at
# startup), independent of the driver exit code: the key proves the
# driver executed that step in this run. Used only for rows whose
# evidence the driver records before the §11 stop.
m6_key_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  if [[ -f "${DRIVER_TSV}" ]] && grep -Fq "${key}" "${DRIVER_TSV}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}
# Plan 187 §11 stop provenance: install-dependent rows pass via
# their own evidence key on the success path; when the driver
# recorded its stop key they are recorded `blocked` (never `passed`,
# never silently skipped); otherwise they fail without provenance.
STOP_FIRED=0
if [[ -f "${DRIVER_TSV}" ]] && grep -Fq "build-reply-gap-stop" "${DRIVER_TSV}"; then
  STOP_FIRED=1
fi
# Plan 191 inbound-delivery boundary E stop provenance: when the
# driver records its Plan 191 stop key, the four inbound-delivery
# rows are recorded `blocked` (never `passed`, never silently
# skipped); otherwise they fail without provenance.
PLAN_191_STOP_FIRED=0
if [[ -f "${DRIVER_TSV}" ]] && grep -Fq "inbound-delivery-boundary-E-stop" "${DRIVER_TSV}"; then
  PLAN_191_STOP_FIRED=1
fi
blocked_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  # Plan 191: the inbound-delivery boundary E uses distinct
  # `reference-received-timeout` / `destination-inbound-send-failed`
  # keys so substring matching does not silently promote a failed
  # row to passed.
  if [[ -f "${DRIVER_TSV}" ]] && awk -v k="${key}" -F'\t' '$1 == k {found=1} END{exit !found}' "${DRIVER_TSV}"; then
    record "${label}" passed "${detail}"
  elif [[ "${STOP_FIRED}" -eq 1 ]]; then
    record "${label}" blocked "${detail} (m6-build-reply-interop-gap; see install-pump-summary)"
  elif [[ "${PLAN_191_STOP_FIRED}" -eq 1 ]] && \
       [[ "${label}" == "external-reference-received" || "${label}" == "external-destination-inbound" ]]; then
    record "${label}" blocked "${detail} (m6-inbound-delivery-boundary-E; see Plan 191 §6 stop provenance)"
  else
    record "${label}" failed "${detail} (no evidence key, no stop provenance)"
  fi
}
m6_key_row "external-daemon-strict-profile" "daemon-strict-profile" \
  "daemon starts with strict SSU2 controlled profile (explicit --ignored --exact driver)"
m6_key_row "external-reference-verified" "reference-routerinfo-verified" \
  "exact-pinned RouterInfo parsed/verified through the documented file path"
m6_key_row "external-reference-floodfill" "reference-floodfill-capable" \
  "reference RouterInfo advertises floodfill and bootstraps the authoritative store"
m6_key_row "external-session-established" "session-established" \
  "authenticated SSU2 session establishes via daemon-owned runtime"
m6_key_row "external-sam-destination-created" "sam-destination-created" \
  "reference DATAGRAM service destination created through i2pd public SAM"
blocked_row "external-outbound-tunnel" "outbound-installed" \
  "real one-hop outbound build installed with cryptographically derived keys"
blocked_row "external-inbound-tunnel" "inbound-installed" \
  "real one-hop inbound build installed with cryptographically derived keys"
ref_row "external-outbound-accepted" "transit-endpoint-created" \
  "reference transit log proves the outbound build was accepted"
ref_row "external-inbound-accepted" "transit-gateway-created" \
  "reference transit log proves the inbound build was accepted"
ref_row "external-reference-ls2-published" "reference-leaseset-updated" \
  "reference floodfill stores the DATAGRAM destination LeaseSet2"
blocked_row "external-lease-lookup-tunnel" "lease-lookup-completed" \
  "reference Standard LeaseSet2 resolved through the real tunnel NetDB path and cached"
blocked_row "external-ls2-publication-tunnel" "ls2-publication-tunnel" \
  "local Standard LeaseSet2 with the real inbound lease published through the controlled path"
blocked_row "external-destination-outbound" "destination-outbound-delivered" \
  "bounded message traverses ECIES/Garlic + real outbound tunnel + selected remote lease"
blocked_row "external-reference-received" "reference-received" \
  "reference DATAGRAM session receives and authenticates the bounded message"
blocked_row "external-destination-inbound" "destination-inbound-received" \
  "reply traverses the real inbound tunnel and the existing ECIES decrypt path"
m6_key_row "external-direct-rejected" "direct-rejected" \
  "direct transport destination delivery is rejected as a counted path"
m6_key_row "external-liveness-first-test" "liveness-first-test" \
  "creator-side liveness scheduler first test succeeds during destination activity"

echo "==> workspace gates slice"
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-fixture-manifest \
           check-ntcp2-vectors check-ssu2-vectors check-ntcp2-interoperability \
           check-constrained-host-lane-boundary check-sam-acceptance-evidence \
           check-ssu2-acceptance-evidence check-destination-tunnel-evidence; do
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
    "schema": "i2pr-m6-destination-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m6-destination-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "role": "mandatory independent destination reference, unmodified",
        "transit": "notransit=false, floodfill=true, SAM loopback (Plan 187 only)",
    },
    "driver_evidence_keys": driver_keys,
    "results": rows,
    "m6_destination": "passed-via-i2pd-2.61.0" if all(
        row["status"] == "passed" for row in rows
    ) else "failed",
    "known_limitations": [
        "one-hop destination tunnels only; no multi-hop tunnel build",
        "loopback-only i2pd reference; no public I2P participation",
        "negative matrix (tamper, replay, sibling, stale, tunnel-loss) proven in local suites; external proves the success path plus direct-rejection",
        "no Streaming claim; Plan 188 owns the Streaming layer on this path",
        "Plan 190 corrects the inbound NetDB reply-path metadata defect (typed InboundGatewayRoute + daemon-owned reply_path_for_inbound_route adapter) so the encoded DatabaseLookup advertises the remote gateway receive id (0x9601), not the local endpoint id (0x9602); external-lease-lookup-tunnel flips from blocked to passed only after a fresh run proves a real tunneled lookup response arrives",
        "no per-tunnel task/timer design; one central scheduler owns every attempt",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 187 destination evidence (local suites + independent i2pd matrix)\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n")
    stream.write("- i2pd profile: `notransit=false`, `floodfill=true`, SAM loopback (Plan 187 only)\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 187 destination lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 187 destination lane passed; sanitized evidence: ${EVIDENCE_DIR}"
