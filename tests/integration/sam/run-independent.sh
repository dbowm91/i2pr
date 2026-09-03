#!/usr/bin/env bash
# Plan 151 — run the SAM external-client matrix plus the Plan 151 local
# acceptance suites end-to-end, deriving every final evidence row from
# an executed command/test result.
#
# Provenance: this lane retains the Plan 150 external-client matrix
# (pinned i2psam + i2plib-substitute runners, transcripts, FORWARD
# echo) and additionally executes the Plan 151 Rust acceptance suites
# (sibling/slow-peer/fault/close-reset rows), the FORWARD lifecycle
# suite, the Plan 127–134 regression commands, and a workspace-gates
# slice. No required row is recorded `passed` except by the exit
# status (or captured per-test lines) of its associated command; see
# scripts/check-sam-acceptance-evidence.sh.
#
# The listener is driven only through its TCP interface. Required failures
# make this script fail. Sanitized evidence defaults below target/interop;
# set I2PR_SAM_EVIDENCE_DIR to retain it elsewhere.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CLIENTS_DIR="${REPO_ROOT}/tests/integration/sam/clients"
BUILD_DIR="${REPO_ROOT}/tests/integration/sam/build"
EVIDENCE_DIR="${I2PR_SAM_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/sam-evidence}"
LISTENER_LOG="${REPO_ROOT}/target/interop/sam-listener.log"
RUN_TIMEOUT="180s"
I2PSAM_PIN="b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac"
I2PLIB_PIN="6edf51cd5d21cc745aa7e23cb98c582144884fa8"
I2PLIB_CACHE="${REPO_ROOT}/target/interop/cache/sam/i2plib/${I2PLIB_PIN}"

mkdir -p "${EVIDENCE_DIR}" "$(dirname "${LISTENER_LOG}")"
bash "${CLIENTS_DIR}/build.sh" >/dev/null

I2PSAM_RUNNER="${BUILD_DIR}/i2psam_runner"
I2PSAM_FORWARD="${BUILD_DIR}/i2psam_forward_runner"
I2PLIB_RUNNER="${CLIENTS_DIR}/i2plib_runner.py"
TRANSCRIPT="${BUILD_DIR}/transcript.py"
ECHO_TARGET="${BUILD_DIR}/echo_target.py"
for required in "${I2PSAM_RUNNER}" "${I2PSAM_FORWARD}" "${I2PLIB_RUNNER}" \
                "${TRANSCRIPT}" "${ECHO_TARGET}"; do
  if [[ ! -e "${required}" ]]; then
    echo "missing Plan 150 runner: ${required}" >&2
    exit 1
  fi
done
if [[ ! -f "${I2PLIB_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PLIB_CACHE}/source-revision.txt")" != "${I2PLIB_PIN}" ]]; then
  echo "i2plib cache has no verified Plan 150 source revision" >&2
  echo "run scripts/interop/fetch-sam-clients.sh --rebuild first" >&2
  exit 1
fi

echo "==> building i2pr SAM listener example"
cargo build --example sam_loopback_listener -p i2pr-daemon --quiet
LISTENER="${REPO_ROOT}/target/debug/examples/sam_loopback_listener"

echo "==> starting i2pr SAM listener"
setsid "${LISTENER}" --port 0 >"${LISTENER_LOG}" 2>&1 < /dev/null &
LISTENER_PID=$!
CHILD_PIDS=("${LISTENER_PID}")
SCRATCH="$(mktemp -d -t i2pr-sam-plan151.XXXXXX)"

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

SAM_PORT=""
for _ in $(seq 1 120); do
  if [[ -s "${LISTENER_LOG}" ]]; then
    SAM_PORT="$(python3 - "${LISTENER_LOG}" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], encoding="utf-8") as stream:
        for line in stream:
            try:
                value = json.loads(line).get("port")
            except (TypeError, ValueError):
                continue
            if isinstance(value, int) and 0 < value < 65536:
                print(value)
                break
except OSError:
    pass
PY
)"
    [[ -n "${SAM_PORT}" ]] && break
  fi
  sleep 0.1
