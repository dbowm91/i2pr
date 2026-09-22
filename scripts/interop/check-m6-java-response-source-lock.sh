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
ROUTER_CLIENT="${SOURCE_ROOT}/router/java/src/net/i2p/router/client/ClientMessageEventListener.java"
ROUTER_OCMOSJ="${SOURCE_ROOT}/router/java/src/net/i2p/router/message/OutboundClientMessageOneShotJob.java"
ROUTER_POOL="${SOURCE_ROOT}/router/java/src/net/i2p/router/ClientMessagePool.java"
ROUTER_DISPATCHER="${SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/TunnelDispatcher.java"

[[ -d "${SOURCE_ROOT}/.git" ]] || { echo "Java source is not a Git checkout" >&2; exit 1; }
[[ "$(git -C "${SOURCE_ROOT}" rev-parse HEAD)" == "${EXPECTED_PIN}" ]] || {
  echo "Java source pin mismatch" >&2
  exit 1
}

for file in \
  "${STREAMING_ROOT}/ConnectionPacketHandler.java" \
  "${STREAMING_ROOT}/Connection.java" \
  "${STREAMING_ROOT}/SchedulerReceived.java" \
  "${STREAMING_ROOT}/SchedulerImpl.java" \
  "${STREAMING_ROOT}/PacketQueue.java" \
  "${I2CP_SESSION}" \
  "${ROUTER_CLIENT}" \
  "${ROUTER_OCMOSJ}" \
  "${ROUTER_POOL}" \
  "${ROUTER_DISPATCHER}"; do
  [[ -f "${file}" ]] || { echo "missing pinned Java source: ${file}" >&2; exit 1; }
done

python3 - "${STREAMING_ROOT}" "${I2CP_SESSION}" "${OUTPUT}" "${EXPECTED_PIN}" "${ROUTER_CLIENT}" "${ROUTER_OCMOSJ}" "${ROUTER_POOL}" "${ROUTER_DISPATCHER}" <<'PY'
from pathlib import Path
import sys

streaming_root = Path(sys.argv[1])
i2cp_session = Path(sys.argv[2])
output = Path(sys.argv[3])
pin = sys.argv[4]
router_client = Path(sys.argv[5]).read_text(encoding="utf-8")
router_ocmosj = Path(sys.argv[6]).read_text(encoding="utf-8")
router_pool = Path(sys.argv[7]).read_text(encoding="utf-8")
router_dispatcher = Path(sys.argv[8]).read_text(encoding="utf-8")

def read(name: str) -> str:
    return (streaming_root / name).read_text(encoding="utf-8")

