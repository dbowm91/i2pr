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
ROUTER_MESSAGE_OUTPUT="${SOURCE_ROOT}/apps/streaming/java/src/net/i2p/client/streaming/impl/MessageOutputStream.java"
ROUTER_RECEIVER="${SOURCE_ROOT}/apps/streaming/java/src/net/i2p/client/streaming/impl/ConnectionDataReceiver.java"
ROUTER_CLIENT="${SOURCE_ROOT}/router/java/src/net/i2p/router/client/ClientMessageEventListener.java"
ROUTER_OCMOSJ="${SOURCE_ROOT}/router/java/src/net/i2p/router/message/OutboundClientMessageOneShotJob.java"
ROUTER_POOL="${SOURCE_ROOT}/router/java/src/net/i2p/router/ClientMessagePool.java"
ROUTER_DISPATCHER="${SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/TunnelDispatcher.java"
ROUTER_ISJ="${SOURCE_ROOT}/router/java/src/net/i2p/router/networkdb/kademlia/IterativeSearchJob.java"
ROUTER_FPS="${SOURCE_ROOT}/router/java/src/net/i2p/router/networkdb/kademlia/FloodfillPeerSelector.java"
ROUTER_STOREJOB="${SOURCE_ROOT}/router/java/src/net/i2p/router/networkdb/kademlia/StoreJob.java"
ROUTER_TUNNEL_POOL_MANAGER="${SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/pool/TunnelPoolManager.java"
ROUTER_TUNNEL_POOL="${SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/pool/TunnelPool.java"
ROUTER_TUNNEL_PEER_SELECTOR="${SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/pool/TunnelPeerSelector.java"
ROUTER_CLIENT_PEER_SELECTOR="${SOURCE_ROOT}/router/java/src/net/i2p/router/tunnel/pool/ClientPeerSelector.java"

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
  "${STREAMING_ROOT}/MessageOutputStream.java" \
  "${STREAMING_ROOT}/ConnectionDataReceiver.java" \
  "${I2CP_SESSION}" \
   "${ROUTER_CLIENT}" \
   "${ROUTER_OCMOSJ}" \
   "${ROUTER_POOL}" \
   "${ROUTER_DISPATCHER}" \
   "${ROUTER_ISJ}" \
   "${ROUTER_FPS}" \
   "${ROUTER_STOREJOB}"; do
  [[ -f "${file}" ]] || { echo "missing pinned Java source: ${file}" >&2; exit 1; }
done

for file in \
   "${ROUTER_TUNNEL_POOL_MANAGER}" \
   "${ROUTER_TUNNEL_POOL}" \
   "${ROUTER_TUNNEL_PEER_SELECTOR}" \
   "${ROUTER_CLIENT_PEER_SELECTOR}"; do
  [[ -f "${file}" ]] || { echo "missing pinned Java source: ${file}" >&2; exit 1; }
done

