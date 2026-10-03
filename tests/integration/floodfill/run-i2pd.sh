#!/usr/bin/env bash
# Plan 278 — exact-pinned i2pd 2.61.0 controlled floodfill qualification lane.
#
# Topology (all unprivileged, loopback-only, no reseed, no public network):
#
#   F1  stock i2pd floodfill=true   (reference replication target)
#   F2  stock i2pd floodfill=true   (reference replication target)
#   C   stock i2pd floodfill=false  (reference publisher + requester; the only
#                                   floodfill in its netDb is i2pr, so its own
#                                   NetDB selection is deterministic)
#   P   i2pr controlled floodfill   (the subject of the qualification)
#
# The controlled identity and its `caps=fR` RouterInfo (peer-test-confirmed
# reachability, Plan 306 / ADR 0030) are produced by the
# driver's prepare phase on a fixed loopback port and then seeded into C's
# netDb layout before C starts. The qualify phase reloads the same identity,
# rebinds the same port, and re-runs controlled activation against the live
# reference peers before driving the matrix.
#
# Required failures make this script fail. Sanitized evidence (counts, digests,
# categorical outcomes) defaults to target/interop/m12-floodfill-evidence; set
# I2PR_M12_FLOODFILL_EVIDENCE_DIR to retain it elsewhere. Reference private
# keys stay in the ephemeral scratch directory and are never copied to
# evidence.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M12_FLOODFILL_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m12-floodfill-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
I2PD_F1_PORT="${I2PR_I2PD_FLOODFILL_A_PORT:-43831}"
I2PD_F2_PORT="${I2PR_I2PD_FLOODFILL_B_PORT:-43832}"
I2PD_C_PORT="${I2PR_I2PD_FLOODFILL_C_PORT:-43833}"
I2PR_PORT="${I2PR_FLOODFILL_PORT:-44831}"
DRIVER_TIMEOUT="${I2PR_M12_FLOODFILL_TIMEOUT:-900s}"
# Frozen before execution: no retry-until-green, no budget increase.
MAX_ATTEMPTS=1

rm -rf "${EVIDENCE_DIR}"
mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m12-plan278.XXXXXX)"
STATE_DIR="${SCRATCH}/state"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"
PIDS=()

# ---- i2pd cache verification (fail closed before any network use) --------
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified Plan 278 source revision" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if "${I2PD_BIN}" --version 2>&1 | grep -Fq "${I2PD_VERSION}"; then
  echo "==> i2pd reference: ${I2PD_VERSION} (${I2PD_PIN})"
else
  echo "i2pd binary does not report ${I2PD_VERSION}" >&2
  exit 1
fi
echo "==> attempt budget: ${MAX_ATTEMPTS} (frozen)"

