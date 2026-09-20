// Plan 227 WP A/D — test-only read-only Router-C eligibility + client-tunnel
// snapshot probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java`, `P222SelectorProbe.java`, `P223BranchProbe.java`,
// and `P224LsProbe.java`. Never compiled into or against the exact-pinned
// source checkout. It MUST NOT patch or replace any Java I2P class, MUST NOT
// mutate NetDB/profile/tunnel/banlist state, MUST NOT use reflection or
// private-field access, MUST NOT store RouterInfos, MUST NOT install tunnels,
// MUST NOT force connections, MUST NOT promote tiers.
//
// Read-only public-API surface:
//   - eligibility: `KademliaNetworkDatabaseFacade.lookupLocallyWithoutValidation`
//     (raw presence) + `lookupRouterInfoLocally` (validated presence) on the
//     main facade, `RouterContext.profileOrganizer().isSelectable(Hash)`,
//     `RouterContext.commSystem().isEstablished(Hash)` (diagnostic only),
//     `RouterContext.banlist().isBanlisted(Hash)`;
//   - tunnels: `RouterContext.tunnelManager().getInboundPool(Hash)` +
//     `getOutboundPool(Hash)`, `TunnelPool.listTunnels()` read-only,
//     `TunnelInfo.getLength()` + `getPeer(int)` semantic path check.
//     Java counts local router membership in `getLength()`, so one remote hop
//     via Router C is the exact local+Router-C path (`getLength() == 2` with
//     peers exactly `{local, C}`), never a hard-coded length name alone.
//     Zero-hop is any installed tunnel with `getLength() <= 1`.
//
// All facts are bounded (tunnel lists capped at 8, peer checks bounded) and
// carry no key material, payloads, or raw log text.
package net.i2p.router.networkdb.kademlia;

import java.util.List;
import net.i2p.data.Hash;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.RouterContext;
import net.i2p.router.TunnelInfo;
import net.i2p.router.tunnel.pool.TunnelPool;

public final class P227Probe {

    private P227Probe() {
    }

    public static final class Eligibility {
        public final boolean mainRawPresent;
        public final boolean mainValidPresent;
        public final boolean selectable;
        public final boolean established;
        public final boolean banlisted;
        public final String error;

        private Eligibility(
                boolean mainRawPresent,
                boolean mainValidPresent,
                boolean selectable,
                boolean established,
                boolean banlisted,
                String error) {
            this.mainRawPresent = mainRawPresent;
            this.mainValidPresent = mainValidPresent;
            this.selectable = selectable;
            this.established = established;
            this.banlisted = banlisted;
            this.error = error;
        }

        public static Eligibility unavailable(String reason) {
            return new Eligibility(false, false, false, false, false, reason);
        }

        public static Eligibility ok(
                boolean mainRawPresent,
                boolean mainValidPresent,
                boolean selectable,
                boolean established,
                boolean banlisted) {
            return new Eligibility(
                mainRawPresent, mainValidPresent, selectable, established, banlisted, null);
        }
    }

    public static final class Tunnels {
        public final boolean clientResolved;
        public final boolean inboundPoolPresent;
        public final boolean outboundPoolPresent;
        public final int inboundTunnelCount;
        public final int outboundTunnelCount;
        public final boolean inboundExactOneRemoteHopViaC;
        public final boolean outboundExactOneRemoteHopViaC;
        public final boolean inboundZeroHopPresent;
        public final boolean outboundZeroHopPresent;
        public final String error;

        private Tunnels(
                boolean clientResolved,
                boolean inboundPoolPresent,
                boolean outboundPoolPresent,
                int inboundTunnelCount,
                int outboundTunnelCount,
                boolean inboundExactOneRemoteHopViaC,
                boolean outboundExactOneRemoteHopViaC,
                boolean inboundZeroHopPresent,
                boolean outboundZeroHopPresent,
                String error) {
            this.clientResolved = clientResolved;
            this.inboundPoolPresent = inboundPoolPresent;
            this.outboundPoolPresent = outboundPoolPresent;
            this.inboundTunnelCount = inboundTunnelCount;
            this.outboundTunnelCount = outboundTunnelCount;
            this.inboundExactOneRemoteHopViaC = inboundExactOneRemoteHopViaC;
            this.outboundExactOneRemoteHopViaC = outboundExactOneRemoteHopViaC;
            this.inboundZeroHopPresent = inboundZeroHopPresent;
            this.outboundZeroHopPresent = outboundZeroHopPresent;
            this.error = error;
        }

        public static Tunnels unavailable(String reason) {
            return new Tunnels(false, false, false, 0, 0,
                false, false, false, false, reason);
        }
    }