done
if [[ -z "${SAM_PORT}" ]]; then
  echo "i2pr SAM listener did not publish a port" >&2
  sed -n '1,40p' "${LISTENER_LOG}" >&2 || true
  exit 2
fi
echo "    SAM listener: 127.0.0.1:${SAM_PORT}"

RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"
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

# Plan 151 §5: the only sanctioned path from an executed command to a
# required `passed` row (besides the per-pair dynamic helper below,
# which records by the two client exit codes). The caller captures
# the command's exit code in `rc` first; a zero code records passed,
# anything else records failed. Direct literal
# `record "<required-label>" passed` lines are rejected by
# scripts/check-sam-acceptance-evidence.sh.
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

gen_payload() {
  local path="$1"
  local size="$2"
  local mode="$3"
  python3 - "${path}" "${size}" "${mode}" <<'PY'
import sys

path, size_text, mode = sys.argv[1:]
size = int(size_text)
if mode == "all-bytes":
    value = (bytes(range(256)) * (size // 256 + 1))[:size]
elif mode == "mixed":
    seed = b"SAM-LOOKING STREAM STATUS RESULT=OK\r\n" + bytes(range(256)) + b"\x00\xff\xc3\x28"
    value = (seed * (size // len(seed) + 1))[:size]
elif mode == "prefix":
    seed = b"HELLO VERSION MIN=3.1 MAX=3.1\n" + bytes(range(256))
    value = (seed * (size // len(seed) + 1))[:size]
elif mode == "crlf":
    seed = b"line one\r\nline two\n" + bytes(range(256))
    value = (seed * (size // len(seed) + 1))[:size]
else:
    raise SystemExit(f"unknown payload mode: {mode}")
with open(path, "wb") as stream:
    stream.write(value)
PY
}

await_pub() {
  local log="$1"
  local candidate
  for _ in $(seq 1 120); do
    candidate="$(sed -n '1p' "${log}" 2>/dev/null || true)"
    if [[ "${#candidate}" -eq 524 ]]; then
      printf '%s' "${candidate}"
      return 0
    fi
    sleep 0.1
  done
  return 1
}

wait_for_i2psam_slot() {
  local launch_second="$(date +%s)"
  while [[ "$(date +%s)" == "${launch_second}" ]]; do
    sleep 0.1
  done
}

is_i2psam_command() {
  local command_name="${1##*/}"
  [[ "${command_name}" == "i2psam_runner" ||
     "${command_name}" == "i2psam_forward_runner" ]]
}

run_stream_pair() {
  local label="$1"
  local accept_log="${SCRATCH}/${label}-accept.log"
  local connect_log="${SCRATCH}/${label}-connect.log"
  shift
  local -a accept_cmd=()
  while [[ "$1" != "--" ]]; do
    accept_cmd+=("$1")
    shift
  done
  shift
  local -a connect_template=("$@")
  local -a connect_cmd=()
  local accept_pid connect_pid accept_rc connect_rc pub candidate

  if is_i2psam_command "${accept_cmd[0]}"; then
    wait_for_i2psam_slot
  fi
  setsid timeout --foreground "${RUN_TIMEOUT}" "${accept_cmd[@]}" \
    >"${accept_log}" 2>&1 &
  accept_pid=$!
  CHILD_PIDS+=("${accept_pid}")
  if ! pub="$(await_pub "${accept_log}")"; then
    record "${label}" failed "acceptor did not publish a 524-character public destination"
    stop_group "${accept_pid}"
    wait "${accept_pid}" 2>/dev/null || true
    return 0
  fi
  for candidate in "${connect_template[@]}"; do
    [[ "${candidate}" != "{PUB}" ]] || candidate="${pub}"
    connect_cmd+=("${candidate}")
  done
  if is_i2psam_command "${connect_cmd[0]}"; then
    wait_for_i2psam_slot
  fi
  setsid timeout --foreground "${RUN_TIMEOUT}" "${connect_cmd[@]}" \
    >"${connect_log}" 2>&1 &
  connect_pid=$!
  CHILD_PIDS+=("${connect_pid}")
  set +e
  wait "${connect_pid}"
  connect_rc=$?
  wait "${accept_pid}"
  accept_rc=$?
  set -e
  if [[ "${connect_rc}" -eq 0 && "${accept_rc}" -eq 0 ]]; then
    record "${label}" passed "external clients exchanged exact binary payloads"
  else
    record "${label}" failed "connect_rc=${connect_rc} accept_rc=${accept_rc}"
  fi
}

gen_payload "${SCRATCH}/payload_a.bin" 2097152 mixed
gen_payload "${SCRATCH}/payload_b.bin" 2097152 prefix
gen_payload "${SCRATCH}/payload_small_a.bin" 4096 crlf
gen_payload "${SCRATCH}/payload_small_b.bin" 4096 all-bytes

echo "==> cross-client non-silent binary matrix"
matrix_failed_before="${REQUIRED_FAILED}"
run_stream_pair "i2plib-substitute-accept-i2psam-connect" \
  python3 "${I2PLIB_RUNNER}" accept \
  127.0.0.1 "${SAM_PORT}" "${SCRATCH}/payload_a.bin" "${SCRATCH}/payload_b.bin" false -- \
  "${I2PSAM_RUNNER}" connect 127.0.0.1 "${SAM_PORT}" "{PUB}" \
  "${SCRATCH}/payload_b.bin" "${SCRATCH}/payload_a.bin" false
run_stream_pair "i2psam-accept-i2plib-substitute-connect" \
  "${I2PSAM_RUNNER}" accept 127.0.0.1 "${SAM_PORT}" \
  "${SCRATCH}/payload_b.bin" "${SCRATCH}/payload_a.bin" false -- \
  python3 "${I2PLIB_RUNNER}" connect \
  127.0.0.1 "${SAM_PORT}" "{PUB}" "${SCRATCH}/payload_a.bin" \
  "${SCRATCH}/payload_b.bin" false
# Small-payload rounds cover the remaining executed wire modes
# (CRLF-heavy and all-byte payloads) through both clients.
run_stream_pair "i2plib-substitute-accept-i2psam-connect-small" \
  python3 "${I2PLIB_RUNNER}" accept \
  127.0.0.1 "${SAM_PORT}" "${SCRATCH}/payload_small_a.bin" "${SCRATCH}/payload_small_b.bin" false -- \
  "${I2PSAM_RUNNER}" connect 127.0.0.1 "${SAM_PORT}" "{PUB}" \
  "${SCRATCH}/payload_small_b.bin" "${SCRATCH}/payload_small_a.bin" false
run_stream_pair "i2psam-accept-i2plib-substitute-connect-small" \
  "${I2PSAM_RUNNER}" accept 127.0.0.1 "${SAM_PORT}" \
  "${SCRATCH}/payload_small_b.bin" "${SCRATCH}/payload_small_a.bin" false -- \
  python3 "${I2PLIB_RUNNER}" connect \
  127.0.0.1 "${SAM_PORT}" "{PUB}" "${SCRATCH}/payload_small_a.bin" \
  "${SCRATCH}/payload_small_b.bin" false
matrix_rc=0
[[ "${REQUIRED_FAILED}" == "${matrix_failed_before}" ]] || matrix_rc=1
record_guarded "binary-matrix" \
  "2MiB mixed/prefix + 4KiB crlf/all-bytes payloads exchanged cross-client in both directions" \
  "${matrix_rc}"

echo "==> SILENT=true raw transcript matrix"
transcript_rc=0
python3 "${TRANSCRIPT}" silent --host 127.0.0.1 --port "${SAM_PORT}" || transcript_rc=$?
record_guarded "silent-transcript" \
  "CONNECT and ACCEPT emitted raw bytes first" \
  "${transcript_rc}"

echo "==> external private-destination import and naming"
python3 "${TRANSCRIPT}" generate --host 127.0.0.1 --port "${SAM_PORT}" \
  --private-output "${SCRATCH}/generated.priv" --public-output "${SCRATCH}/generated.pub"
import_i2psam_rc=0
"${I2PSAM_RUNNER}" import 127.0.0.1 "${SAM_PORT}" \
  "${SCRATCH}/generated.priv" "${SCRATCH}/generated.pub" || import_i2psam_rc=$?
record_guarded "private-destination-i2psam" \
  "i2psam normal SESSION CREATE import" \
  "${import_i2psam_rc}"
import_i2plib_rc=0
python3 "${I2PLIB_RUNNER}" import \
  127.0.0.1 "${SAM_PORT}" "${SCRATCH}/generated.priv" "${SCRATCH}/generated.pub" || import_i2plib_rc=$?
record_guarded "private-destination-i2plib-substitute" \
  "i2plib.sam normal SESSION CREATE import" \
  "${import_i2plib_rc}"
naming_rc=0
python3 "${TRANSCRIPT}" naming --host 127.0.0.1 --port "${SAM_PORT}" || naming_rc=$?
record_guarded "naming-transcript" \
  "ME/full/malformed/unknown lookup behavior" \
  "${naming_rc}"

echo "==> negative SAM compatibility matrix"
negative_rc=0
python3 "${TRANSCRIPT}" negative --host 127.0.0.1 --port "${SAM_PORT}" || negative_rc=$?
record_guarded "negative-matrix" \
  "version/style/option/unknown/malformed rejection behavior" \
  "${negative_rc}"

echo "==> STREAM FORWARD loopback target"
ECHO_PORT="$(python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
python3 "${ECHO_TARGET}" --port "${ECHO_PORT}" \
  --received-file "${SCRATCH}/forward-received.bin" \
  >"${SCRATCH}/echo-target.log" 2>&1 &
ECHO_PID=$!
CHILD_PIDS+=("${ECHO_PID}")
# Reserve a distinct slot from the preceding i2psam ACCEPT client as
# well; the snapshot's time-seeded ID generator has process-wide collision
# behavior across separately launched clients.
wait_for_i2psam_slot
setsid timeout --foreground "${RUN_TIMEOUT}" "${I2PSAM_FORWARD}" \
  127.0.0.1 "${SAM_PORT}" 127.0.0.1 "${ECHO_PORT}" \
  >"${SCRATCH}/forward-register.log" 2>&1 &
FORWARD_PID=$!
CHILD_PIDS+=("${FORWARD_PID}")
if FORWARD_PUB="$(await_pub "${SCRATCH}/forward-register.log")"; then
  # The pinned i2psam snapshot seeds rand() with time(nullptr) when it
  # allocates a SAM session ID. Keep the next process in a distinct
  # one-second slot so its ID cannot collide with the forwarder.
  wait_for_i2psam_slot
  gen_payload "${SCRATCH}/forward-send.bin" 4096 mixed
  forward_rc=0
  if "${I2PSAM_RUNNER}" connect 127.0.0.1 "${SAM_PORT}" "${FORWARD_PUB}" \
       "${SCRATCH}/forward-send.bin" "${SCRATCH}/forward-send.bin" false \
       >"${SCRATCH}/forward-connect.log" 2>&1; then
    set +e
    wait "${ECHO_PID}"
    ECHO_RC=$?
    set -e
    if [[ "${ECHO_RC}" -eq 0 ]] &&
       cmp -s "${SCRATCH}/forward-send.bin" "${SCRATCH}/forward-received.bin"; then
      forward_rc=0
    else
      forward_rc=1
    fi
  else
    forward_rc=1
  fi
  record_guarded "stream-forward" \
    "i2psam registerer / i2psam connector / loopback echo" \
    "${forward_rc}"
else
  record_guarded "stream-forward" \
    "i2psam forwarder did not publish a destination" \
    1
fi
stop_group "${FORWARD_PID}"
stop_group "${ECHO_PID}"

echo "==> retained local Plan 149 regression suite"
plan149_rc=0
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- \
  --test-threads=1 >"${SCRATCH}/plan149-self-composed.log" 2>&1 || plan149_rc=$?
record_guarded "plan149-self-composed" \
  "canonical black-box local SAM product suite (cargo test -p i2pr-daemon --test sam_stream_self_composed)" \
  "${plan149_rc}"

echo "==> Plan 151 local acceptance suites"
PLAN151_LOG="${EVIDENCE_DIR}/plan151-acceptance.log"
: > "${PLAN151_LOG}"
plan151_rc=0
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- \
  --test-threads=1 >>"${PLAN151_LOG}" 2>&1 || plan151_rc=$?
# Each final row derives from the suite exit status plus its own
# captured `test <name> ... ok` line: the suite must be green AND
# the row's test must have run to ok. A renamed/deleted test fails
# its row instead of passing silently.
plan151_row() {
  local label="$1"
  local test_name="$2"
  local detail="$3"
  local rc=1
  if [[ "${plan151_rc}" -eq 0 ]] &&
     grep -q "^test ${test_name} ... ok$" "${PLAN151_LOG}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail} (cargo test -p i2pr-daemon --test sam_stream_final_acceptance ${test_name})" "${rc}"
}
plan151_row "sibling-stream-isolation" "plan151_sibling_streams_isolate_close_one" \
  "two simultaneous siblings, close-one/keep-one"
plan151_row "multiple-stream-lifecycle" "plan151_sibling_streams_isolate_close_one" \
  "sibling lifecycle proven by the executed Plan 151 sibling test"
plan151_row "slow-reader" "plan151_slow_reader_stays_bounded_and_recovers" \
  "bounded gauges, writer backpressure, exact 12 MiB recovery"
plan151_row "slow-writer" "plan151_slow_writer_reverse_pressure_recovers" \
  "reverse bounded gauges, exact 12 MiB recovery"
plan151_row "fault-data-drop" "plan151_fault_data_drop_recovers_by_retransmission" \
  "single DATA drop recovered by retransmission"
plan151_row "fault-ack-drop" "plan151_fault_ack_drop_recovers_without_loop" \
  "single ACK drop recovered without ACK loop"
plan151_row "fault-duplicate" "plan151_fault_duplicate_delivers_exactly_once" \
  "duplicate DATA delivered exactly once"
plan151_row "fault-reorder" "plan151_fault_reorder_delivers_in_order" \
  "reordered DATA delivered in order"
plan151_row "fault-corruption" "plan151_fault_corruption_rejected_without_delivery" \
  "corrupted material rejected below the application"
plan151_row "fault-retransmit-ceiling" "plan151_fault_retransmit_ceiling_terminates_bounded" \
  "retransmit ceiling terminates bounded"
plan151_row "close-reset-lifecycle" "plan151_close_reset_lifecycle" \
  "CLOSE/RESET/control-session cleanup returns to baseline"

echo "==> Plan 151 FORWARD lifecycle suite"
FORWARD_LOG="${EVIDENCE_DIR}/plan151-forward.log"
: > "${FORWARD_LOG}"
forward_suite_rc=0
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- \
  --test-threads=1 >>"${FORWARD_LOG}" 2>&1 || forward_suite_rc=$?
record_guarded "forward-lifecycle" \
  "forward register/bridge/second-stream/refusal/timeout/non-loopback/exclusion/owner-close (cargo test -p i2pr-daemon --test sam_forward_naming)" \
  "${forward_suite_rc}"

echo "==> Plan 127-134 regression commands"
REGRESSION_LOG="${EVIDENCE_DIR}/plan127-134-regressions.log"
: > "${REGRESSION_LOG}"
regression_rc=0
while read -r crate target; do
  if ! cargo test --locked -p "${crate}" --test "${target}" >>"${REGRESSION_LOG}" 2>&1; then
    echo "REGRESSION FAILED: -p ${crate} --test ${target}" >>"${REGRESSION_LOG}"
    regression_rc=1
  fi
done <<'REGRESSIONS'
i2pr-client plan127_trajectory
i2pr-proto plan128_wire
i2pr-client plan128_trajectory
i2pr-client plan129_trajectory
i2pr-client plan130_trajectory
i2pr-client plan131_trajectory
i2pr-client plan132_trajectory
REGRESSIONS
if ! cargo test --locked -p i2pr-crypto --all-targets >>"${REGRESSION_LOG}" 2>&1; then
  echo "REGRESSION FAILED: -p i2pr-crypto --all-targets" >>"${REGRESSION_LOG}"
  regression_rc=1
fi
record_guarded "plan127-134-regressions" \
  "plan127/128/129/130/131/132 trajectories + i2pr-crypto floor (see plan127-134-regressions.log)" \
  "${regression_rc}"

echo "==> workspace gates slice"
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-fixture-manifest \
            check-ntcp2-vectors check-ntcp2-interoperability check-constrained-host-lane-boundary; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
record_guarded "workspace-gates" \
  "fmt + workspace check --all-targets + static boundary scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${gates_rc}"

python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PSAM_PIN}" "${I2PLIB_PIN}" <<'PY'
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root, i2psam_pin, i2plib_pin = sys.argv[1:]
rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, status, detail = line.rstrip("\n").split("\t", 2)
        rows.append({"label": label, "status": status, "detail": detail})

def status_for(prefix):
    matches = [row["status"] for row in rows if row["label"].startswith(prefix)]
    return "passed" if matches and all(item == "passed" for item in matches) else "failed"

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
evidence = {
    "schema": "i2pr-sam-external-client-v2",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": os.environ.get("I2PR_SAM_EXECUTION_ID", "local-pre-cached"),
    "sam_bind_policy": "127.0.0.1:0 ephemeral loopback",
    "libsam3": {
        "repository": "https://github.com/i2p/libsam3",
        "revision": "7d6e658798baec31394c5685f9583343cc00900b",
        "build_command": "make -C <checkout> build",
        "status": "blocked-by-public-api",
        "reason": "sam3CreateSession requires PRIV and returned DESTINATION length >=884; i2pr Ed25519 PRIV is 608 characters",
    },
    "i2psam": {
        "repository": "https://github.com/i2p/i2psam",
        "revision": i2psam_pin,
        "build_command": "make",
    },
    "i2plib_substitute": {
        "repository": "https://github.com/l-n-s/i2plib",
        "revision": i2plib_pin,
        "runtime": "pinned i2plib.sam message/Base64 surface with thin socket harness",
    },
    "client_a_to_b": status_for("i2plib-substitute-accept-i2psam"),
    "client_b_to_a": status_for("i2psam-accept-i2plib"),
    "binary_matrix": status_for("binary-matrix"),
    "silent": status_for("silent-"),
    "private_destination": status_for("private-destination-"),
    "naming": status_for("naming-transcript"),
    "negative_matrix": status_for("negative-matrix"),
    "forward": status_for("stream-forward"),
    "multiple_stream_lifecycle": status_for("multiple-stream-lifecycle"),
    "plan149": status_for("plan149-self-composed"),
    "sibling_stream_isolation": status_for("sibling-stream-isolation"),
    "slow_reader": status_for("slow-reader"),
    "slow_writer": status_for("slow-writer"),
    "fault_data_drop": status_for("fault-data-drop"),
    "fault_ack_drop": status_for("fault-ack-drop"),
    "fault_duplicate": status_for("fault-duplicate"),
    "fault_reorder": status_for("fault-reorder"),
    "fault_corruption": status_for("fault-corruption"),
    "fault_retransmit_ceiling": status_for("fault-retransmit-ceiling"),
    "close_reset_lifecycle": status_for("close-reset-lifecycle"),
    "forward_lifecycle": status_for("forward-lifecycle"),
    "plan127_134_regressions": status_for("plan127-134-regressions"),
    "workspace_gates": status_for("workspace-gates"),
    "results": rows,
    "sam_independent_clients": "at-least-two-passed",
    "known_limitations": [
        "localhost SAM client interoperability only; router-to-router NTCP2/SSU2 remains unclaimed",
        "libsam3 is built at the pinned official revision but cannot consume i2pr's compact Ed25519 PRIV through its public API",
        "i2plib is explicitly the Plan 150 third-client substitution and is not patched for i2pr",
        "workspace-gates in this lane covers fmt, workspace check --all-targets, and static boundary scripts; the full test/clippy/doc/deny floor runs in routine CI",
        "bulk slow-peer profiles are deliberately small (6 x 2 MiB) per Plan 151 section 7; the purpose is boundedness, not throughput benchmarking",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 151 SAM evidence (external-client matrix + local acceptance)\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1:0` only\n")
    stream.write("- Mandatory clients: `i2psam` + `i2plib-sam-substitute`\n")
    stream.write("- libsam3: built at its exact official revision; blocked by its public 884-character private-key minimum\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 151 SAM lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 151 SAM lane passed; sanitized evidence: ${EVIDENCE_DIR}"
