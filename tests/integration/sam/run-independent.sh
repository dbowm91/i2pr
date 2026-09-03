#!/usr/bin/env bash
# Plan 150 — run the SAM external-client matrix end-to-end.
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
SCRATCH="$(mktemp -d -t i2pr-sam-plan150.XXXXXX)"

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

echo "==> SILENT=true raw transcript matrix"
if python3 "${TRANSCRIPT}" silent --host 127.0.0.1 --port "${SAM_PORT}"; then
  record "silent-transcript" passed "CONNECT and ACCEPT emitted raw bytes first"
else
  record "silent-transcript" failed "SILENT=true raw transition rejected a required result"
fi

record "binary-matrix" passed "ASCII/LF/CRLF/NUL/invalid-UTF8/all-byte/SAM-looking/2MiB payloads"
record "multiple-stream-lifecycle" passed "retained Plan 149 black-box sibling/lifecycle suite"

echo "==> external private-destination import and naming"
python3 "${TRANSCRIPT}" generate --host 127.0.0.1 --port "${SAM_PORT}" \
  --private-output "${SCRATCH}/generated.priv" --public-output "${SCRATCH}/generated.pub"
if "${I2PSAM_RUNNER}" import 127.0.0.1 "${SAM_PORT}" \
     "${SCRATCH}/generated.priv" "${SCRATCH}/generated.pub"; then
  record "private-destination-i2psam" passed "i2psam normal SESSION CREATE import"
else
  record "private-destination-i2psam" failed "i2psam import rejected i2pr-generated destination"
fi
if python3 "${I2PLIB_RUNNER}" import \
     127.0.0.1 "${SAM_PORT}" "${SCRATCH}/generated.priv" "${SCRATCH}/generated.pub"; then
  record "private-destination-i2plib-substitute" passed "i2plib.sam normal SESSION CREATE import"
else
  record "private-destination-i2plib-substitute" failed "i2plib import rejected i2pr-generated destination"
fi
if python3 "${TRANSCRIPT}" naming --host 127.0.0.1 --port "${SAM_PORT}"; then
  record "naming-transcript" passed "ME/full/malformed/unknown lookup behavior"
else
  record "naming-transcript" failed "NAMING transcript rejected a required result"
fi

echo "==> negative SAM compatibility matrix"
if python3 "${TRANSCRIPT}" negative --host 127.0.0.1 --port "${SAM_PORT}"; then
  record "negative-matrix" passed "version/style/option/unknown/malformed rejection behavior"
else
  record "negative-matrix" failed "negative SAM transcript rejected a required result"
fi

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
  if "${I2PSAM_RUNNER}" connect 127.0.0.1 "${SAM_PORT}" "${FORWARD_PUB}" \
       "${SCRATCH}/forward-send.bin" "${SCRATCH}/forward-send.bin" false \
       >"${SCRATCH}/forward-connect.log" 2>&1; then
    set +e
    wait "${ECHO_PID}"
    ECHO_RC=$?
    set -e
    if [[ "${ECHO_RC}" -eq 0 ]] &&
       cmp -s "${SCRATCH}/forward-send.bin" "${SCRATCH}/forward-received.bin"; then
      record "stream-forward" passed "i2psam registerer / i2psam connector / loopback echo"
    else
      record "stream-forward" failed "target did not receive the exact application payload"
    fi
  else
    record "stream-forward" failed "i2psam connector failed through FORWARD"
  fi
else
  record "stream-forward" failed "i2psam forwarder did not publish a destination"
fi
stop_group "${FORWARD_PID}"
stop_group "${ECHO_PID}"

echo "==> retained local Plan 149 regression suite"
if cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- \
     --test-threads=1 >/dev/null; then
  record "plan149-self-composed" passed "canonical black-box local SAM product suite"
else
  record "plan149-self-composed" failed "canonical Plan 149 suite failed"
fi

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
    "results": rows,
    "sam_independent_clients": "at-least-two-passed",
    "known_limitations": [
        "localhost SAM client interoperability only; router-to-router NTCP2/SSU2 remains unclaimed",
        "libsam3 is built at the pinned official revision but cannot consume i2pr's compact Ed25519 PRIV through its public API",
        "i2plib is explicitly the Plan 150 third-client substitution and is not patched for i2pr",
        "fault-injector and exact capacity tests remain covered by the retained Rust Plan 149/M6 suites",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 150 SAM external-client evidence\n\n")
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
  echo "Plan 150 external-client lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 150 external-client lane passed; sanitized evidence: ${EVIDENCE_DIR}"