python3 - "${STREAMING_ROOT}" "${I2CP_SESSION}" "${OUTPUT}" "${EXPECTED_PIN}" "${ROUTER_CLIENT}" "${ROUTER_OCMOSJ}" "${ROUTER_POOL}" "${ROUTER_DISPATCHER}" "${ROUTER_ISJ}" "${ROUTER_FPS}" "${ROUTER_STOREJOB}" "${ROUTER_TUNNEL_POOL_MANAGER}" "${ROUTER_TUNNEL_POOL}" "${ROUTER_TUNNEL_PEER_SELECTOR}" "${ROUTER_CLIENT_PEER_SELECTOR}" "${ROUTER_MESSAGE_OUTPUT}" "${ROUTER_RECEIVER}" <<'PY'
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
router_isj = Path(sys.argv[9]).read_text(encoding="utf-8")
router_fps = Path(sys.argv[10]).read_text(encoding="utf-8")
router_storejob = Path(sys.argv[11]).read_text(encoding="utf-8")
router_tunnel_pool_manager = Path(sys.argv[12]).read_text(encoding="utf-8")
router_tunnel_pool = Path(sys.argv[13]).read_text(encoding="utf-8")
router_tunnel_peer_selector = Path(sys.argv[14]).read_text(encoding="utf-8")
router_client_peer_selector = Path(sys.argv[15]).read_text(encoding="utf-8")
message_output_stream = Path(sys.argv[16]).read_text(encoding="utf-8")
receiver = Path(sys.argv[17]).read_text(encoding="utf-8")

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
    # Plan 240 §4/§14 — exact-pinned streaming-epoch lookup sequence
    # the P240 observer attributes. `runJob()` checks negative cache
    # before selection, selects floodfill peers through the main DB
    # selector, removes self/target, registers the job, logs the
    # source-locked `New ISJ ... toTry:` row, then `retry()` picks in
    # routing-key order with IP-close diversity before `sendQuery()`
    # applies the pre-dispatch guards (old-router, outbound/client
    # tunnels, reply-encryption, zero-hop, encrypted-prep) and only
    # then emits `ISJ try ...` / `Encrypted DLM for ...`. A source
    # upgrade that renames any of these must fail the lane before an
    # external attempt.
    "ISJ.negative_cache_check": (router_isj, "isNegativeCached(_key)"),
    "ISJ.negative_cached_log": (router_isj, "Negative cached, not searching: "),
    "ISJ.select_floodfill": (router_isj, "selectFloodfillParticipants(_rkey, _totalSearchLimit + EXTRA_PEERS, ks)"),
    "ISJ.new_isj_totry": (router_isj, "New ISJ for "),
    "ISJ.new_isj_totry_list": (router_isj, "toTry: "),
    "ISJ.ip_close_skip": (router_isj, "Skipping query w/ router too close to others "),
    "ISJ.should_store_to": (router_isj, "StoreJob.shouldStoreTo(ri)"),
    "ISJ.old_router_log": (router_isj, "not sending query to old router: "),
    "ISJ.no_ib_client_tunnel_log": (router_isj, " failed, no IB client tunnel to receive reply"),
    "ISJ.no_ratchet_elg_log": (router_isj, " skipped, no ratchet/elg support"),
    "ISJ.zero_hop_self_log": (router_isj, "not doing zero-hop self-lookup of "),
    "ISJ.zero_hop_unknown_log": (router_isj, "not doing zero-hop lookup to unknown "),
    "ISJ.try_log": (router_isj, "ISJ try "),
    "ISJ.encrypted_dlm": (router_isj, "Encrypted DLM for "),
    "FPS.capability_source": (router_fps, "getPeersByCapability(FloodfillNetworkDatabaseFacade.CAPABILITY_FLOODFILL)"),
    "FPS.banlist_forever": (router_fps, "isBanlistedForever(h)"),
    "FPS.same_16": (router_fps, "Same /16, family, or port: "),
    "FPS.old": (router_fps, "Old: "),
    "FPS.bad_country": (router_fps, "Bad country: "),
    "FPS.slow": (router_fps, "Slow: "),
    "FPS.bad_new": (router_fps, "Bad (new): "),
    "FPS.good": (router_fps, "Good: "),
    "FPS.ok": (router_fps, "OK: "),
    "FPS.bad_db": (router_fps, "Bad (DB): "),
    "FPS.bad_no_hist": (router_fps, "Bad (no hist): "),
    "FPS.bad_no_prof": (router_fps, "Bad (no prof): "),
    # Plan 241 §4/§15 — exact-pinned one-hop client-tunnel selection
    # the P241 lane corrects into. `selectOutboundTunnel(destination,
    # closestTo)` serves the destination client pool when one exists
    # (no exploratory fallback); `selectTunnel(closestTo)` sorts with
    # `TunnelInfoComparator(target, avoidZeroHop)`, which puts
    # zero-hop tunnels last when `allowZeroHop=false`. The
    # `sendQuery()` split reads main-NetDB RI for send preparation
    # but guards on the client-facade lookup for selected zero-hop
    # tunnels (`outTunnel.getLength() <= 1`), then dispatches via
    # `dispatchOutbound(outMsg, outTunnel.getSendTunnelId(0), peer)`.
    # A source upgrade that renames any of these must fail the lane
    # before an external attempt.
    "TunnelPoolManager.select_outbound_tunnel": (router_tunnel_pool_manager, "public TunnelInfo selectOutboundTunnel(Hash destination, Hash closestTo)"),
    "TunnelPoolManager.client_outbound_pool": (router_tunnel_pool_manager, "_clientOutboundPools.get(destination)"),
    "TunnelPoolManager.pool_select_tunnel": (router_tunnel_pool_manager, "return pool.selectTunnel(closestTo);"),
    "TunnelPool.select_tunnel_closest": (router_tunnel_pool, "TunnelInfo selectTunnel(Hash closestTo)"),
    "TunnelPool.avoid_zero_hop": (router_tunnel_pool, "boolean avoidZeroHop = !_settings.getAllowZeroHop()"),
    "TunnelPool.comparator_order": (router_tunnel_pool, "new TunnelInfoComparator(closestTo, avoidZeroHop)"),
    "TunnelPool.comparator_zero_last": (router_tunnel_pool, "if true, zero-hop tunnels will be put last"),
    "ISJ.main_netdb_ri_lookup": (router_isj, "RouterInfo ri = ctx.netDb().lookupRouterInfoLocally(peer);"),
    "ISJ.client_facade_lookup": (router_isj, "_facade.lookupLocallyWithoutValidation(peer)"),
    "ISJ.zero_hop_length_guard": (router_isj, "outTunnel.getLength() <= 1"),
    "ISJ.dispatch_outbound": (router_isj, "dispatchOutbound(outMsg, outTunnel.getSendTunnelId(0), peer)"),
    # Plan 242 §3 — exact-pinned probabilistic explicit-peer selection.
    # `TunnelPeerSelector.shouldSelectExplicit(settings)` short-circuits
    # on `isExploratory()`, reads the per-pool `explicitPeers` (or the
    # router-global fallback), and returns true only when
    # `ctx.random().nextInt(4) == 0` (the one-in-four explicit branch).
    # `TunnelPeerSelector.selectExplicit(settings, length)` is the matching
    # explicit branch; `ClientPeerSelector.selectPeers(settings)` is the
    # dispatcher that chooses between the explicit and the stock
    # fast-peer branches. Any upgrade that makes the explicit branch
    # deterministic, removes the random gate, or skips
    # `shouldSelectExplicit` must fail the lane before an external
    # attempt. A source upgrade that renames any of these must fail the
    # lane before an external attempt.
    "TPS.should_select_explicit": (router_tunnel_peer_selector, "protected boolean shouldSelectExplicit(TunnelPoolSettings settings)"),
    "TPS.explicit_peers_property": (router_tunnel_peer_selector, "String peers = opts.getProperty(\"explicitPeers\");"),
    "TPS.random_one_in_four": (router_tunnel_peer_selector, "ctx.random().nextInt(4) == 0"),
    "TPS.select_explicit": (router_tunnel_peer_selector, "protected List<Hash> selectExplicit(TunnelPoolSettings settings, int length)"),
    "TPS.random_shuffle_explicit": (router_tunnel_peer_selector, "Collections.shuffle(rv, ctx.random());"),
    "TPS.local_router_appended": (router_tunnel_peer_selector, "rv.add(ctx.routerHash());"),
    "TPS.no_valid_explicit_zero_hop": (router_tunnel_peer_selector, "\"No valid explicit peers found, building zero hop\""),
    "TPS.fallback_select_fast_peers": (router_tunnel_peer_selector, "ctx.profileOrganizer().selectFastPeers(more, exclude, matches);"),
    "CPS.select_peers": (router_client_peer_selector, "public List<Hash> selectPeers(TunnelPoolSettings settings)"),
    "CPS.should_select_explicit_check": (router_client_peer_selector, "if (shouldSelectExplicit(settings))"),
    "CPS.explicit_returns_select_explicit": (router_client_peer_selector, "return selectExplicit(settings, length);"),
    # Plan 245 §11 — exact-pinned stock-response construction signal
    # attribution. The retained Plan 237/244 retransmit-timer
    # `Resend in ...` log is conditional: ACK-only packets with
    # `sequenceNum == 0` and no SYN flag take the ACK-only branch and
    # never schedule the retransmit timer. ConnectionDataReceiver
    # `buildPacket()` therefore emits the direct authoritative
    # construction log `New OB pkt (acks not yet filled in): ...`
    # before returning the PacketLocal to Connection.sendPacket().
    # The Plan 245 lane also observes the doSend-false INFO log
    # (`writeData called: size=... doSend=false ...`) and the
    # MessageOutputStream.flushAvailable INFO log to attribute the
    # exact predicate that suppressed construction when doSend=false.
    # A source upgrade that renames any of these must fail the lane
    # before an external attempt.
    "SchedulerReceived.accept_predicates": (scheduler, "(con != null) && \n               (con.getLastSendId() < 0) &&\n               (con.getSendStreamId() > 0)"),
    "SchedulerReceived.unacked_guard": (scheduler, "if (con.getUnackedPacketsReceived() <= 0) {"),
    "SchedulerReceived.no_unacked_warn": (scheduler, "hmm, state is received, but no unacked packets received?"),
    "SchedulerReceived.send_branch_log": (scheduler, "_log.debug(\"received con... send a packet\");"),
    "SchedulerReceived.reschedule_branch_log": (scheduler, "_log.debug(\"received con... time till next send: \" + timeTillSend);"),
    "SchedulerReceived.send_available_call": (scheduler, "con.sendAvailable();"),
    "SchedulerReceived.set_next_send_time_negative": (scheduler, "con.setNextSendTime(-1);"),
    "Connection.sendAvailable_method": (connection, "void sendAvailable()"),
    "Connection.sendAvailable_flush": (connection, "_outputStream.flushAvailable(_receiver, false);"),
    "MessageOutputStream.flushAvailable_two_arg": (message_output_stream, "void flushAvailable(DataReceiver target, boolean blocking)"),
    "MessageOutputStream.writeData_call": (message_output_stream, "ws = target.writeData(_buf, 0, _valid);"),
    "MessageOutputStream.zero_valid_allowed": (message_output_stream, "// if valid == 0 return ??? - no, this could flush a CLOSE packet too."),
    "ConnectionDataReceiver.writeData_method": (receiver, "public MessageOutputStream.WriteStatus writeData(byte[] buf, int off, int size)"),
    "ConnectionDataReceiver.writeData_dosend_false_log": (receiver, "_log.info(\"writeData called: size=\"+size + \" doSend=\" + doSend"),
    "ConnectionDataReceiver.unacked_received_forces_doSend": (receiver, "if (con.getUnackedPacketsReceived() > 0)\n            doSend = true;"),
    "ConnectionDataReceiver.send_calls_buildPacket": (receiver, "PacketLocal packet = buildPacket(buf, off, size, forceIncrement);"),
    "ConnectionDataReceiver.buildPacket_new_ob_log": (receiver, "_log.debug(\"New OB pkt (acks not yet filled in): \" + packet + \" on \" + _connection);"),
    "Connection.sendPacket_ack_only_branch": (connection, "if ( (packet.getSequenceNum() == 0) && (!packet.isFlagSet(Packet.FLAG_SYNCHRONIZE)) ) {"),
    "Connection.sendPacket_ack_only_comment": (connection, "// ACK-only"),
    "Connection.sendPacket_resend_timer_log": (connection, "_log.debug(Connection.this + \" Resend in \" + timeout + \" for \" + packet);"),
    "Connection.sendPacket_retransmit_schedule": (connection, "if (_retransmitEvent.scheduleIfNotRunning(timeout)) {"),
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
        # Plan 240 §4/§14 — pinned streaming-epoch lookup sequence the
        # P240-ROUTER-B + P240 exact-job observer attributes. Retained
        # Plan-236/237/238/239 rows above stay frozen; these rows are
        # additive. Log signals are source facts only; execution raw
        # log lines never enter durable evidence.
        "java_isj_negative_cache\tisNegativeCached(_key) | Negative cached, not searching: <key> (IterativeSearchJob.runJob pre-selection guard)\n",
        "java_isj_selection\tselectFloodfillParticipants(_rkey, _totalSearchLimit + EXTRA_PEERS, ks) | New ISJ for LS <target> ... toTry: [...] (exact target-job candidate set)\n",
        "java_isj_retry_guards\tSkipping query w/ router too close to others <peer> (retry IP-close diversity, IP_CLOSE_BYTES = 3)\n",
        "java_isj_sendquery_guards\tStoreJob.shouldStoreTo(ri) | not sending query to old router | failed, no IB client tunnel to receive reply | skipped, no ratchet/elg support | not doing zero-hop self-lookup | not doing zero-hop lookup to unknown (sendQuery pre-dispatch guards)\n",
        "java_isj_dispatch_proof\tISJ try <n> for LS <target> to <peer> | Encrypted DLM for <target> to <peer> (query-dispatch preparation vs authoritative dispatch)\n",
        "java_fps_capability_source\tgetPeersByCapability(FloodfillNetworkDatabaseFacade.CAPABILITY_FLOODFILL) | isBanlistedForever(h) (selector candidate source + forever-banlist exclusion)\n",
        "java_fps_classification\tSame /16, family, or port | Old | Bad country | Slow | Bad (new) | Good | OK | Bad (DB) | Bad (no hist) | Bad (no prof) (FloodfillPeerSelector ranking family)\n",
        # Plan 241 §4/§15 — pinned one-hop client-tunnel selection the
        # P241 lane corrects into. Retained Plan-236/237/238/239/240
        # rows above stay frozen; these rows are additive. Source facts
        # only; execution raw log lines never enter durable evidence.
        "java_tunnelpool_selection\tselectOutboundTunnel(destination, closestTo) | _clientOutboundPools.get(destination) | pool.selectTunnel(closestTo) (destination client pool served when present, no exploratory fallback)\n",
        "java_tunnelpool_zero_hop_last\tselectTunnel(Hash closestTo) | avoidZeroHop = !getAllowZeroHop() | TunnelInfoComparator(closestTo, avoidZeroHop) zero-hop-last (genuine non-zero-hop client tunnel bypasses the ISJ guard)\n",
        "java_isj_sendquery_split\tctx.netDb().lookupRouterInfoLocally(peer) (main-NetDB send preparation) vs _facade.lookupLocallyWithoutValidation(peer) (client-facade zero-hop guard) | outTunnel.getLength() <= 1 | dispatchOutbound(outMsg, outTunnel.getSendTunnelId(0), peer) (authoritative dispatch)\n",
        # Plan 242 §3/§15 — pinned probabilistic explicit-peer selection
        # that proves a `length=1` SessionConfig via `explicitPeers` is
        # NOT deterministic. ShouldSelectExplicit returns true only when
        # `ctx.random().nextInt(4) == 0`; otherwise ClientPeerSelector
        # falls back to the stock `selectFastPeers` path. Retained Plan
        # 236–241 rows above stay frozen; these rows are additive.
        "java_explicit_peer_semantics\tshouldSelectExplicit(settings) returns true only when ctx.random().nextInt(4) == 0 (TunnelPeerSelector one-in-four explicit branch)\n",
        "java_explicit_peer_dispatcher\tClientPeerSelector.selectPeers(settings) | if (shouldSelectExplicit(settings)) return selectExplicit(settings, length); (explicit vs stock fast-peer dispatch)\n",
        "java_explicit_branch_evidence\tNo valid explicit peers found, building zero hop | selectFastPeers(more, exclude, matches) | Collections.shuffle(rv, ctx.random()) (TunnelPeerSelector explicit branch fallbacks)\n",
        # Plan 245 §11 — pinned stock-response construction signal
        # attribution that supersedes only the Plan 237/244 proxy.
        # Retained Plan 236–244 rows above stay frozen; these rows
        # are additive. The retransmit-timer `Resend in` signal is
        # conditional (ACK-only sequence-0 non-SYN packets bypass the
        # retransmit-timer block); the authoritative construction
        # signal is `ConnectionDataReceiver.buildPacket()` direct log.
        "java_send_received_accept\tscheduler.accept(con) predicates: con != null && con.getLastSendId() < 0 && con.getSendStreamId() > 0 (SchedulerReceived.accept)\n",
        "java_scheduler_unacked_guard\tif (con.getUnackedPacketsReceived() <= 0) { ... return; } (SchedulerReceived.eventOccurred pre-send branch guard)\n",
        "java_scheduler_send_branch\treceived con... send a packet | con.sendAvailable(); | con.setNextSendTime(-1); (SchedulerReceived send-branch order)\n",
        "java_scheduler_reschedule_branch\treceived con... time till next send: <n> | reschedule(<n>, con) (SchedulerReceived reschedule-only branch)\n",
        "java_scheduler_no_unacked_warn\thmm, state is received, but no unacked packets received? (SchedulerReceived guard warning)\n",
        "java_send_available_call\tvoid sendAvailable() | _outputStream.flushAvailable(_receiver, false) (Connection.sendAvailable → MessageOutputStream)\n",
        "java_flush_available_calls_write_data\tvoid flushAvailable(DataReceiver target, boolean blocking) | ws = target.writeData(_buf, 0, _valid); (MessageOutputStream.flushAvailable even with zero valid)\n",
        "java_writedata_dosend_false_log\twriteData called: size=<n> doSend=false unackedReceived: <n> con: ... (ConnectionDataReceiver.writeData doSend=false INFO log)\n",
        "java_writedata_unacked_forces_dosend\tif (con.getUnackedPacketsReceived() > 0) doSend = true; (ConnectionDataReceiver.writeData unacked override)\n",
        "java_buildpacket_direct_log\tNew OB pkt (acks not yet filled in): <packet> on <connection> (ConnectionDataReceiver.buildPacket direct construction signal)\n",
        "java_sendpacket_ack_only_branch\tif ( (packet.getSequenceNum() == 0) && (!packet.isFlagSet(Packet.FLAG_SYNCHRONIZE)) ) { /* ACK-only */ } (Connection.sendPacket ACK-only branch — bypasses retransmit timer)\n",
        "java_sendpacket_resend_timer_log\tResend in <timeout> for <packet> | _retransmitEvent.scheduleIfNotRunning(timeout) (Connection.sendPacket retransmit-timer log — conditional, not universal)\n",
    ]),
    encoding="utf-8",
)
PY

echo "Java response source lock passed: ${OUTPUT}"
