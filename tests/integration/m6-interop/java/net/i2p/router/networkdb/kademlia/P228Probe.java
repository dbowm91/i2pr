// Plan 228 WP A/G — test-only read-only tunnel-infrastructure + client-pool
// snapshot probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P227Probe.java`. Never compiled into or
// against the exact-pinned source checkout. It MUST NOT patch or replace
// any Java I2P class, MUST NOT mutate NetDB/profile/tunnel/banlist state,
// MUST NOT use reflection or private-field access, MUST NOT store
// RouterInfos, MUST NOT install or build tunnels, MUST NOT force
// connections, MUST NOT promote tiers.
//
// Read-only public-API surface:
//   - infra: `RouterContext.tunnelManager()` facade
//     (`getFreeTunnelCount`, `getOutboundTunnelCount`,
//     `getInboundClientTunnelCount`, `getOutboundClientTunnelCount`,
//     `getInboundExploratoryPool` / `getOutboundExploratoryPool` +
//     `TunnelPool.listTunnels()` read-only with `TunnelInfo.getLength()`);
//   - client pools: `getInboundPool(Hash)` + `getOutboundPool(Hash)` +
//     `listTunnels()` read-only, counts capped at 8.
//
// All facts are bounded (tunnel lists capped at 8, counts clamped to
// 0..64) and carry no key material, payloads, peer paths, or raw log
// text. No peer hash is echoed except the caller-supplied client DBID
// echo already performed by the launcher.
package net.i2p.router.networkdb.kademlia;

import java.util.List;
import net.i2p.data.Hash;
import net.i2p.router.RouterContext;
import net.i2p.router.TunnelInfo;
import net.i2p.router.TunnelManagerFacade;
import net.i2p.router.tunnel.pool.TunnelPool;

public final class P228Probe {

    private P228Probe() {
    }

    public static final class Infra {
        public final boolean observable;
        public final int freeTunnelCount;
        public final int inboundTunnelCount;
        public final int outboundTunnelCount;
        public final int inboundExploratoryCount;
        public final int outboundExploratoryCount;
        public final int inboundExploratoryNonzeroCount;
        public final int outboundExploratoryNonzeroCount;
        public final String error;

        private Infra(
                boolean observable,
                int freeTunnelCount,
                int inboundTunnelCount,
                int outboundTunnelCount,
                int inboundExploratoryCount,
                int outboundExploratoryCount,
                int inboundExploratoryNonzeroCount,
                int outboundExploratoryNonzeroCount,
                String error) {
            this.observable = observable;
            this.freeTunnelCount = freeTunnelCount;
            this.inboundTunnelCount = inboundTunnelCount;
            this.outboundTunnelCount = outboundTunnelCount;
            this.inboundExploratoryCount = inboundExploratoryCount;
            this.outboundExploratoryCount = outboundExploratoryCount;
            this.inboundExploratoryNonzeroCount = inboundExploratoryNonzeroCount;
            this.outboundExploratoryNonzeroCount = outboundExploratoryNonzeroCount;
            this.error = error;
        }

        public static Infra unavailable(String reason) {
            return new Infra(false, 0, 0, 0, 0, 0, 0, 0, reason);
        }

        public static Infra ok(
                int freeTunnelCount,
                int inboundTunnelCount,
                int outboundTunnelCount,
                int inboundExploratoryCount,
                int outboundExploratoryCount,
                int inboundExploratoryNonzeroCount,
                int outboundExploratoryNonzeroCount) {
            return new Infra(true, freeTunnelCount, inboundTunnelCount,
                outboundTunnelCount, inboundExploratoryCount,
                outboundExploratoryCount, inboundExploratoryNonzeroCount,
                outboundExploratoryNonzeroCount, null);
        }
    }

    public static final class ClientPools {
        public final boolean observable;
        public final boolean inboundPoolPresent;
        public final boolean outboundPoolPresent;
        public final int inboundTunnelCount;
        public final int outboundTunnelCount;
        public final String error;

        private ClientPools(
                boolean observable,
                boolean inboundPoolPresent,
                boolean outboundPoolPresent,
                int inboundTunnelCount,
                int outboundTunnelCount,
                String error) {
            this.observable = observable;
            this.inboundPoolPresent = inboundPoolPresent;
            this.outboundPoolPresent = outboundPoolPresent;
            this.inboundTunnelCount = inboundTunnelCount;
            this.outboundTunnelCount = outboundTunnelCount;
            this.error = error;
        }

        public static ClientPools unavailable(String reason) {
            return new ClientPools(false, false, false, 0, 0, reason);
        }

        public static ClientPools ok(
                boolean inboundPoolPresent,
                boolean outboundPoolPresent,
                int inboundTunnelCount,
                int outboundTunnelCount) {
            return new ClientPools(true, inboundPoolPresent, outboundPoolPresent,
                inboundTunnelCount, outboundTunnelCount, null);
        }
    }

    private static int clampCount(int value) {
        if (value < 0) {
            return 0;
        }
        if (value > 64) {
            return 64;
        }
        return value;
    }

    private static int boundedTunnelCount(List<TunnelInfo> tunnels) {
        if (tunnels == null) {
            return 0;
        }
        return Math.min(tunnels.size(), 8);
    }

