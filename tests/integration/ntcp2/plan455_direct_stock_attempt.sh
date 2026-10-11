#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
pin="635b013a612ff47278ef02acf8580a28e10e26c5"
i2pd_bin="${I2PR_PLAN455_I2PD_BIN:-${repo_root}/target/interop/cache/ssu2/i2pd/${pin}/bin/i2pd}"

if [[ ! -x "${i2pd_bin}" ]]; then
  printf '%s\n' 'plan455: blocked; exact-pinned i2pd binary is unavailable' >&2
  exit 2
fi
if [[ "$(cat "$(dirname "$(dirname "${i2pd_bin}")")/source-revision.txt")" != "${pin}" ]]; then
  printf '%s\n' 'plan455: blocked; i2pd source revision does not match the frozen pin' >&2
  exit 2
fi
if ! "${i2pd_bin}" --version 2>&1 | rg -q 'i2pd version 2\.61\.0'; then
  printf '%s\n' 'plan455: blocked; i2pd binary version does not match the frozen pin' >&2
  exit 2
fi

cargo build --locked -p i2pr-interop --bin i2pr-interop
interop="${repo_root}/target/debug/i2pr-interop"
scratch="$(mktemp -d)"
chmod 700 "${scratch}"
stock_pid=

cleanup() {
  if [[ -n "${stock_pid}" ]]; then
    if kill -TERM "${stock_pid}" 2>/dev/null; then :; fi
    if wait "${stock_pid}" 2>/dev/null; then :; fi
  fi
  rm -rf "${scratch}"
}
trap cleanup EXIT

stock_port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
local_port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
mkdir -m 700 "${scratch}/stock" "${scratch}/run" "${scratch}/run/peer"

cat > "${scratch}/stock.conf" <<EOF
daemon = false
loglevel = debug
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = 0
ipv4 = true
ipv6 = false
nat = false
notransit = true
floodfill = false
reservedrange = false
bandwidth = L
[ssu2]
enabled = false
published = false
[ntcp2]
enabled = true
published = true
port = ${stock_port}
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

"${i2pd_bin}" "--conf=${scratch}/stock.conf" "--datadir=${scratch}/stock" \
  --log=file "--logfile=${scratch}/stock.log" >/dev/null 2>&1 &
stock_pid=$!
for _ in $(seq 1 60); do
  if [[ -s "${scratch}/stock/router.info" ]] &&
    rg -q "NTCP2: Start listening v4 TCP port ${stock_port}" "${scratch}/stock.log"; then
    break
  fi
  if ! kill -0 "${stock_pid}" 2>/dev/null; then
    printf '%s\n' 'plan455: blocked; pristine stock process exited before listener readiness' >&2
    exit 2
  fi
  sleep 1
done
if [[ ! -s "${scratch}/stock/router.info" ]] ||
  ! rg -q "NTCP2: Start listening v4 TCP port ${stock_port}" "${scratch}/stock.log"; then
  printf '%s\n' 'plan455: blocked; pristine stock NTCP2 listener did not become ready' >&2
  exit 2
fi

cp "${scratch}/stock/router.info" "${scratch}/run/peer/router.info"
prepared="$("${interop}" ntcp2 prepare --state-dir "${scratch}/run/state" \
  --local-address 127.0.0.1 --local-port "${local_port}" --network-id 2 \
  --topology-kind current-network-loopback)"
local_hash="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["router_hash_sha256"])' <<<"${prepared}")"
peer_hash_result="$("${interop}" ntcp2 router-hash --router-info "${scratch}/run/peer/router.info")"
peer_hash="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["router_hash_sha256"])' <<<"${peer_hash_result}")"

cat > "${scratch}/run/scenario.toml" <<EOF
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-i2pd-ipv4"
run_id = "plan455-gatea-one-shot"
role = "initiator"
address_family = "ipv4"
local_address = "127.0.0.1"
local_port = ${local_port}
network_id = 2
peer_address = "127.0.0.1"
peer_port = ${stock_port}
state_dir = "state"
peer_router_info = "peer/router.info"
handshake_deadline_ms = 30000
read_deadline_ms = 30000
write_deadline_ms = 30000
queue_deadline_ms = 5000
drain_deadline_ms = 5000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
expected_result_class = "authenticated-handshake-and-directional-data-phase"
status_path = "status.jsonl"
delivery_status_message_id = 4550101
expected_sender_router_hash_sha256 = "${local_hash}"
expected_receiver_router_hash_sha256 = "${peer_hash}"
reference_driver_mode = "i2pd-direct-driver"
run_identity_sha256 = "4554554554554554554554554554554554554554554554554554554554554554"
data_phase_mode = "initiator-data-only"
expected_observation = "i2pr-sent-only"
topology_kind = "current-network-loopback"
EOF

"${interop}" ntcp2 validate-scenario --scenario-config "${scratch}/run/scenario.toml" >/dev/null
set +e
"${interop}" ntcp2 dial --scenario-config "${scratch}/run/scenario.toml" \
  >"${scratch}/initiator.status" 2>/dev/null
initiator_exit=$?
set -e

python3 - "${scratch}/initiator.status" "${scratch}/stock.log" "${initiator_exit}" <<'PY'
import json
import pathlib
import sys

status_path, log_path, exit_code = sys.argv[1:]
records = []
for line in pathlib.Path(status_path).read_text(errors="replace").splitlines():
    try:
        row = json.loads(line)
    except json.JSONDecodeError:
        continue
    if row.get("type") == "i2pr-interop-status":
        records.append(row)
terminal = next((row for row in reversed(records) if row.get("phase") == "terminal"), {})
counters = terminal.get("counters", {})
log_lines = pathlib.Path(log_path).read_text(errors="replace").splitlines()
receiver_markers = sum(
    1 for line in log_lines
    if "DeliveryStatus" in line or "Delivery status" in line
)
print(f"plan455_initiator_exit={exit_code}")
print(f"plan455_authenticated_sessions={counters.get('authenticated', 0)}")
print(f"plan455_i2np_messages_sent={counters.get('i2np_sent', 0)}")
print(f"plan455_stock_delivery_status_log_markers={receiver_markers}")
if exit_code != 0 or counters.get("authenticated") != 1 or counters.get("i2np_sent") != 1:
    raise SystemExit(2)
if receiver_markers == 0:
    raise SystemExit(3)
PY
