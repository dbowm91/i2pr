// Plan 239 WP — test-only read-only Router-A pre-dispatch / OCMOSJ
// attribution probe for the streaming response epoch.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P238Probe.java`.
// Never compiled into or against the exact-pinned source checkout.
// It MUST NOT patch or replace any Java I2P class, MUST NOT mutate
// NetDB/profile/tunnel/banlist/queue/stat state, MUST NOT use reflection
// or private-field access, MUST NOT store RouterInfos, MUST NOT install
// or build tunnels, MUST NOT force connections, MUST NOT promote tiers,
// MUST NOT create profiles, MUST NOT call `heardAbout`, MUST NOT read
// private queues, MUST NOT create rates.
//
// Read-only public-API surface:
//   - target LS: `P224LsProbe.snapshotClient` (local read-only
//     `lookupLocallyWithoutValidation` + `lookupLeaseSetLocally` on
//     `ctx.clientNetDb(clientDbid)`; never primes the sub-DB);
//   - lookup/dispatch stats: `RouterContext.statManager().getRate(name)` +
//     `RateStat.getLifetimeEventCount()` for the six exact-pinned names
//     `client.leaseSetFoundRemoteTime`,
//     `client.leaseSetFailedRemoteTime`, `client.dispatchNoTunnels`,
//     `client.dispatchPrepareTime`, `client.dispatchTime`,
//     `client.dispatchSendTime`. A missing rate reports -1 (unknown),
//     never zero-as-fact;
//   - tunnels: `RouterContext.tunnelManager().getOutboundPool(dbid)` +
//     `getInboundPool(dbid)` + `TunnelPool.listTunnels()` read-only;
//     when exactly one outbound tunnel is installed its hop-0 send id
//     (`TunnelInfo.getSendTunnelId(0)`) is exposed, when exactly one
//     inbound tunnel is installed its hop-0 receive id
//     (`TunnelInfo.getReceiveTunnelId(0)`) is exposed, otherwise 0;
//   - logs: `RouterContext.logManager().getBuffer()`
//     `.getMostRecentMessages()` substring counts for the five
//     exact-pinned OCMOSJ branches (bounded, capped). No raw log text
//     leaves the probe — counts only.
//
// All facts are bounded (tunnel lists capped at 8, log buffer scanned
// with a fixed cap, strings truncated to fixed ceilings) and carry only
// booleans, counts, type codes, and tunnel ids. No peer paths, keys,
// tags, payloads, queue contents, destinations, hashes, or raw log text
// are exposed.
package net.i2p.router.networkdb.kademlia;

import java.util.List;
import net.i2p.data.Hash;
import net.i2p.data.TunnelId;
import net.i2p.router.RouterContext;
import net.i2p.router.TunnelInfo;
import net.i2p.router.tunnel.pool.TunnelPool;
import net.i2p.stat.RateStat;

public final class P239Probe {

    private P239Probe() {
    }

    /** Unknown lifetime count: the named rate was never created. */
    public static final long COUNT_UNKNOWN = -1;

    /** Unknown tunnel id: no exactly-one tunnel to attribute. */
    public static final long TUNNEL_UNKNOWN = 0;

    /** Unknown log count: the console buffer was unreadable. */
    public static final int LOG_UNKNOWN = -1;

    /** Max log-buffer matches counted per needle (bounded). */
    public static final int MAX_LOG_MATCHES = 512;

    /** Exact-pinned OCMOSJ stat names the dispatch observer counts. */
    public static final String STAT_FOUND_REMOTE = "client.leaseSetFoundRemoteTime";
    public static final String STAT_FAILED_REMOTE = "client.leaseSetFailedRemoteTime";
    public static final String STAT_NO_TUNNELS = "client.dispatchNoTunnels";
    public static final String STAT_PREPARE = "client.dispatchPrepareTime";
    public static final String STAT_DISPATCH = "client.dispatchTime";
    public static final String STAT_DISPATCH_SEND = "client.dispatchSendTime";

    /** Exact-pinned OCMOSJ log branches the dispatch observer counts. */
    public static final String LOG_NO_OUTBOUND_TUNNEL =
        "Could not find any outbound tunnels to send the payload through";
    public static final String LOG_GARLIC_NO_TUNNEL =
        "Unable to create the garlic message (no tunnels left or too lagged)";
    public static final String LOG_LOCAL_LS_MISSING =
        "Lookup locally didn't find the leaseSet for ";
    public static final String LOG_ONLY_RAP_LS =
        "Only have RAP LS for ";
    public static final String LOG_LEASE_SEND_FAILURE =
        "Got the lease but can't send to it, failure code ";
    public static final String LOG_NO_LEASES =
        "No leases found from: ";