    private static int countNonzeroHop(List<TunnelInfo> tunnels) {
        if (tunnels == null) {
            return 0;
        }
        int count = 0;
        int checked = 0;
        for (TunnelInfo info : tunnels) {
            if (checked >= 8 || info == null) {
                break;
            }
            checked++;
            try {
                if (info.getLength() > 1) {
                    count++;
                }
            } catch (RuntimeException re) {
                continue;
            }
        }
        return Math.min(count, 8);
    }

    /**
     * Read-only router tunnel-infrastructure snapshot.
     * Uses only the public TunnelManagerFacade accessors; never builds,
     * installs, or mutates tunnels. Exploratory nonzero counts are
     * derived from read-only listTunnels() length checks only; no peer
     * hashes or paths are exposed.
     */
    public static Infra snapshotTunnelInfra(RouterContext ctx) {
        if (ctx == null) {
            return Infra.unavailable("null-argument");
        }
        try {
            TunnelManagerFacade mgr;
            try {
                mgr = ctx.tunnelManager();
            } catch (RuntimeException re) {
                return Infra.unavailable("probe-threw-" + re.getClass().getSimpleName());
            }
            if (mgr == null) {
                return Infra.unavailable("manager-null");
            }
            int free;
            int outboundExpl;
            int inboundClient;
            int outboundClient;
            try {
                free = clampCount(mgr.getFreeTunnelCount());
            } catch (RuntimeException re) {
                return Infra.unavailable("probe-threw-" + re.getClass().getSimpleName());
            }
            try {
                outboundExpl = clampCount(mgr.getOutboundTunnelCount());
            } catch (RuntimeException re) {
                outboundExpl = 0;
            }
            try {
                inboundClient = clampCount(mgr.getInboundClientTunnelCount());
            } catch (RuntimeException re) {
                inboundClient = 0;
            }
            try {
                outboundClient = clampCount(mgr.getOutboundClientTunnelCount());
            } catch (RuntimeException re) {
                outboundClient = 0;
            }
            int inboundTotal = clampCount(free + inboundClient);
            int outboundTotal = clampCount(outboundExpl + outboundClient);
            int inboundNonzero = 0;
            int outboundNonzero = 0;
            try {
                TunnelPool inboundExplPool = mgr.getInboundExploratoryPool();
                if (inboundExplPool != null) {
                    List<TunnelInfo> tunnels = null;
                    try {
                        tunnels = inboundExplPool.listTunnels();
                    } catch (RuntimeException re) {
                        tunnels = null;
                    }
                    inboundNonzero = countNonzeroHop(tunnels);
                }
            } catch (RuntimeException re) {
                inboundNonzero = 0;
            }
            try {
                TunnelPool outboundExplPool = mgr.getOutboundExploratoryPool();
                if (outboundExplPool != null) {
                    List<TunnelInfo> tunnels = null;
                    try {
                        tunnels = outboundExplPool.listTunnels();
                    } catch (RuntimeException re) {
                        tunnels = null;
                    }
                    outboundNonzero = countNonzeroHop(tunnels);
                }
            } catch (RuntimeException re) {
                outboundNonzero = 0;
            }
            return Infra.ok(free, inboundTotal, outboundTotal, free,
                outboundExpl, inboundNonzero, outboundNonzero);
        } catch (RuntimeException e) {
            return Infra.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    /**
     * Read-only client-pool presence/count snapshot for one helper client
     * DBID. Resolves live pools through public tunnel-manager accessors
     * and inspects installed-tunnel counts read-only. Never installs,
     * builds, or mutates tunnels. No peer paths exposed.
     */
    public static ClientPools snapshotClientPools(RouterContext ctx, Hash clientDbid) {
        if (ctx == null || clientDbid == null) {
            return ClientPools.unavailable("null-argument");
        }
        try {
            TunnelManagerFacade mgr;
            try {
                mgr = ctx.tunnelManager();
            } catch (RuntimeException re) {
                return ClientPools.unavailable("probe-threw-" + re.getClass().getSimpleName());
            }
            if (mgr == null) {
                return ClientPools.unavailable("manager-null");
            }
            TunnelPool inboundPool = null;
            TunnelPool outboundPool = null;
            try {
                inboundPool = mgr.getInboundPool(clientDbid);
            } catch (RuntimeException re) {
                inboundPool = null;
            }
            try {
                outboundPool = mgr.getOutboundPool(clientDbid);
            } catch (RuntimeException re) {
                outboundPool = null;
            }
            boolean inboundPresent = inboundPool != null;
            boolean outboundPresent = outboundPool != null;
            int inboundCount = 0;
            int outboundCount = 0;
            if (inboundPresent) {
                try {
                    inboundCount = boundedTunnelCount(inboundPool.listTunnels());
                } catch (RuntimeException re) {
                    inboundCount = 0;
                }
            }
            if (outboundPresent) {
                try {
                    outboundCount = boundedTunnelCount(outboundPool.listTunnels());
                } catch (RuntimeException re) {
                    outboundCount = 0;
                }
            }
            return ClientPools.ok(inboundPresent, outboundPresent,
                clampCount(inboundCount), clampCount(outboundCount));
        } catch (RuntimeException e) {
            return ClientPools.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
