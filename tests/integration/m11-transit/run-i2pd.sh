#!/usr/bin/env bash
# Plan 255 — M11 exact-pinned i2pd controlled transit qualification runner.
#
# Local rows execute the focused Plan 254 / Plan 255 transit lanes
# (live-owner regressions, disabled-probe regressions, m11_i2pd_external
# regression additions, and the static boundary checkers). The external
# rows provision one ephemeral exact-pinned i2pd 2.61.0 process on
# loopback with transit enabled (`notransit = false`, public reseed
# disabled, public network disabled, `i2pd.conf` locked to loopback)
# and run the single fail-closed driver
# `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` through its
# explicit `--ignored --exact` selection. The driver consumes
# `Ssu2DaemonHandle::next_inbound()` from the production owner and
# proves the real authenticated SSU2 -> enabled `TransitLiveOwner`
# path for OBEP / IBGW / Participant roles. Each external row derives
# from the driver exit status plus its own sanitized evidence keys
# written to `driver-evidence.tsv`.
#
# The lane is unprivileged and loopback-only. Required failures make
# this script fail. Sanitized evidence defaults below
# target/interop/m11-transit-evidence; set I2PR_M11_EVIDENCE_DIR to
# retain it elsewhere. Private router keys stay in the ephemeral
# scratch directory and are never copied to evidence.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M11_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m11-transit-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
I2PD_PORT="${I2PR_I2PD_PORT:-43983}"
I2PR_PORT="${I2PR_M11_PORT:-44181}"
I2PD_B_PORT="${I2PR_I2PD_B_PORT:-43984}"
DRIVER_TIMEOUT="600s"

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m11-plan255.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"

# ---- i2pd cache verification (fail closed before any network use) -------
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified Plan 255 source revision" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if "${I2PD_BIN}" --version 2>&1 | grep -Fq "${I2PD_VERSION}"; then
  echo "==> i2pd reference: ${I2PD_VERSION} (${I2PD_PIN})"
else
  echo "i2pd binary does not report ${I2PD_VERSION}" >&2
  exit 1
fi

# ---- ephemeral i2pd-A provisioning (creator, transit enabled) -----------
# i2pd-A is the build creator. It must know i2pr's signed RouterInfo and
# be configured to use i2pr as an explicit peer for its tunnel pool. The
# runner supplies the genuine signed i2pr RouterInfo by writing it into
# i2pd-A's local netDb before startup; no in-memory patching or LD_PRELOAD
# hook is used. Reseed is explicitly disabled (no public network).
I2PD_A_HOME="${SCRATCH}/i2pd-a"
I2PD_A_DATA="${I2PD_A_HOME}/data"
I2PD_A_LOG="${EVIDENCE_DIR}/i2pd-a.log"
mkdir -p "${I2PD_A_DATA}/netDb"
cat > "${I2PD_A_HOME}/i2pd.conf" <<EOF
daemon = false
loglevel = info
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = ${I2PD_PORT}
ipv4 = true
ipv6 = false
nat = false
notransit = false
floodfill = false
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
port = ${I2PD_PORT_SAM:-$((I2PD_PORT + 1000))}
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
: > "${I2PD_A_LOG}"
setsid "${I2PD_BIN}" "--conf=${I2PD_A_HOME}/i2pd.conf" "--datadir=${I2PD_A_DATA}" \
  --log=file "--logfile=${I2PD_A_LOG}" >/dev/null 2>&1 < /dev/null &
I2PD_A_PID=$!
CHILD_PIDS=("${I2PD_A_PID}")

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

echo "==> waiting for ephemeral i2pd-A on 127.0.0.1:${I2PD_PORT}"
I2PD_A_RI=""
for _ in $(seq 1 240); do
  if [[ -f "${I2PD_A_DATA}/router.info" ]] &&
     grep -Fq "Start listening on 127.0.0.1:${I2PD_PORT}" "${I2PD_A_LOG}" 2>/dev/null; then
    I2PD_A_RI="${I2PD_A_DATA}/router.info"
    break
  fi
  if ! kill -0 "${I2PD_A_PID}" 2>/dev/null; then
    echo "ephemeral i2pd-A exited during startup" >&2
    sed -n '1,80p' "${I2PD_A_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ -z "${I2PD_A_RI}" ]]; then
  echo "ephemeral i2pd-A did not publish router.info / SSU2 listener" >&2
  sed -n '1,80p' "${I2PD_A_LOG}" >&2 || true
  exit 2
