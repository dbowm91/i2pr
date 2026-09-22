#!/usr/bin/env bash
# Plan 236 §6 — source-lock the exact Java Streaming response path.
#
# This is a read-only check against the exact-pinned Java I2P checkout. It
# writes only sanitized class/method facts; it never patches or builds the
# reference tree. The external lane consumes the TSV as durable evidence.

set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: $0 <java-i2p-source-root> <output-tsv>" >&2
  exit 64
fi

SOURCE_ROOT="$1"
OUTPUT="$2"
EXPECTED_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
STREAMING_ROOT="${SOURCE_ROOT}/apps/streaming/java/src/net/i2p/client/streaming/impl"
I2CP_SESSION="${SOURCE_ROOT}/core/java/src/net/i2p/client/I2PSession.java"

[[ -d "${SOURCE_ROOT}/.git" ]] || { echo "Java source is not a Git checkout" >&2; exit 1; }
[[ "$(git -C "${SOURCE_ROOT}" rev-parse HEAD)" == "${EXPECTED_PIN}" ]] || {
  echo "Java source pin mismatch" >&2
  exit 1
}

for file in \
  "${STREAMING_ROOT}/ConnectionPacketHandler.java" \
  "${STREAMING_ROOT}/Connection.java" \
  "${STREAMING_ROOT}/SchedulerReceived.java" \
  "${STREAMING_ROOT}/PacketQueue.java" \
  "${I2CP_SESSION}"; do
  [[ -f "${file}" ]] || { echo "missing pinned Java source: ${file}" >&2; exit 1; }
done

python3 - "${STREAMING_ROOT}" "${I2CP_SESSION}" "${OUTPUT}" "${EXPECTED_PIN}" <<'PY'
from pathlib import Path
import sys

streaming_root = Path(sys.argv[1])
i2cp_session = Path(sys.argv[2])
output = Path(sys.argv[3])
pin = sys.argv[4]

def read(name: str) -> str:
    return (streaming_root / name).read_text(encoding="utf-8")

packet_handler = read("ConnectionPacketHandler.java")
connection = read("Connection.java")
scheduler = read("SchedulerReceived.java")
queue = read("PacketQueue.java")
session = i2cp_session.read_text(encoding="utf-8")

required = {
    "ConnectionPacketHandler.receivePacket": (packet_handler, "void receivePacket(Packet packet, Connection con)"),
    "ConnectionPacketHandler.eventOccurred": (packet_handler, "con.eventOccurred();"),
    "Connection.eventOccurred": (connection, "void eventOccurred()"),
    "Connection.scheduler_event": (connection, "sched.eventOccurred(this);"),
    "SchedulerReceived.eventOccurred": (scheduler, "public void eventOccurred(Connection con)"),
    "SchedulerReceived.sendAvailable": (scheduler, "con.sendAvailable();"),
    "Connection.sendPacket": (connection, "void sendPacket(PacketLocal packet)"),
    "PacketQueue.enqueue": (queue, "boolean enqueue(PacketLocal packet)"),
    "PacketQueue.streaming_protocol": (queue, "I2PSession.PROTO_STREAMING"),
    "PacketQueue.boolean_send_overload": (queue, "                                 options);"),
    "I2PSession.boolean_send_overload": (session, "public boolean sendMessage(Destination dest, byte[] payload, int offset, int size,"),
}
for label, (source, needle) in required.items():
    if needle not in source:
        raise SystemExit(f"source-lock missing {label}: {needle}")

# The scheduler path is intentionally ordered. A source upgrade that moves a
# stage or changes the overload must fail the lane before an external attempt.
ordered = [
    (packet_handler, "void receivePacket(Packet packet, Connection con)"),
    (packet_handler, "con.eventOccurred();"),
    (connection, "void eventOccurred()"),
    (connection, "sched.eventOccurred(this);"),
    (scheduler, "public void eventOccurred(Connection con)"),
    (scheduler, "con.sendAvailable();"),
    (connection, "void sendPacket(PacketLocal packet)"),
    (queue, "boolean enqueue(PacketLocal packet)"),
]
for before, after in zip(ordered, ordered[1:]):
    # Cross-file ordering is represented by the protocol, while same-file
    # ordering is checked directly. This catches local source drift without
    # pretending unrelated files have a shared byte order.
    if before[0] is after[0] and before[0].find(before[1]) >= before[0].find(after[1]):
        raise SystemExit(f"source-lock order mismatch: {before[1]} before {after[1]}")

output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(
    "".join([
        f"java_source_pin\t{pin}\n",
        "java_response_scheduler_class\tnet.i2p.client.streaming.impl.SchedulerReceived\n",
        "java_response_scheduler_method\teventOccurred\n",
        "java_response_packet_kind\tACK_OR_SYN_ACK\n",
        "java_response_send_method\tConnection.sendPacket(PacketLocal)\n",
        "java_packetqueue_method\tPacketQueue.enqueue(PacketLocal)\n",
        "java_i2psession_send_method\tboolean_sendMessage_SendMessageOptions\n",
        "java_i2psession_status_listener_overload\tconditional-only\n",
    ]),
    encoding="utf-8",
)
PY

echo "Java response source lock passed: ${OUTPUT}"