cleanup() {
  local pid
  for pid in "${PIDS[@]:-}"; do
    kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  done
  for pid in "${PIDS[@]:-}"; do
    wait "${pid}" 2>/dev/null || true
  done
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

# ---- reference instance configuration ------------------------------------
# Loopback only, SSU2 only, no reseed, no public network. F1/F2 carry transit
# so the client can build tunnels; C carries transit but is not a floodfill.
write_conf() { # name port floodfill
  local name="$1" port="$2" floodfill="$3"
  mkdir -p "${SCRATCH}/${name}" "${SCRATCH}/${name}data"
  cat > "${SCRATCH}/${name}/i2pd.conf" <<EOF
daemon = false
loglevel = debug
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = ${port}
ipv4 = true
ipv6 = false
nat = false
notransit = false
floodfill = ${floodfill}
# The controlled peer advertises a loopback SSU2 host. i2pd 2.61.0 invalidates
# a host option that falls in a reserved IPv4 range (libi2pd/util.cpp
# IsInReservedRange covers 127.0.0.0/8) unless this check is disabled; every
# existing loopback lane in this repository sets it. This is lane
# configuration, not a router or protocol change.
reservedrange = false
bandwidth = L
[ssu2]
enabled = true
published = true
port = ${port}
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
verify = false
urls =
threshold = 0
EOF
}

start_i2pd() { # name port
  local name="$1" port="$2"
  setsid "${I2PD_BIN}" "--conf=${SCRATCH}/${name}/i2pd.conf" \
    "--datadir=${SCRATCH}/${name}data" --log=file \
    "--logfile=${EVIDENCE_DIR}/${name}.log" >/dev/null 2>&1 < /dev/null &
  PIDS+=("$!")
  printf '%s' "$!" > "${SCRATCH}/${name}.pid"
}

wait_ready() { # name port attempts
  local name="$1" port="$2" n="$3" i
  for i in $(seq 1 "${n}"); do
    if [[ -f "${SCRATCH}/${name}data/router.info" ]] &&
       grep -Fq "Start listening on 127.0.0.1:${port}" \
         "${EVIDENCE_DIR}/${name}.log" 2>/dev/null; then
      return 0
    fi
    sleep 0.5
  done
  echo "ephemeral i2pd ${name} never published router.info / SSU2 listener" >&2
  return 1
}

# Seeds one RouterInfo into i2pd's exact source-locked HashedStorage layout:
# <datadir>/netDb/r<C0>/routerInfo-<I>.dat over all 64 buckets, where the ident
# rendering is the i2p base64 of the 32-byte router hash.
seed_netdb() { # datadir src_file ident_b64
  local datadir="$1" src="$2" ident="$3" first bucket
  first="${ident:0:1}"
  for bucket in 0 1 2 3 4 5 6 7 8 9 a b c d e f g h i j k l m n o p q r s t u v w x y z A B C D E F G H I J K L M N O P Q R S T U V W X Y Z - ~; do
    mkdir -p "${datadir}/netDb/r${bucket}"
    cp "${src}" "${datadir}/netDb/r${bucket}/routerInfo-${ident}.dat"
  done
  [[ -n "${first}" ]]
}

record_guarded() { # label detail rc
  local label="$1" detail="$2" rc="$3"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  if [[ "${rc}" -eq 0 ]]; then
    printf '%s\t%s\tpassed\t%s\n' "${label}" "plan278" "${detail}" >> "${RESULTS_FILE}"
  else
    printf '%s\t%s\tfailed\t%s (exit %s)\n' "${label}" "plan278" "${detail}" "${rc}" >> "${RESULTS_FILE}"
  fi
}

# ---- phase 0: prepare the stable controlled identity + caps=f RouterInfo --
mkdir -p "${STATE_DIR}"
PREPARE_DIR="${EVIDENCE_DIR}/phase-prepare"
mkdir -p "${PREPARE_DIR}"
echo "==> phase 1: controlled identity + controlled activation"
prepare_rc=0
I2PR_FLOODFILL_STATE_DIR="${STATE_DIR}" \
I2PR_FLOODFILL_BIND_PORT="${I2PR_PORT}" \
EVIDENCE_DIR="${EVIDENCE_DIR}" \
timeout --foreground "${DRIVER_TIMEOUT}" cargo test --locked -p i2pr-daemon \
  --test floodfill_i2pd_external floodfill_prepare_against_i2pd \
  -- --ignored --exact --nocapture --test-threads=1 \
  > "${EVIDENCE_DIR}/prepare-driver.log" 2>&1 || prepare_rc=$?
record_guarded "controlled-activation-completed" \
  "peer-test-confirmed activation installed the caps=f RouterInfo (explicit --ignored --exact driver)" \
  "${prepare_rc}"
if [[ "${prepare_rc}" -ne 0 ]]; then
  sed -n '1,60p' "${EVIDENCE_DIR}/prepare-driver.log" >&2 || true
  echo "Plan 278 lane stopped at controlled activation" >&2
  exit 1
fi
PUBLISHED="${STATE_DIR}/published.routerinfo"
if [[ ! -f "${PUBLISHED}" ]]; then
  echo "controlled activation did not publish a RouterInfo" >&2
  exit 1
fi
echo "    published $(wc -c < "${PUBLISHED}") bytes"

# ---- phase 1: bring up the reference mesh and seed the controlled peer ----
write_conf f1 "${I2PD_F1_PORT}" true
write_conf f2 "${I2PD_F2_PORT}" true
write_conf c "${I2PD_C_PORT}" false
start_i2pd f1 "${I2PD_F1_PORT}"
start_i2pd f2 "${I2PD_F2_PORT}"
wait_ready f1 "${I2PD_F1_PORT}" 120
wait_ready f2 "${I2PD_F2_PORT}" 120
echo "==> reference floodfills up on 127.0.0.1:${I2PD_F1_PORT},127.0.0.1:${I2PD_F2_PORT}"

# C must know only i2pr as a floodfill, so its own NetDB selection is
# deterministic. The ident is the i2p base64 router hash the prepare phase
# recorded; i2pd's HashedStorage filenames use exactly that rendering.
CONTROLLED_IDENT="$(<"${STATE_DIR}/ident.b64")"
if [[ -z "${CONTROLLED_IDENT}" ]]; then
  echo "prepare phase did not record the controlled router ident" >&2
  exit 2
fi
seed_netdb "${SCRATCH}/cdata" "${PUBLISHED}" "${CONTROLLED_IDENT}"
echo "==> seeded controlled RouterInfo into the reference client's netDb layout"
start_i2pd c "${I2PD_C_PORT}"
wait_ready c "${I2PD_C_PORT}" 120
echo "    reference client up on 127.0.0.1:${I2PD_C_PORT}"

# ---- phase 2: run the qualification matrix --------------------------------
echo "==> phase 2: qualification matrix"
QUALIFY_DIR="${EVIDENCE_DIR}/phase-qualify"
mkdir -p "${QUALIFY_DIR}"
qualify_rc=0
I2PR_FLOODFILL_STATE_DIR="${STATE_DIR}" \
I2PR_FLOODFILL_BIND_PORT="${I2PR_PORT}" \
EVIDENCE_DIR="${EVIDENCE_DIR}" \
I2PD_FLOODFILL_A_ROUTER_INFO="${SCRATCH}/f1data/router.info" \
I2PD_FLOODFILL_A_ENDPOINT="127.0.0.1:${I2PD_F1_PORT}" \
I2PD_FLOODFILL_B_ROUTER_INFO="${SCRATCH}/f2data/router.info" \
I2PD_FLOODFILL_B_ENDPOINT="127.0.0.1:${I2PD_F2_PORT}" \
I2PD_FLOODFILL_C_ROUTER_INFO="${SCRATCH}/cdata/router.info" \
I2PD_FLOODFILL_C_ENDPOINT="127.0.0.1:${I2PD_C_PORT}" \
timeout --foreground "${DRIVER_TIMEOUT}" cargo test --locked -p i2pr-daemon \
  --test floodfill_i2pd_external floodfill_qualify_against_i2pd \
  -- --ignored --exact --nocapture --test-threads=1 \
  > "${EVIDENCE_DIR}/qualify-driver.log" 2>&1 || qualify_rc=$?
if [[ "${qualify_rc}" -ne 0 ]]; then
  sed -n '1,80p' "${EVIDENCE_DIR}/qualify-driver.log" >&2 || true
fi

DRIVER_TSV="${EVIDENCE_DIR}/driver-evidence.tsv"
driver_row() { # label key detail
  local label="$1" key="$2" detail="$3" rc=1
  if [[ "${qualify_rc}" -eq 0 && -f "${DRIVER_TSV}" ]] &&
     grep -Fq "${key}" "${DRIVER_TSV}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}

# ---- boundary probe -------------------------------------------------------
# The qualification cannot start if the reference never loads the controlled
# RouterInfo. This probe seeds a second reference client with the same identity
# plus one extra options entry, so the exact reference-side gate is localized
# instead of guessed. It never satisfies a matrix row.
PROBE_RC=0
PROBE_ACCEPTED="unknown"
if I2PR_FLOODFILL_STATE_DIR="${STATE_DIR}" \
   I2PR_FLOODFILL_BIND_PORT="${I2PR_PORT}" \
   EVIDENCE_DIR="${EVIDENCE_DIR}" \
   timeout --foreground 300 cargo test --locked -p i2pr-daemon \
   --test floodfill_i2pd_external floodfill_diagnose_version_gate \
   -- --ignored --exact --nocapture --test-threads=1 \
   > "${EVIDENCE_DIR}/probe-driver.log" 2>&1; then
  PROBE_RC=0
else
  PROBE_RC=$?
fi
if [[ "${PROBE_RC}" -eq 0 && -f "${STATE_DIR}/probe.routerinfo" ]]; then
  I2PD_C2_PORT="${I2PR_I2PD_FLOODFILL_C2_PORT:-43834}"
  PROBE_IDENT="$(<"${STATE_DIR}/probe.ident.b64")"
  rm -rf "${SCRATCH}/c2" "${SCRATCH}/c2data"
  write_conf c2 "${I2PD_C2_PORT}" false
  seed_netdb "${SCRATCH}/c2data" "${STATE_DIR}/probe.routerinfo" "${PROBE_IDENT}"
  start_i2pd c2 "${I2PD_C2_PORT}"
  if wait_ready c2 "${I2PD_C2_PORT}" 120; then
    sleep 5
    if grep -Fq "is invalid or too old" "${EVIDENCE_DIR}/c2.log"; then
      PROBE_ACCEPTED="rejected"
    else
      PROBE_ACCEPTED="accepted"
    fi
  else
    PROBE_ACCEPTED="no-start"
  fi
fi
{
  echo "plan278	reference-accepts-versioned-routerinfo	diagnostic-only	diagnostic probe: versioned options probe ${PROBE_ACCEPTED}"
} >> "${RESULTS_FILE}"
echo "==> boundary probe (diagnostic-only): reference ${PROBE_ACCEPTED} a versioned controlled RouterInfo"

driver_row "external-reference-verified" "reference-routerinfo-verified" \
  "every reference RouterInfo parsed and verified through the documented file path"
driver_row "external-role-active" "role-active-after-activation" \
  "controlled activation reached the Active role with the caps=f RouterInfo installed"
driver_row "external-publisher-store" "publisher-store-accepted" \
  "reference publisher store validated and accepted (matrix A)"
driver_row "external-store-ack" "publisher-store-ack-delivered" \
  "DatabaseStore DeliveryStatus acknowledgement delivered to the reference (matrix A)"
driver_row "external-lookup-answered" "lookup-replies-classified" \
  "reference DatabaseLookup answered with classified replies (matrix C/E)"
driver_row "external-replication-direct" "replication-direct-store-delivered" \
  "direct zero-token SSU2 DatabaseStore reached the reference floodfill (matrix F)"
driver_row "external-no-tunnel-flood" "replication-tunnel-fallback-absent" \
  "replication stayed on the direct authenticated path with no tunnel fallback"
driver_row "external-dispatch-clean" "client-peer-identified" \
  "no dispatch errors and no unclassified effects during the matrix"

# ---- local gates slice ----------------------------------------------------
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-m12-floodfill-boundaries; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
record_guarded "workspace-gates" \
  "fmt + workspace check --all-targets + static boundary scripts" "${gates_rc}"

python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PD_PIN}" "${I2PD_VERSION}" \
  "${MAX_ATTEMPTS}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root, i2pd_pin, i2pd_version, budget = sys.argv[1:7]
rows = []
diagnostic = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, plan, status, detail = line.rstrip("\n").split("\t", 3)
        entry = {"label": label, "plan": plan, "status": status, "detail": detail}
        # A diagnostic-only row is never required to pass; it is retained as
        # boundary provenance and must never be promoted to a matrix row.
        (diagnostic if status == "diagnostic-only" else rows).append(entry)

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
driver_keys = []
driver_tsv = Path(evidence_dir) / "driver-evidence.tsv"
if driver_tsv.exists():
    for line in driver_tsv.read_text(encoding="utf-8").splitlines():
        driver_keys.append(line.split("\t", 1)[0])
passed = all(row["status"] == "passed" for row in rows)
evidence = {
    "schema": "i2pr-m12-floodfill-qualification-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m12-floodfill-i2pd-external",
    "attempt_budget": int(budget),
    "attempts_used": 1,
    "bind_policy": "127.0.0.1 loopback only; controlled fixed port; no introducer",
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "role": "mandatory independent floodfill reference, unmodified",
        "profile": "two floodfill=true reference peers, one floodfill=false client, SSU2 only, reseed disabled",
    },
    "driver_evidence_keys": driver_keys,
    "results": rows,
    "diagnostic_only": diagnostic,
    "m12_floodfill_one_family": "passed-via-i2pd-2.61.0" if passed else "failed",
    "known_limitations": [
        "loopback-only controlled mesh; no public I2P participation",
        "one-family (i2pd) experimental progression only; no normal-daemon caps=f advertisement",
        "reference logs are diagnostic only; retained evidence is sanitized counts, digests, and categorical outcomes",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 278 M12 controlled floodfill qualification (exact-pinned i2pd)\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- Attempt budget: `{budget}` frozen before execution\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, fixed controlled port, no introducer\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows + diagnostic:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if awk -F'\t' '$3 != "passed" && $3 != "diagnostic-only" { found = 1 } END { exit found ? 0 : 1 }' "${RESULTS_FILE}"; then
  echo "Plan 278 lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 278 lane passed; sanitized evidence: ${EVIDENCE_DIR}"