    /**
     * Read-only Router-C eligibility on Router A's main NetDB.
     * Never creates profiles, never promotes tiers, never forces
     * connections, never stores RouterInfos.
     */
    public static Eligibility snapshotEligibility(
            RouterContext ctx,
            KademliaNetworkDatabaseFacade mainFacade,
            Hash routerC) {
        if (ctx == null || mainFacade == null || routerC == null) {
            return Eligibility.unavailable("null-argument");
        }
        try {
            boolean rawPresent = false;
            try {
                rawPresent = mainFacade.lookupLocallyWithoutValidation(routerC) != null;
            } catch (RuntimeException re) {
                rawPresent = false;
            }
            boolean validPresent = false;
            try {
                RouterInfo info = mainFacade.lookupRouterInfoLocally(routerC);
                validPresent = info != null;
            } catch (RuntimeException re) {
                validPresent = false;
            }
            boolean selectable = false;
            try {
                selectable = ctx.profileOrganizer().isSelectable(routerC);
            } catch (RuntimeException re) {
                return Eligibility.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            boolean established = false;
            try {
                established = ctx.commSystem().isEstablished(routerC);
            } catch (RuntimeException re) {
                established = false;
            }
            boolean banlisted = false;
            try {
                banlisted = ctx.banlist().isBanlisted(routerC);
            } catch (RuntimeException re) {
                banlisted = false;
            }
            return Eligibility.ok(rawPresent, validPresent, selectable, established, banlisted);
        } catch (RuntimeException e) {
            return Eligibility.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    private static boolean tunnelIsZeroHop(TunnelInfo info) {
        try {
            return info.getLength() <= 1;
        } catch (RuntimeException re) {
            return false;
        }
    }

    private static boolean tunnelIsExactOneViaC(
            TunnelInfo info, Hash localHash, Hash routerC) {
        try {
            if (info.getLength() != 2) {
                return false;
            }
            Hash peer0 = info.getPeer(0);
            Hash peer1 = info.getPeer(1);
            if (peer0 == null || peer1 == null) {
                return false;
            }
            boolean zeroIsC = peer0.equals(routerC);
            boolean oneIsC = peer1.equals(routerC);
            boolean zeroIsLocal = peer0.equals(localHash);
            boolean oneIsLocal = peer1.equals(localHash);
            return (zeroIsC && oneIsLocal) || (oneIsC && zeroIsLocal);
        } catch (RuntimeException re) {
            return false;
        }
    }

    private static int boundedTunnelCount(List<TunnelInfo> tunnels) {
        if (tunnels == null) {
            return 0;
        }
        return Math.min(tunnels.size(), 8);
    }

    /**
     * Read-only installed client-tunnel snapshot for one helper client DBID.
     * Resolves live inbound/outbound pools through public tunnel-manager
     * accessors and inspects installed tunnels read-only. Never installs,
     * builds, or mutates tunnels.
     */
    public static Tunnels snapshotClientTunnels(
            RouterContext ctx, Hash clientDbid, Hash routerC) {
        if (ctx == null || clientDbid == null || routerC == null) {
            return Tunnels.unavailable("null-argument");
        }
        try {
            Hash localHash;
            try {
                localHash = ctx.routerHash();
            } catch (RuntimeException re) {
                return Tunnels.unavailable("probe-threw-" + re.getClass().getSimpleName());
            }
            if (localHash == null) {
                return Tunnels.unavailable("local-hash-null");
            }
            TunnelPool inboundPool = null;
            TunnelPool outboundPool = null;
            try {
                inboundPool = ctx.tunnelManager().getInboundPool(clientDbid);
            } catch (RuntimeException re) {
                inboundPool = null;
            }
            try {
                outboundPool = ctx.tunnelManager().getOutboundPool(clientDbid);
            } catch (RuntimeException re) {
                outboundPool = null;
            }
            boolean inboundPresent = inboundPool != null;
            boolean outboundPresent = outboundPool != null;
            boolean clientResolved = inboundPresent && outboundPresent;
            int inboundCount = 0;
            int outboundCount = 0;
            boolean inboundExact = false;
            boolean outboundExact = false;
            boolean inboundZero = false;
            boolean outboundZero = false;
            if (inboundPresent) {
                List<TunnelInfo> tunnels = null;
                try {
                    tunnels = inboundPool.listTunnels();
                } catch (RuntimeException re) {
                    tunnels = null;
                }
                if (tunnels != null) {
                    inboundCount = boundedTunnelCount(tunnels);
                    int checked = 0;
                    for (TunnelInfo info : tunnels) {
                        if (checked >= 8 || info == null) {
                            break;
                        }
                        checked++;
                        if (tunnelIsZeroHop(info)) {
                            inboundZero = true;
                        }
                        if (tunnelIsExactOneViaC(info, localHash, routerC)) {
                            inboundExact = true;
                        }
                    }
                }
            }
            if (outboundPresent) {
                List<TunnelInfo> tunnels = null;
                try {
                    tunnels = outboundPool.listTunnels();
                } catch (RuntimeException re) {
                    tunnels = null;
                }
                if (tunnels != null) {
                    outboundCount = boundedTunnelCount(tunnels);
                    int checked = 0;
                    for (TunnelInfo info : tunnels) {
                        if (checked >= 8 || info == null) {
                            break;
                        }
                        checked++;
                        if (tunnelIsZeroHop(info)) {
                            outboundZero = true;
                        }
                        if (tunnelIsExactOneViaC(info, localHash, routerC)) {
                            outboundExact = true;
                        }
                    }
                }
            }
            return new Tunnels(clientResolved, inboundPresent, outboundPresent,
                inboundCount, outboundCount, inboundExact, outboundExact,
                inboundZero, outboundZero, null);
        } catch (RuntimeException e) {
            return Tunnels.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