    public static final class Dispatch {
        public final boolean targetLsLocalPresent;
        public final String targetLsCurrent;
        public final int targetLsType;
        public final String targetLsRap;
        public final String targetLsRar;
        public final long foundRemoteEvents;
        public final long failedRemoteEvents;
        public final int outboundTunnelCount;
        public final int inboundTunnelCount;
        public final long outboundSendId;
        public final long inboundReceiveId;
        public final long noTunnelsEvents;
        public final long prepareEvents;
        public final long dispatchTimeEvents;
        public final long dispatchSendEvents;
        public final int logNoOutbound;
        public final int logGarlicNoTunnel;
        public final int logLocalMissing;
        public final int logOnlyRap;
        public final int logBadUnsupported;
        public final String error;

        private Dispatch(
                boolean targetLsLocalPresent,
                String targetLsCurrent,
                int targetLsType,
                String targetLsRap,
                String targetLsRar,
                long foundRemoteEvents,
                long failedRemoteEvents,
                int outboundTunnelCount,
                int inboundTunnelCount,
                long outboundSendId,
                long inboundReceiveId,
                long noTunnelsEvents,
                long prepareEvents,
                long dispatchTimeEvents,
                long dispatchSendEvents,
                int logNoOutbound,
                int logGarlicNoTunnel,
                int logLocalMissing,
                int logOnlyRap,
                int logBadUnsupported,
                String error) {
            this.targetLsLocalPresent = targetLsLocalPresent;
            this.targetLsCurrent = targetLsCurrent;
            this.targetLsType = targetLsType;
            this.targetLsRap = targetLsRap;
            this.targetLsRar = targetLsRar;
            this.foundRemoteEvents = foundRemoteEvents;
            this.failedRemoteEvents = failedRemoteEvents;
            this.outboundTunnelCount = outboundTunnelCount;
            this.inboundTunnelCount = inboundTunnelCount;
            this.outboundSendId = outboundSendId;
            this.inboundReceiveId = inboundReceiveId;
            this.noTunnelsEvents = noTunnelsEvents;
            this.prepareEvents = prepareEvents;
            this.dispatchTimeEvents = dispatchTimeEvents;
            this.dispatchSendEvents = dispatchSendEvents;
            this.logNoOutbound = logNoOutbound;
            this.logGarlicNoTunnel = logGarlicNoTunnel;
            this.logLocalMissing = logLocalMissing;
            this.logOnlyRap = logOnlyRap;
            this.logBadUnsupported = logBadUnsupported;
            this.error = error;
        }

