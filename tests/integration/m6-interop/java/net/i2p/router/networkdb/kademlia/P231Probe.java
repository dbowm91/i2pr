// Plan 231 WP B/C/D — test-only read-only post-`ACCEPTED` tunnel-dispatch
// attribution probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P230Probe.java`.
// Never compiled into or against the exact-pinned source checkout.
// It MUST NOT patch or replace any Java I2P class, MUST NOT mutate
// NetDB/profile/tunnel/banlist/queue state, MUST NOT use reflection or
// private-field access, MUST NOT store RouterInfos, MUST NOT install or
// build tunnels, MUST NOT force connections, MUST NOT promote tiers,
// MUST NOT create profiles, MUST NOT call `heardAbout`, MUST NOT read
// private queues.
//
// Read-only public-API surface:
//   - gateway stats: `RouterContext.statManager().getRate(name)` +
//     `RateStat.getLifetimeEventCount()` for the six exact-pinned names
//     `client.dispatchTime`, `client.dispatchSendTime`,
//     `tunnel.dispatchOutboundTunnel`, `tunnel.dropGatewayOverflow`,
//     `tunnel.dispatchInbound`, `tunnel.inboundLookupSuccess`.
//     A missing rate reports -1 (unknown), never zero-as-fact.
//   - client outbound: `RouterContext.tunnelManager().getOutboundPool(dbid)`
//     + `TunnelPool.listTunnels()` read-only; when exactly one outbound
//     client tunnel is installed its hop-0 send tunnel id
//     (`TunnelInfo.getSendTunnelId(0)`) is exposed, otherwise 0.
//     This is the `_outTunnel.getSendTunnelId(0)` value OCMOSJ passes to
//     `TunnelDispatcher.dispatchOutbound(...)`.
//   - participating: `RouterContext.tunnelDispatcher()`
//     `.listParticipatingTunnels()` read-only; the caller filters by the
//     exact receive tunnel id and receives the single matching
//     `HopConfig` snapshot (`getReceiveTunnelId`, `getSendTunnelId`,
//     `getReceiveFrom` / `getSendTo` as lowercase hex, `none` when null,
//     `getProcessedMessagesCount`). No peer paths, keys, tags, payloads,
//     or raw log text are exposed.
//
// All facts are bounded (tunnel lists capped, strings truncated to fixed
// ceilings) and carry only booleans, counts, tunnel ids, router hashes,
// and enumerated stage tokens.
package net.i2p.router.networkdb.kademlia;

import java.util.List;
import net.i2p.data.Hash;
import net.i2p.data.TunnelId;
import net.i2p.router.RouterContext;
import net.i2p.router.TunnelInfo;
import net.i2p.router.tunnel.HopConfig;
import net.i2p.router.tunnel.pool.TunnelPool;
import net.i2p.stat.RateStat;

public final class P231Probe {

    private P231Probe() {
    }

    /** Unknown lifetime count: the named rate was never created. */
    public static final long COUNT_UNKNOWN = -1;

    /** Unknown tunnel id: no exactly-one tunnel to attribute. */
    public static final long TUNNEL_UNKNOWN = 0;

    public static final class GatewayStats {
        public final long dispatchTime;
        public final long dispatchSendTime;
        public final long dispatchOutboundTunnel;
        public final long dropGatewayOverflow;
        public final long dispatchInbound;
        public final long inboundLookupSuccess;
        public final String error;

        private GatewayStats(
                long dispatchTime,
                long dispatchSendTime,
                long dispatchOutboundTunnel,
                long dropGatewayOverflow,
                long dispatchInbound,
                long inboundLookupSuccess,
                String error) {
            this.dispatchTime = dispatchTime;
            this.dispatchSendTime = dispatchSendTime;
            this.dispatchOutboundTunnel = dispatchOutboundTunnel;
            this.dropGatewayOverflow = dropGatewayOverflow;
            this.dispatchInbound = dispatchInbound;
            this.inboundLookupSuccess = inboundLookupSuccess;
            this.error = error;
        }

        public static GatewayStats unavailable(String reason) {
            return new GatewayStats(
                COUNT_UNKNOWN, COUNT_UNKNOWN, COUNT_UNKNOWN,
                COUNT_UNKNOWN, COUNT_UNKNOWN, COUNT_UNKNOWN, reason);
        }

        public static GatewayStats ok(
                long dispatchTime,
                long dispatchSendTime,
                long dispatchOutboundTunnel,
                long dropGatewayOverflow,
                long dispatchInbound,
                long inboundLookupSuccess) {
            return new GatewayStats(dispatchTime, dispatchSendTime,
                dispatchOutboundTunnel, dropGatewayOverflow,
                dispatchInbound, inboundLookupSuccess, null);
        }
    }