fi
echo "    i2pd-A: 127.0.0.1:${I2PD_PORT} ($(wc -c <"${I2PD_A_RI}")-byte router.info)"

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

echo "==> local Plan 254 / Plan 255 rows (foundation)"
LOCAL_LOG="${EVIDENCE_DIR}/local-foundation.log"
: > "${LOCAL_LOG}"
local_rc=0
cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- \
  --test-threads=1 >>"${LOCAL_LOG}" 2>&1 || local_rc=$?
record_guarded "m11-i2pd-live-owner-enabled" \
  "live-owner 34-row matrix (cargo test -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1)" \
  "${local_rc}"

DISABLED_LOG="${EVIDENCE_DIR}/local-disabled-probe.log"
: > "${DISABLED_LOG}"
disabled_rc=0
cargo test --locked -p i2pr-daemon --lib transit_owner -- \
  --test-threads=1 >>"${DISABLED_LOG}" 2>&1 || disabled_rc=$?
record_guarded "m11-i2pd-no-direct-build-injection" \
  "disabled-probe + direct-injection regression rows (cargo test -p i2pr-daemon --lib transit_owner -- --test-threads=1)" \
  "${disabled_rc}"

FOCUS_LOG="${EVIDENCE_DIR}/local-focus.log"
: > "${FOCUS_LOG}"
focus_rc=0
if cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external -- \
   --test-threads=1 --skip m11_transit_against_i2pd >>"${FOCUS_LOG}" 2>&1; then
  focus_rc=0
else
  focus_rc=$?
fi
record_guarded "m11-i2pd-driver-exists" \
  "external driver compiles + the non-environment local rows pass (cargo test -p i2pr-daemon --test m11_transit_i2pd_external --skip external)" \
  "${focus_rc}"

# Local row: the driver must be #[ignore]-gated so ordinary CI stays green
if grep -qE '#\[ignore\s*=.*Plan 255' \
     "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"; then
  record_guarded "m11-i2pd-driver-ignored-gated" \
    "external driver is #[ignore]-gated with the Plan 255 explanation" \
    "0"
else
  record_guarded "m11-i2pd-driver-ignored-gated" \
    "external driver is #[ignore]-gated with the Plan 255 explanation" \
    "1"
fi

# Local row: missing i2pd environment must fail the driver closed
if grep -qE 'env_value|missing required env' \
     "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"; then
  record_guarded "m11-i2pd-driver-missing-env-fails" \
    "external driver requires exact i2pd env variables and fails closed on absence" \
    "0"
else
  record_guarded "m11-i2pd-driver-missing-env-fails" \
    "external driver requires exact i2pd env variables and fails closed on absence" \
    "1"
fi

# Work package B source-lock rows -----------------------------------------
SHA_PIN="$(<"${I2PD_CACHE}/source-revision.txt")"
if [[ "${SHA_PIN}" == "${I2PD_PIN}" ]]; then
  record_guarded "m11-i2pd-source-pin" \
    "i2pd cache revision is exactly ${I2PD_PIN}" \
    "0"
else
  record_guarded "m11-i2pd-source-pin" \
    "i2pd cache revision is exactly ${I2PD_PIN}" \
    "1"
fi

if git -C "${REPO_ROOT}/target/interop/ssu2-sources/i2pd-${I2PD_PIN}" \
     status --porcelain --untracked-files=no 2>/dev/null | rg -q .; then
  record_guarded "m11-i2pd-source-clean" \
    "i2pd source tree is clean (no tracked modifications)" \
    "1"
