#!/usr/bin/env bash
# Plan 368 bounded i2pd 2.61.0 SAM compatibility diagnostic.
# i2pd is diagnostic only; its result cannot gate the normative profile.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
SOURCE_REVISION="${I2PD_CACHE}/source-revision.txt"
I2PD_SOURCE_ROOT="${I2PR_I2PD_SOURCE_ROOT:-${REPO_ROOT}/target/interop/sam-i2pd-2.61}"
EVIDENCE_DIR="${I2PR_SAM_368_I2PD_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/sam-368-i2pd-evidence}"

if [[ ! -x "${I2PD_BIN}" || ! -f "${SOURCE_REVISION}" ]] ||
   [[ "$(<"${SOURCE_REVISION}")" != "${I2PD_PIN}" ]]; then
  echo "exact-pinned i2pd ${I2PD_VERSION} cache missing or mismatched" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh first" >&2
  exit 1
fi
if ! "${I2PD_BIN}" --version 2>&1 | grep -Fq "i2pd version ${I2PD_VERSION} "; then
  echo "i2pd binary does not report ${I2PD_VERSION}" >&2
  exit 1
fi
if [[ ! -d "${I2PD_SOURCE_ROOT}/.git" ]] ||
   [[ "$(git -C "${I2PD_SOURCE_ROOT}" rev-parse HEAD)" != "${I2PD_PIN}" ]]; then
  echo "i2pd source checkout is not the pinned revision ${I2PD_PIN}" >&2
  exit 1
fi
I2PD_SAM_SOURCE="${I2PD_SOURCE_ROOT}/libi2pd_client/SAM.cpp"
if ! grep -Fq 'else if (style == SAM_VALUE_MASTER)' "${I2PD_SAM_SOURCE}" ||
   grep -Fq 'style == SAM_VALUE_PRIMARY' "${I2PD_SAM_SOURCE}" ||
   ! grep -Fq "case '3': datagramVersion = i2p::datagram::eDatagramV3" "${I2PD_SAM_SOURCE}" ||
   ! grep -Fq '// TODO: implement other styles' "${I2PD_SAM_SOURCE}"; then
  echo "pinned i2pd SAM source diagnostics differ from the recorded Plan 368 profile" >&2
  exit 1
fi

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-sam368-i2pd.XXXXXX)"
PID=""
cleanup() {
  if [[ -n "${PID}" ]]; then
    kill "${PID}" 2>/dev/null || true
    wait "${PID}" 2>/dev/null || true
  fi
  rm -rf "${SCRATCH}"
}
trap cleanup EXIT

SAM_PORT="$(python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"

cat >"${SCRATCH}/i2pd.conf" <<EOF
host = 127.0.0.1
address4 = 127.0.0.1
port = 0
ipv6 = false
notransit = true
ntcp2.enabled = true
ssu2.enabled = false
reseed.urls =
addressbook.enabled = false
addressbook.subscriptions =
sam.enabled = true
sam.address = 127.0.0.1
sam.port = ${SAM_PORT}
sam.portudp = 0
http.enabled = false
httpproxy.enabled = false
socksproxy.enabled = false
EOF

# The only enabled transport binds to loopback; reseed and addressbook
# subscriptions are disabled, and the empty NetDB has no remote peers.
"${I2PD_BIN}" --conf="${SCRATCH}/i2pd.conf" --datadir="${SCRATCH}/data" \
  --log="${SCRATCH}/i2pd.log" --loglevel=error >/dev/null 2>&1 &
PID=$!

python3 - "${SAM_PORT}" "${I2PD_VERSION}" "${I2PD_PIN}" "${EVIDENCE_DIR}/evidence.tsv" <<'PY'
import socket
import sys
import time

port, version, pin, evidence_path = sys.argv[1:]
port = int(port)

def exchange(commands):
    with socket.create_connection(("127.0.0.1", port), timeout=5.0) as sock:
        stream = sock.makefile("rwb", buffering=0)
        replies = []
        for command in commands:
            stream.write(command.encode("ascii") + b"\n")
            reply = stream.readline(8192)
            if not reply or len(reply) > 8192:
                raise RuntimeError("missing or oversized SAM reply")
            replies.append(reply.decode("ascii", "replace").strip())
        return replies

def category(reply):
    if reply.startswith("SESSION STATUS RESULT=OK"):
        return "session_status_ok"
    if "RESULT=NOVERSION" in reply:
        return "no_common_version"
    if "NOT_IMPLEMENTED" in reply or "RESULT=I2P_ERROR" in reply:
        return "unsupported"
    if "RESULT=INVALID_STYLE" in reply:
        return "invalid_style"
    return "other_error"

deadline = time.monotonic() + 30.0
while time.monotonic() < deadline:
    try:
        hello = exchange(["HELLO VERSION MIN=1.0 MAX=3.3"])[0]
        break
    except (OSError, RuntimeError):
        time.sleep(0.1)
else:
    raise SystemExit("pinned i2pd SAM loopback listener did not become ready")

# PRIMARY is attempted only because the pinned source rejects its style before
# allocating tunnels. Keep the attempt bounded and persist only a result class.
try:
    primary_reply = exchange([
        "HELLO VERSION MIN=1.0 MAX=3.3",
        "SESSION CREATE STYLE=PRIMARY ID=plan368-primary DESTINATION=TRANSIENT",
    ])[1]
    primary_result = category(primary_reply)
except (OSError, RuntimeError, TimeoutError):
    primary_result = "session_create_timeout"

rows = {
    "i2pd_version": version,
    "i2pd_revision": pin,
    "bind_scope": "loopback_ntcp2_no_reseed_no_addressbook_peers_empty",
    "hello": "version_3_3" if "VERSION=3.3" in hello else "other",
    "primary_create": primary_result,
    "primary_master_spelling": "master_only_source",
    "session_add": "stream_only_source",
    "session_remove": "master_only_source",
    "datagram_styles": "ordinary_17_19_20_source",
    "raw_style": "ordinary_raw_source",
}
with open(evidence_path, "w", encoding="ascii") as stream:
    for key, value in rows.items():
        stream.write(f"{key}\t{value}\n")
PY

cat "${EVIDENCE_DIR}/evidence.tsv"