    public static final class ClientOutbound {
        public final boolean clientResolved;
        public final int outboundTunnelCount;
        public final long singleSendTunnelId;
        public final String error;

        private ClientOutbound(
                boolean clientResolved,
                int outboundTunnelCount,
                long singleSendTunnelId,
                String error) {
            this.clientResolved = clientResolved;
            this.outboundTunnelCount = outboundTunnelCount;
            this.singleSendTunnelId = singleSendTunnelId;
            this.error = error;
        }

        public static ClientOutbound unavailable(String reason) {
            return new ClientOutbound(false, 0, TUNNEL_UNKNOWN, reason);
        }

        public static ClientOutbound ok(
                boolean clientResolved,
                int outboundTunnelCount,
                long singleSendTunnelId) {
            return new ClientOutbound(clientResolved, outboundTunnelCount,
                singleSendTunnelId, null);
        }
    }

    public static final class Participating {
        public final boolean present;
        public final int matchCount;
        public final long receiveTunnelId;
        public final long sendTunnelId;
        public final String receiveFromHex;
        public final String sendToHex;
        public final int processedMessages;
        public final String error;

        private Participating(
                boolean present,
                int matchCount,
                long receiveTunnelId,
                long sendTunnelId,
                String receiveFromHex,
                String sendToHex,
                int processedMessages,
                String error) {
            this.present = present;
            this.matchCount = matchCount;
            this.receiveTunnelId = receiveTunnelId;
            this.sendTunnelId = sendTunnelId;
            this.receiveFromHex = receiveFromHex;
            this.sendToHex = sendToHex;
            this.processedMessages = processedMessages;
            this.error = error;
        }

        public static Participating absent(String reason) {
            return new Participating(false, 0, TUNNEL_UNKNOWN, TUNNEL_UNKNOWN,
                "none", "none", 0, reason);
        }

        public static Participating ok(
                int matchCount,
                long receiveTunnelId,
                long sendTunnelId,
                String receiveFromHex,
                String sendToHex,
                int processedMessages) {
            return new Participating(true, matchCount, receiveTunnelId,
                sendTunnelId, receiveFromHex, sendToHex,
                processedMessages, null);
        }
    }

    private static long lifetimeCount(RouterContext ctx, String name) {
        try {
            RateStat rate = ctx.statManager().getRate(name);
            if (rate == null) {
                return COUNT_UNKNOWN;
            }
            long count = rate.getLifetimeEventCount();
            if (count < 0) {
                return COUNT_UNKNOWN;
            }
            return count;
        } catch (RuntimeException re) {
            return COUNT_UNKNOWN;
        }
    }

    private static String hashHex(Hash hash) {
        if (hash == null) {
            return "none";
        }
        try {
            byte[] raw = hash.getData();
            if (raw == null || raw.length != 32) {
                return "none";
            }
            StringBuilder hex = new StringBuilder(64);
            for (byte b : raw) {
                hex.append(String.format("%02x", b & 0xff));
            }
            return hex.toString();
        } catch (RuntimeException re) {
            return "none";
        }
    }

    /**
     * Read-only lifetime-event-count snapshot for the six exact-pinned
     * Plan-231 stat names. Observation only; never creates rates, never
     * mutates counters. A never-created rate reports -1 (unknown).
     */
    public static GatewayStats snapshotGatewayStats(RouterContext ctx) {
        if (ctx == null) {
            return GatewayStats.unavailable("null-argument");
        }
        try {
            long dispatchTime = lifetimeCount(ctx, "client.dispatchTime");
            long dispatchSendTime = lifetimeCount(ctx, "client.dispatchSendTime");
            long dispatchOutboundTunnel =
                lifetimeCount(ctx, "tunnel.dispatchOutboundTunnel");
            long dropGatewayOverflow =
                lifetimeCount(ctx, "tunnel.dropGatewayOverflow");
            long dispatchInbound = lifetimeCount(ctx, "tunnel.dispatchInbound");
            long inboundLookupSuccess =
                lifetimeCount(ctx, "tunnel.inboundLookupSuccess");
            return GatewayStats.ok(dispatchTime, dispatchSendTime,
                dispatchOutboundTunnel, dropGatewayOverflow,
                dispatchInbound, inboundLookupSuccess);
        } catch (RuntimeException e) {
            return GatewayStats.unavailable(
                "probe-threw-" + e.getClass().getSimpleName());
        }
    }