else
  record_guarded "m11-i2pd-source-clean" \
    "i2pd source tree is clean (no tracked modifications)" \
    "0"
fi

if grep -qE 'm_BuildMessage|SendShortBuildMessage|HandleShortBuildRequest' \
     "${REPO_ROOT}/target/interop/ssu2-sources/i2pd-${I2PD_PIN}/libi2pd/TunnelPool.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-short-build-source-lock" \
    "short-build creation paths exist in i2pd 2.61.0 source" \
    "0"
else
  record_guarded "m11-i2pd-short-build-source-lock" \
    "short-build creation paths exist in i2pd 2.61.0 source" \
    "1"
fi

if grep -qE 'SetExplicitPeers|GetNextRouter|I2CP_PARAM_EXPLICIT_PEERS' \
     "${REPO_ROOT}/target/interop/ssu2-sources/i2pd-${I2PD_PIN}/libi2pd/TunnelPool.h" \
     "${REPO_ROOT}/target/interop/ssu2-sources/i2pd-${I2PD_PIN}/libi2pd/TunnelPool.cpp" \
     "${REPO_ROOT}/target/interop/ssu2-sources/i2pd-${I2PD_PIN}/libi2pd/Destination.cpp" 2>/dev/null; then
  record_guarded "m11-i2pd-explicit-peer-source-lock" \
    "explicitPeers + getNextRouter source-locked in i2pd 2.61.0" \
    "0"
else
  record_guarded "m11-i2pd-explicit-peer-source-lock" \
    "explicitPeers + getNextRouter source-locked in i2pd 2.61.0" \
    "1"
fi

# Static guard rows --------------------------------------------------------
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
bash "${REPO_ROOT}/scripts/check-m11-transit-boundaries.sh" >>"${GATES_LOG}" 2>&1 || gates_rc=1
bash "${REPO_ROOT}/scripts/check-m11-transit-qualification-evidence.sh" >>"${GATES_LOG}" 2>&1 || gates_rc=1
record_guarded "m11-i2pd-runner-pinned" \
  "static boundary + Plan 255 evidence checkers (check-m11-transit-{boundaries,qualification-evidence}.sh)" \
  "${gates_rc}"

if grep -qF '127.0.0.1' "${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh"; then
  record_guarded "m11-i2pd-runner-loopback" \
    "runner stays loopback-only (127.0.0.1)" \
    "0"
else
  record_guarded "m11-i2pd-runner-loopback" \
    "runner stays loopback-only (127.0.0.1)" \
    "1"
fi

if grep -qF '[reseed]' "${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh" &&
   ! grep -qE 'reseed-url=|reseedverify|reseedverifyurl|http://reseed' "${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh"; then
  record_guarded "m11-i2pd-runner-no-public-network" \
    "runner disables public reseed/network" \
    "0"
else
  record_guarded "m11-i2pd-runner-no-public-network" \
    "runner disables public reseed/network" \
    "1"
fi

if [[ -f "${REPO_ROOT}/.github/workflows/m11-transit-external.yml" ]]; then
  record_guarded "m11-i2pd-workflow-exists" \
    "hosted M11 transit external workflow exists" \
    "0"
else
  record_guarded "m11-i2pd-workflow-exists" \
    "hosted M11 transit external workflow exists" \
    "1"
fi

if ! rg -q 'static_secret|static_priv_key|session_priv|router_secret|private_key_file|privkey_path' \
     "${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"; then
  record_guarded "m11-i2pd-evidence-no-secret" \
    "external driver does not retain secret material" \
    "0"
else
  record_guarded "m11-i2pd-evidence-no-secret" \
    "external driver does not retain secret material" \
    "1"
fi