packet_handler = read("ConnectionPacketHandler.java")
connection = read("Connection.java")
scheduler = read("SchedulerReceived.java")
scheduler_impl = read("SchedulerImpl.java")
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
    # Plan 237 §4 — exact-pinned stock observability signals the
    # helper-JVM observer depends on. A source upgrade that renames any
    # of these must fail the lane before an external attempt.
    "SchedulerReceived.send_branch_log": (scheduler, "received con... send a packet"),
    "SchedulerReceived.reschedule_branch_log": (scheduler, "received con... time till next send: "),
    # Corrective (counted attempt 1): SchedulerReceived events log via
    # the SchedulerImpl logger, and ackImmediately ("sending new ack")
    # fires only on dup/fast-ack paths, never on the fresh SYN-ACK path.
    # The active construction signal is the sendPacket timer log below.
    "SchedulerImpl.scheduler_logger": (scheduler_impl, "getLog(SchedulerImpl.class)"),
    "Connection.sendpacket_construction_log": (connection, '" Resend in "'),
    "PacketQueue.sendmessage_stat": (queue, '"stream.con.sendMessageSize"'),
    "PacketQueue.send_exception_log": (queue, "Unable to send the packet"),
    "PacketQueue.send_failed_log": (queue, "Send failed for "),
    "PacketQueue.slow_send_log": (queue, "ms to sendMessage(...)"),
    # Plan 238 §6 — exact-pinned Router-A I2CP-admission signals the
    # P238 observer counts. `client.distributeTime` is added on every
    # `handleSendMessage` after `distributeMessage` (first Router-A
    # stage, works for best-effort Streaming); `client.dispatchTime` /
    # `client.dispatchSendTime` are added in the OCMOSJ dispatch path
    # after `dispatchOutbound`; `tunnel.dispatchOutboundTunnel` is the
    # tunnel-handoff context. A source upgrade that renames any of
    # these must fail the lane before an external attempt.
    "ClientMessageEventListener.handleSendMessage": (router_client, "void handleSendMessage(SendMessageMessage message)"),
    "ClientMessageEventListener.distribute_stat": (router_client, '"client.distributeTime"'),
    "ClientMessagePool.ocmosj_init": (router_pool, "OutboundClientMessageOneShotJob.init"),
    "OCMOSJ.dispatch_stat": (router_ocmosj, '"client.dispatchTime"'),
    "OCMOSJ.dispatch_send_stat": (router_ocmosj, '"client.dispatchSendTime"'),
    "OCMOSJ.dispatch_outbound_call": (router_ocmosj, "dispatchOutbound("),
    "TunnelDispatcher.dispatch_outbound_stat": (router_dispatcher, '"tunnel.dispatchOutboundTunnel"'),
    # Plan 239 §3/§9 — exact-pinned OCMOSJ pre-dispatch / dispatch
    # ordering the P239 observer counts. The constructor performs a
    # local `lookupLeaseSetLocally(toHash)` before any remote lookup,
    # so a zero `leaseSetFoundRemoteTime` delta never proves no target
    # LS. Remote success/failure record `leaseSetFoundRemoteTime` /
    # `leaseSetFailedRemoteTime`; tunnel/garlic preparation failures
    # record `dispatchNoTunnels` on two distinct stock branches;
    # successful preparation runs `DispatchJob` inline via
    # `tunnelDispatcher().dispatchOutbound(...)`, then records
    # `dispatchTime` / `dispatchSendTime`, and returns to `send()` to
    # record `dispatchPrepareTime`. A source upgrade that renames any
    # of these must fail the lane before an external attempt.
    "OCMOSJ.constructor_local_lookup": (router_ocmosj, "ctx.clientNetDb(_from.calculateHash()).lookupLeaseSetLocally(toHash)"),
    "OCMOSJ.lease_found_remote_stat": (router_ocmosj, '"client.leaseSetFoundRemoteTime"'),
    "OCMOSJ.lease_failed_remote_stat": (router_ocmosj, '"client.leaseSetFailedRemoteTime"'),
    "OCMOSJ.dispatch_no_tunnels_stat": (router_ocmosj, '"client.dispatchNoTunnels"'),
    "OCMOSJ.no_outbound_tunnel_log": (router_ocmosj, "Could not find any outbound tunnels to send the payload through"),
    "OCMOSJ.garlic_no_tunnel_log": (router_ocmosj, "Unable to create the garlic message (no tunnels left or too lagged)"),
    "OCMOSJ.dispatch_prepare_stat": (router_ocmosj, '"client.dispatchPrepareTime"'),
    "OCMOSJ.dispatch_outbound_full": (router_ocmosj, "tunnelDispatcher().dispatchOutbound"),
    # Plan 239 §3.3/§5 — exact-pinned local-LS rejection logs the
    # P239-DISPATCH log counters distinguish. `getNextLease()` warns
    # when the constructor/local lookup left no LS, when only an
    # unacceptable received-as-published LS is present, and when the
    # selected lease cannot be sent (failure code path covering
    # bad/unsupported/encryption-key cases); empty lease lists log
    # `No leases found`. These are source facts only; execution raw
    # log lines never enter durable evidence.
    "OCMOSJ.local_ls_missing_log": (router_ocmosj, "Lookup locally didn\'t find the leaseSet for "),
    "OCMOSJ.only_rap_ls_log": (router_ocmosj, "Only have RAP LS for "),
    "OCMOSJ.lease_send_failure_log": (router_ocmosj, "Got the lease but can\'t send to it, failure code "),
    "OCMOSJ.no_leases_log": (router_ocmosj, "No leases found from: "),
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
        # Plan 237 §4 — pinned stock signals the helper observer counts.
        # Corrective: scheduler events log via SchedulerImpl; response
        # construction is the active sendPacket timer log (ackImmediately
        # fires only on dup/fast-ack paths, never on fresh SYN-ACK).
        "java_scheduler_log_signals\treceived con... send a packet | received con... time till next send: <n>\n",
        "java_scheduler_logger_class\tnet.i2p.client.streaming.impl.SchedulerImpl\n",
        "java_sendpacket_log_signal\tResend in <timeout> for <packet> (Connection.sendPacket first-send timer)\n",
        "java_sendmessage_stat\tstream.con.sendMessageSize\n",
        "java_send_failure_signals\tUnable to send the packet | Send failed for <PacketLocal> | Took <n>ms to sendMessage(...)\n",
        # Plan 238 §6 — pinned Router-A admission signals the P238
        # observer counts (I2CP admission first, dispatch second,
        # tunnel handoff as context only).
        "java_router_distribute_stat\tclient.distributeTime (ClientMessageEventListener.handleSendMessage after distributeMessage)\n",
        "java_router_dispatch_stats\tclient.dispatchTime | client.dispatchSendTime (OCMOSJ dispatch path after dispatchOutbound)\n",
        "java_router_tunnel_handoff_stat\ttunnel.dispatchOutboundTunnel (TunnelDispatcher context only)\n",
        # Plan 239 §3/§9 — pinned OCMOSJ pre-dispatch / dispatch
        # ordering the P239-DISPATCH observer counts. Retained
        # Plan-236/237/238 rows above stay frozen; these rows are
        # additive. Log signals are source facts only; execution raw
        # log lines never enter durable evidence.
        "java_ocmosj_constructor_local_lookup\tctx.clientNetDb(_from.calculateHash()).lookupLeaseSetLocally(toHash) (OCMOSJ constructor, before runJob remote lookup)\n",
        "java_lease_lookup_remote_stats\tclient.leaseSetFoundRemoteTime | client.leaseSetFailedRemoteTime (OCMOSJ remote lookup success/failure)\n",
        "java_dispatch_no_tunnels_stat\tclient.dispatchNoTunnels (OCMOSJ tunnel/garlic preparation failure, two distinct branches)\n",
        "java_no_outbound_tunnel_log\tCould not find any outbound tunnels to send the payload through (OCMOSJ selectOutboundTunnel branch)\n",
        "java_garlic_no_tunnel_log\tUnable to create the garlic message (no tunnels left or too lagged) (OCMOSJ garlic-construction branch)\n",
        "java_dispatch_prepare_stat\tclient.dispatchPrepareTime (OCMOSJ send() after inline DispatchJob returns)\n",
        "java_dispatch_outbound_call\ttunnelDispatcher().dispatchOutbound (DispatchJob.runJob inline, before dispatchTime/dispatchSendTime)\n",
        "java_local_ls_rejection_logs\tLookup locally didn't find the leaseSet for <dest> | Only have RAP LS for <dest> (getNextLease local-LS rejection)\n",
        "java_lease_send_failure_logs\tGot the lease but can't send to it, failure code <rc> | No leases found from: <ls> (getNextLease bad/unsupported/no-lease paths)\n",
    ]),
    encoding="utf-8",
)
PY

echo "Java response source lock passed: ${OUTPUT}"