        public static Dispatch unavailable(String reason) {
            return new Dispatch(
                false, "unknown", -1, "unknown", "unknown",
                COUNT_UNKNOWN, COUNT_UNKNOWN,
                0, 0, TUNNEL_UNKNOWN, TUNNEL_UNKNOWN,
                COUNT_UNKNOWN, COUNT_UNKNOWN, COUNT_UNKNOWN, COUNT_UNKNOWN,
                LOG_UNKNOWN, LOG_UNKNOWN, LOG_UNKNOWN, LOG_UNKNOWN, LOG_UNKNOWN,
                reason);
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

    private static String triState(Boolean value) {
        if (value == null) {
            return "unknown";
        }
        return value.booleanValue() ? "true" : "false";
    }

    private static int countBufferSubstring(RouterContext ctx, String needle) {
        try {
            if (ctx.logManager() == null || ctx.logManager().getBuffer() == null) {
                return LOG_UNKNOWN;
            }
            List<String> messages = ctx.logManager().getBuffer().getMostRecentMessages();
            if (messages == null) {
                return LOG_UNKNOWN;
            }
            int count = 0;
            for (String message : messages) {
                if (message == null) {
                    continue;
                }
                if (message.contains(needle)) {
                    count++;
                    if (count >= MAX_LOG_MATCHES) {
                        break;
                    }
                }
            }
            return count;
        } catch (RuntimeException re) {
            return LOG_UNKNOWN;
        } catch (Throwable t) {
            return LOG_UNKNOWN;
        }
    }

    /**
     * Read-only pre-dispatch snapshot for one helper client DBID and one
     * target hash. Observation only; never creates rates, never mutates
     * counters, never primes the client sub-DB, never installs tunnels,
     * never promotes log text. Unknown rates report -1, unknown tunnel
     * ids report 0, unreadable log buffers report -1.
     */
    public static Dispatch snapshotDispatch(
            RouterContext ctx, Hash clientDbid, Hash targetHash) {
        if (ctx == null || clientDbid == null || targetHash == null) {
            return Dispatch.unavailable("null-argument");
        }
        try {
            // Target LS via the retained P224 local/client-subDB
            // semantics (local reads only; no remote search).
            P224LsProbe.Result ls = P224LsProbe.snapshotClient(ctx, clientDbid, targetHash);
            boolean localPresent = false;
            String current = "unknown";
            int entryType = -1;
            String rap = "unknown";
            String rar = "unknown";
            if (ls != null && ls.error == null) {
                localPresent = ls.validatedPresent;
                current = triState(ls.current);
                entryType = ls.entryType;
                rap = triState(ls.receivedAsPublished);
                rar = triState(ls.receivedAsReply);
            } else if (ls != null) {
                // Unresolvable client DB or fallback: keep Unknown
                // LS facts but still report stats/tunnels/logs below.
                // Do not convert Unknown into zero.
                localPresent = false;
            }

            long foundRemote = lifetimeCount(ctx, STAT_FOUND_REMOTE);
            long failedRemote = lifetimeCount(ctx, STAT_FAILED_REMOTE);
            long noTunnels = lifetimeCount(ctx, STAT_NO_TUNNELS);
            long prepare = lifetimeCount(ctx, STAT_PREPARE);
            long dispatchTime = lifetimeCount(ctx, STAT_DISPATCH);
            long dispatchSend = lifetimeCount(ctx, STAT_DISPATCH_SEND);

            int outboundCount = 0;
            int inboundCount = 0;
            long outboundSendId = TUNNEL_UNKNOWN;
            long inboundReceiveId = TUNNEL_UNKNOWN;
            try {
                TunnelPool outboundPool = null;
                TunnelPool inboundPool = null;
                try {
                    outboundPool = ctx.tunnelManager().getOutboundPool(clientDbid);
                } catch (RuntimeException re) {
                    outboundPool = null;
                }
                try {
                    inboundPool = ctx.tunnelManager().getInboundPool(clientDbid);
                } catch (RuntimeException re) {
                    inboundPool = null;
                }
                if (outboundPool != null) {
                    List<TunnelInfo> tunnels = null;
                    try {
                        tunnels = outboundPool.listTunnels();
                    } catch (RuntimeException re) {
                        tunnels = null;
                    }
                    if (tunnels != null) {
                        int seen = 0;
                        TunnelInfo single = null;
                        for (TunnelInfo info : tunnels) {
                            if (seen >= 8 || info == null) {
                                break;
                            }
                            seen++;
                            if (seen == 1) {
                                single = info;
                            } else {
                                single = null;
                            }
                        }
                        outboundCount = Math.min(seen, 8);
                        if (seen == 1 && single != null) {
                            try {
                                TunnelId tid = single.getSendTunnelId(0);
                                if (tid != null) {
                                    long id = tid.getTunnelId();
                                    if (id > 0) {
                                        outboundSendId = id;
                                    }
                                }
                            } catch (RuntimeException re) {
                                outboundSendId = TUNNEL_UNKNOWN;
                            }
                        }
                    }
                }
                if (inboundPool != null) {
                    List<TunnelInfo> tunnels = null;
                    try {
                        tunnels = inboundPool.listTunnels();
                    } catch (RuntimeException re) {
                        tunnels = null;
                    }
                    if (tunnels != null) {
                        int seen = 0;
                        TunnelInfo single = null;
                        for (TunnelInfo info : tunnels) {
                            if (seen >= 8 || info == null) {
                                break;
                            }
                            seen++;
                            if (seen == 1) {
                                single = info;
                            } else {
                                single = null;
                            }
                        }
                        inboundCount = Math.min(seen, 8);
                        if (seen == 1 && single != null) {
                            try {
                                TunnelId tid = single.getReceiveTunnelId(0);
                                if (tid != null) {
                                    long id = tid.getTunnelId();
                                    if (id > 0) {
                                        inboundReceiveId = id;
                                    }
                                }
                            } catch (RuntimeException re) {
                                inboundReceiveId = TUNNEL_UNKNOWN;
                            }
                        }
                    }
                }
            } catch (RuntimeException re) {
                // Tunnel snapshot failure leaves counts at 0/unknown-id;
                // stats/logs below still report.
            }

            int logNoOutbound = countBufferSubstring(ctx, LOG_NO_OUTBOUND_TUNNEL);
            int logGarlic = countBufferSubstring(ctx, LOG_GARLIC_NO_TUNNEL);
            int logLocal = countBufferSubstring(ctx, LOG_LOCAL_LS_MISSING);
            int logRap = countBufferSubstring(ctx, LOG_ONLY_RAP_LS);
            int logSendFail = countBufferSubstring(ctx, LOG_LEASE_SEND_FAILURE);
            int logNoLeases = countBufferSubstring(ctx, LOG_NO_LEASES);
            int logBad;
            if (logSendFail == LOG_UNKNOWN || logNoLeases == LOG_UNKNOWN) {
                logBad = LOG_UNKNOWN;
            } else {
                logBad = Math.min(logSendFail + logNoLeases, MAX_LOG_MATCHES);
            }

            return new Dispatch(
                localPresent, current, entryType, rap, rar,
                foundRemote, failedRemote,
                outboundCount, inboundCount, outboundSendId, inboundReceiveId,
                noTunnels, prepare, dispatchTime, dispatchSend,
                logNoOutbound, logGarlic, logLocal, logRap, logBad,
                null);
        } catch (RuntimeException e) {
            return Dispatch.unavailable(
                "probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