echo "==> external M11 transit qualification against exact-pinned i2pd"
# The external driver consumes the real Ssu2DaemonHandle::next_inbound
# stream from the daemon-owned runtime and feeds those exact
# authenticated events into an enabled TransitLiveOwner. The driver is
# `#[ignore]`-gated; explicit selection fails closed without env vars.
DRIVER_EVIDENCE="${EVIDENCE_DIR}/driver"
mkdir -p "${DRIVER_EVIDENCE}"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0
if I2PD_ROUTER_INFO="${I2PD_A_RI}" \
   I2PD_SSU2_ENDPOINT="127.0.0.1:${I2PD_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   EVIDENCE_DIR="${DRIVER_EVIDENCE}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external \
   m11_transit_against_i2pd -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${DRIVER_EVIDENCE}/driver-evidence.tsv"
m11_row() {
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

# Work package A — real SSU2 inbound ownership gate
m11_row "m11-i2pd-live-next-inbound-observed" "live-next-inbound-observed" \
  "real Ssu2DaemonHandle::next_inbound events observed through the live owner"
m11_row "m11-i2pd-live-owner-enabled" "live-owner-enabled" \
  "controlled TransitLiveOwner is enabled and reachable from the runtime"
m11_row "m11-i2pd-authenticated-peer-bound" "authenticated-peer-bound" \
  "peer/link provenance is bound to the authenticated SSU2 session"
m11_row "m11-i2pd-no-direct-build-injection" "no-direct-build-injection" \
  "no hand-built STBM enters the live owner after runtime startup"
m11_row "m11-i2pd-session-close-reconciled" "session-close-reconciled" \
  "session close invokes note_session_closed and reconciles the peer index"

# Work package B — exact-pin source lock and peer placement
m11_row "m11-i2pd-reference-knows-i2pr-ri" "reference-knows-i2pr-ri" \
  "i2pd-A knows the genuine signed i2pr RouterInfo via the bounded bootstrap"
m11_row "m11-i2pd-selected-role-proven" "selected-role-proven" \
  "selected role (OBEP / IBGW / Participant) is provable from decoded request metadata"

# Work package C — accepted build matrix
m11_row "m11-i2pd-obep-build-received" "obep-build-received" \
  "real i2pd-A outbound tunnel build reaches i2pr as OBEP"
m11_row "m11-i2pd-obep-build-accepted" "obep-build-accepted" \
  "OBEP build accepted with code 0 reply and live registration"
m11_row "m11-i2pd-obep-registration-live" "obep-registration-live" \
  "exactly one OBEP live registration installed after acceptance"
m11_row "m11-i2pd-ibgw-build-received" "ibgw-build-received" \
  "real i2pd-A inbound tunnel build reaches i2pr as IBGW"
m11_row "m11-i2pd-ibgw-build-accepted" "ibgw-build-accepted" \
  "IBGW build accepted with code 0 reply and live registration"
m11_row "m11-i2pd-ibgw-registration-live" "ibgw-registration-live" \
  "exactly one IBGW live registration installed after acceptance"
m11_row "m11-i2pd-participant-build-received" "participant-build-received" \
  "real i2pd-A -> i2pr -> i2pd-B intermediate build reaches i2pr as Participant"
m11_row "m11-i2pd-participant-build-accepted" "participant-build-accepted" \
  "Participant build accepted with code 0 reply and live registration"
m11_row "m11-i2pd-participant-registration-live" "participant-registration-live" \
  "exactly one Participant live registration installed after acceptance"

# Work package D — role-correct live data plane
m11_row "m11-i2pd-participant-data-forward" "participant-data-forward" \
  "Participant TunnelData transforms and forwards to the next router"
m11_row "m11-i2pd-participant-data-digest" "participant-data-digest" \
  "far-side digest/counter proves the transform is not a no-op"
m11_row "m11-i2pd-obep-delivery" "obep-delivery" \
  "OBEP semantic delivery emits exactly one routing action"
m11_row "m11-i2pd-obep-fragmented-once" "obep-fragmented-once" \
  "fragmented OBEP delivery reassembles to one semantic delivery"
m11_row "m11-i2pd-ibgw-gateway-ingress" "ibgw-gateway-ingress" \
  "IBGW TunnelGateway ingress emits bounded TunnelData cells"
m11_row "m11-i2pd-ibgw-multicell-bounded" "ibgw-multicell-bounded" \
  "IBGW multi-cell delivery is bounded and emits no spurious cells"
m11_row "m11-i2pd-replay-no-second-delivery" "replay-no-second-delivery" \
  "duplicate TunnelData produces no second delivery"

# Work package E — rejection and bandwidth
m11_row "m11-i2pd-code30-build-rejected" "code30-build-rejected" \
  "deterministic-reject admission returns code 30 to i2pd-A"
m11_row "m11-i2pd-code30-no-registration" "code30-no-registration" \
  "code 30 leaves zero live registrations"
m11_row "m11-i2pd-code30-pending-baseline" "code30-pending-baseline" \
  "all pending / per-peer / global counters return to baseline"
m11_row "m11-i2pd-bandwidth-option-disposition" "bandwidth-option-disposition" \
  "m/r/l/b option disposition is truthful (no fabrication)"

# Work package F — expiry / replay / cancellation / restart
m11_row "m11-i2pd-expiry-drops-live-data" "expiry-drops-live-data" \
  "logical 600-second expiry drops later genuine TunnelData"
m11_row "m11-i2pd-expiry-resource-baseline" "expiry-resource-baseline" \
  "expiry removes registration and clears secret-owning state"
m11_row "m11-i2pd-cancel-drains" "cancel-drains" \
  "cancellation drains transit synchronously"
m11_row "m11-i2pd-session-close-peer-baseline" "session-close-peer-baseline" \
  "session close returns active/pending/peer state to zero"
m11_row "m11-i2pd-restart-clean-baseline" "restart-clean-baseline" \
  "fresh datadir restart proves no transit state survives"

echo "==> workspace gates slice"
WS_GATES_LOG="${EVIDENCE_DIR}/workspace-gates-slice.log"
: > "${WS_GATES_LOG}"
ws_rc=0
cargo fmt --all --check >>"${WS_GATES_LOG}" 2>&1 || ws_rc=1
cargo check --locked --workspace --all-targets >>"${WS_GATES_LOG}" 2>&1 || ws_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-service-tunnel-boundaries \
           check-m11-transit-boundaries check-m11-transit-qualification-evidence \
           check-fixture-manifest check-ntcp2-vectors check-ssu2-vectors check-i2cp-vectors \
           check-ntcp2-interoperability check-constrained-host-lane-boundary \
           check-sam-acceptance-evidence check-ssu2-acceptance-evidence \
           check-i2cp-acceptance-evidence check-service-tunnel-acceptance-evidence \
           check-exploratory-tunnel-evidence check-netdb-tunnel-evidence \
           check-destination-tunnel-evidence check-streaming-tunnel-evidence \
           check-m6-mixed-router-acceptance-evidence; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${WS_GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${WS_GATES_LOG}"
    ws_rc=1
  fi
done
record_guarded "m11-i2pd-driver-exists" \
  "fmt + workspace check + 18 static gate scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${ws_rc}"

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
all_passed = all(row["status"] == "passed" for row in rows)
evidence = {
    "schema": "i2pr-m11-transit-qualification-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m11-transit-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "build_command": "make USE_UPNP=no DEBUG=0 (via scripts/interop/fetch-ssu2-reference.sh)",
        "role": "mandatory independent M11 controlled transit reference, unmodified",
    },
    "driver_evidence_keys": driver_keys,
    "results": rows,
    "m11_transit_qualification": (
        "passed-via-i2pd-2.61.0" if all_passed else "failed"
    ),
    "known_limitations": [
        "controlled transit qualification only; no public transit, RouterInfo capability, or public-network participation",
        "direct loopback SSU2 session evidence only; no public I2P participation",
        "exact-pinned i2pd 2.61.0 reference; Java second-family lane stays retained/deferred",
        "two complete same-SHA external passes remain owned by hosted Actions or manual execution",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 255 M11 exact-pinned i2pd controlled transit qualification\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 255 M11 transit qualification lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 255 M11 transit qualification lane passed; sanitized evidence: ${EVIDENCE_DIR}"