    /**
     * Read-only installed outbound client-tunnel snapshot for one helper
     * client DBID. When exactly one outbound tunnel is installed, its
     * hop-0 send tunnel id is exposed (the value OCMOSJ passes as
     * `_outTunnel.getSendTunnelId(0)`); otherwise 0 (unknown, never a
     * guessed id). Never installs, builds, or mutates tunnels.
     */
    public static ClientOutbound snapshotClientOutbound(
            RouterContext ctx, Hash clientDbid) {
        if (ctx == null || clientDbid == null) {
            return ClientOutbound.unavailable("null-argument");
        }
        try {
            TunnelPool outboundPool = null;
            try {
                outboundPool = ctx.tunnelManager().getOutboundPool(clientDbid);
            } catch (RuntimeException re) {
                outboundPool = null;
            }
            boolean resolved = outboundPool != null;
            int count = 0;
            long sendId = TUNNEL_UNKNOWN;
            if (resolved) {
                List<TunnelInfo> tunnels = null;
                try {
                    tunnels = outboundPool.listTunnels();
                } catch (RuntimeException re) {
                    tunnels = null;
                }
                if (tunnels != null) {
                    int checked = 0;
                    int seen = 0;
                    TunnelInfo single = null;
                    for (TunnelInfo info : tunnels) {
                        if (checked >= 8 || info == null) {
                            break;
                        }
                        checked++;
                        seen++;
                        if (seen == 1) {
                            single = info;
                        } else {
                            single = null;
                        }
                    }
                    count = Math.min(seen, 8);
                    if (seen == 1 && single != null) {
                        try {
                            TunnelId tid = single.getSendTunnelId(0);
                            if (tid != null) {
                                long id = tid.getTunnelId();
                                if (id > 0) {
                                    sendId = id;
                                }
                            }
                        } catch (RuntimeException re) {
                            sendId = TUNNEL_UNKNOWN;
                        }
                    }
                }
            }
            return ClientOutbound.ok(resolved, count, sendId);
        } catch (RuntimeException e) {
            return ClientOutbound.unavailable(
                "probe-threw-" + e.getClass().getSimpleName());
        }
    }

    /**
     * Read-only single participating-tunnel snapshot filtered by the exact
     * receive tunnel id. Iterates
     * `tunnelDispatcher().listParticipatingTunnels()` read-only (capped at
     * 32 configs) and returns the first config whose
     * `getReceiveTunnelId()` equals the requested id, with the total match
     * count. Absent when no config matches. Never mutates tunnel state.
     */
    public static Participating snapshotParticipating(
            RouterContext ctx, long receiveTunnelId) {
        if (ctx == null) {
            return Participating.absent("null-argument");
        }
        if (receiveTunnelId <= 0) {
            return Participating.absent("invalid-receive-tunnel-id");
        }
        try {
            List<HopConfig> configs = null;
            try {
                configs = ctx.tunnelDispatcher().listParticipatingTunnels();
            } catch (RuntimeException re) {
                return Participating.absent(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            if (configs == null) {
                return Participating.absent("participating-null");
            }
            int checked = 0;
            int matches = 0;
            HopConfig first = null;
            for (HopConfig cfg : configs) {
                if (checked >= 32 || cfg == null) {
                    break;
                }
                checked++;
                long recv = 0;
                try {
                    recv = cfg.getReceiveTunnelId();
                } catch (RuntimeException re) {
                    continue;
                }
                if (recv == receiveTunnelId) {
                    matches++;
                    if (first == null) {
                        first = cfg;
                    }
                }
            }
            if (first == null) {
                return Participating.absent("not-found");
            }
            long sendId = 0;
            String recvFrom = "none";
            String sendTo = "none";
            int processed = 0;
            try {
                sendId = first.getSendTunnelId();
            } catch (RuntimeException re) {
                sendId = 0;
            }
            try {
                recvFrom = hashHex(first.getReceiveFrom());
            } catch (RuntimeException re) {
                recvFrom = "none";
            }
            try {
                sendTo = hashHex(first.getSendTo());
            } catch (RuntimeException re) {
                sendTo = "none";
            }
            try {
                processed = first.getProcessedMessagesCount();
                if (processed < 0) {
                    processed = 0;
                }
            } catch (RuntimeException re) {
                processed = 0;
            }
            return Participating.ok(matches, receiveTunnelId, sendId,
                recvFrom, sendTo, processed);
        } catch (RuntimeException e) {
            return Participating.absent(
                "probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
