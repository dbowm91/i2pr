// Plan 229 WP B/C/D — test-only read-only transit-peer eligibility,
// exploratory-settings, and exploratory-tunnel snapshot probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java`, `P227Probe.java`, and `P228Probe.java`.
// Never compiled into or against the exact-pinned source checkout.
// It MUST NOT patch or replace any Java I2P class, MUST NOT mutate
// NetDB/profile/tunnel/banlist state, MUST NOT use reflection or
// private-field access, MUST NOT store RouterInfos, MUST NOT install or
// build tunnels, MUST NOT force connections, MUST NOT promote tiers,
// MUST NOT create profiles (never `addProfile`, never
// `getOrCreateProfile*` from a probe path).
//
// Read-only public-API surface:
//   - transit peer: `KademliaNetworkDatabaseFacade.lookupLocallyWithoutValidation`
//     (raw presence) + `lookupRouterInfoLocally` (validated presence),
//     `RouterContext.profileOrganizer().selectAllPeers()` membership plus
//     `getProfileNonblocking(Hash)` (read-only, never creates),
//     `isSelectable(Hash)`, `countNotFailingPeers()`,
//     `RouterContext.banlist().isBanlisted(Hash)`,
//     `ProfileOrganizer.isFailing(Hash)` as the public read-only
//     reachability-failure signal (`PeerProfile.wasUnreachable()` is
//     package-private and unavailable to this out-of-tree probe),
//     `RouterInfo.getCapabilities()` floodfill-membership check;
//   - exploratory settings: `TunnelManagerFacade.getInboundSettings()` +
//     `getOutboundSettings()` with `TunnelPoolSettings.getLength()`,
//     `getLengthVariance()`, `getQuantity()` (observation only, never
//     `setLength` / `setQuantity` / `setInboundSettings`);
//   - exploratory tunnels: `getInboundExploratoryPool()` /
//     `getOutboundExploratoryPool()` + `TunnelPool.listTunnels()`
//     read-only with `TunnelInfo.getLength()` + `getPeer(int)` C-membership
//     check (bounded to 8 tunnels).
//
// All facts are bounded (tunnel lists capped at 8, counts clamped to
// 0..64) and carry no key material, payloads, peer paths, or raw log
// text. No peer hash is echoed except the caller-supplied Router-C echo
// already performed by the launcher.
package net.i2p.router.networkdb.kademlia;

import java.util.List;
import java.util.Set;
import net.i2p.data.Hash;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.RouterContext;
import net.i2p.router.TunnelInfo;
import net.i2p.router.TunnelPoolSettings;
import net.i2p.router.peermanager.PeerProfile;
import net.i2p.router.tunnel.pool.TunnelPool;

public final class P229Probe {

    private P229Probe() {
    }

    public static final class TransitPeer {
        public final boolean mainRawPresent;
        public final boolean mainValidPresent;
        public final boolean profilePresent;
        public final boolean selectable;
        public final boolean banlisted;
        public final boolean unreachable;
        public final boolean capsHasF;
        public final int profileCount;
        public final int notFailingCount;
        public final String error;

        private TransitPeer(
                boolean mainRawPresent,
                boolean mainValidPresent,
                boolean profilePresent,
                boolean selectable,
                boolean banlisted,
                boolean unreachable,
                boolean capsHasF,
                int profileCount,
                int notFailingCount,
                String error) {
            this.mainRawPresent = mainRawPresent;
            this.mainValidPresent = mainValidPresent;
            this.profilePresent = profilePresent;
            this.selectable = selectable;
            this.banlisted = banlisted;
            this.unreachable = unreachable;
            this.capsHasF = capsHasF;
            this.profileCount = profileCount;
            this.notFailingCount = notFailingCount;
            this.error = error;
        }

        public static TransitPeer unavailable(String reason) {
            return new TransitPeer(false, false, false, false, false, true, true, 0, 0, reason);
        }

        public static TransitPeer ok(
                boolean mainRawPresent,
                boolean mainValidPresent,
                boolean profilePresent,
                boolean selectable,
                boolean banlisted,
                boolean unreachable,
                boolean capsHasF,
                int profileCount,
                int notFailingCount) {
            return new TransitPeer(mainRawPresent, mainValidPresent, profilePresent,
                selectable, banlisted, unreachable, capsHasF,
                profileCount, notFailingCount, null);
        }
    }

    public static final class ExploratorySettings {
        public final int inboundLength;
        public final int inboundVariance;
        public final int inboundQuantity;
        public final int outboundLength;
        public final int outboundVariance;
        public final int outboundQuantity;
        public final String error;

        private ExploratorySettings(
                int inboundLength,
                int inboundVariance,
                int inboundQuantity,
                int outboundLength,
                int outboundVariance,
                int outboundQuantity,
                String error) {
            this.inboundLength = inboundLength;
            this.inboundVariance = inboundVariance;
            this.inboundQuantity = inboundQuantity;
            this.outboundLength = outboundLength;
            this.outboundVariance = outboundVariance;
            this.outboundQuantity = outboundQuantity;
            this.error = error;
        }

        public static ExploratorySettings unavailable(String reason) {
            return new ExploratorySettings(-1, -1, -1, -1, -1, -1, reason);
        }

        public static ExploratorySettings ok(
                int inboundLength,
                int inboundVariance,
                int inboundQuantity,
                int outboundLength,
                int outboundVariance,
                int outboundQuantity) {
            return new ExploratorySettings(inboundLength, inboundVariance, inboundQuantity,
                outboundLength, outboundVariance, outboundQuantity, null);
        }
    }

    public static final class ExploratoryTunnels {
        public final int inboundExploratoryCount;
        public final int outboundExploratoryCount;
        public final int inboundNonzeroCount;
        public final int outboundNonzeroCount;
        public final boolean inboundCPresent;
        public final boolean outboundCPresent;
        public final boolean zeroHopFallbackPresent;
        public final String error;

        private ExploratoryTunnels(
                int inboundExploratoryCount,
                int outboundExploratoryCount,
                int inboundNonzeroCount,
                int outboundNonzeroCount,
                boolean inboundCPresent,
                boolean outboundCPresent,
                boolean zeroHopFallbackPresent,
                String error) {
            this.inboundExploratoryCount = inboundExploratoryCount;
            this.outboundExploratoryCount = outboundExploratoryCount;
            this.inboundNonzeroCount = inboundNonzeroCount;
            this.outboundNonzeroCount = outboundNonzeroCount;
            this.inboundCPresent = inboundCPresent;
            this.outboundCPresent = outboundCPresent;
            this.zeroHopFallbackPresent = zeroHopFallbackPresent;
            this.error = error;
        }

        public static ExploratoryTunnels unavailable(String reason) {
            return new ExploratoryTunnels(0, 0, 0, 0, false, false, false, reason);
        }

        public static ExploratoryTunnels ok(
                int inboundExploratoryCount,
                int outboundExploratoryCount,
                int inboundNonzeroCount,
                int outboundNonzeroCount,
                boolean inboundCPresent,
                boolean outboundCPresent,
                boolean zeroHopFallbackPresent) {
            return new ExploratoryTunnels(inboundExploratoryCount, outboundExploratoryCount,
                inboundNonzeroCount, outboundNonzeroCount,
                inboundCPresent, outboundCPresent, zeroHopFallbackPresent, null);
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

    /**
     * Read-only Router-C transit-peer snapshot on this router's main NetDB.
     * Uses only public read-only accessors; never creates profiles (only
     * `selectAllPeers` membership plus `getProfileNonblocking`), never
     * promotes tiers, never forces connections, never stores RouterInfos.
     * Fail-closed: unknown state reports the gate-failing value
     * (unreachable=true, capsHasF=true) so the harness stops instead of
     * silently manufacturing the prerequisite.
     */
    public static TransitPeer snapshotTransitPeer(
            RouterContext ctx,
            KademliaNetworkDatabaseFacade mainFacade,
            Hash routerC) {
        if (ctx == null || mainFacade == null || routerC == null) {
            return TransitPeer.unavailable("null-argument");
        }
        try {
            boolean rawPresent = false;
            try {
                rawPresent = mainFacade.lookupLocallyWithoutValidation(routerC) != null;
            } catch (RuntimeException re) {
                rawPresent = false;
            }
            boolean validPresent = false;
            RouterInfo info = null;
            try {
                info = mainFacade.lookupRouterInfoLocally(routerC);
                validPresent = info != null;
            } catch (RuntimeException re) {
                validPresent = false;
                info = null;
            }
            boolean profilePresent = false;
            int profileCount = 0;
            try {
                Set<Hash> all = ctx.profileOrganizer().selectAllPeers();
                if (all != null) {
                    profileCount = clampCount(all.size());
                    if (all.contains(routerC)) {
                        PeerProfile profile = null;
                        try {
                            profile = ctx.profileOrganizer().getProfileNonblocking(routerC);
                        } catch (RuntimeException re) {
                            profile = null;
                        }
                        profilePresent = profile != null;
                    }
                }
            } catch (RuntimeException re) {
                return TransitPeer.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            boolean selectable = false;
            try {
                selectable = ctx.profileOrganizer().isSelectable(routerC);
            } catch (RuntimeException re) {
                return TransitPeer.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            boolean banlisted = false;
            try {
                banlisted = ctx.banlist().isBanlisted(routerC);
            } catch (RuntimeException re) {
                banlisted = false;
            }
            // Reachability-failure signal through the public read-only
            // organizer facade. `PeerProfile.wasUnreachable()` is
            // package-private and unavailable to this out-of-tree probe;
            // `isFailing` is the public equivalent. Fail-closed: any
            // read error reports unreachable so the harness stops.
            boolean unreachable = true;
            try {
                unreachable = ctx.profileOrganizer().isFailing(routerC);
            } catch (RuntimeException re) {
                unreachable = true;
            }
            boolean capsHasF = true;
            try {
                if (info != null) {
                    String caps = info.getCapabilities();
                    capsHasF = caps != null && caps.indexOf('f') >= 0;
                } else {
                    capsHasF = true;
                }
            } catch (RuntimeException re) {
                capsHasF = true;
            }
            int notFailing = 0;
            try {
                notFailing = clampCount(ctx.profileOrganizer().countNotFailingPeers());
            } catch (RuntimeException re) {
                notFailing = 0;
            }
            return TransitPeer.ok(rawPresent, validPresent, profilePresent,
                selectable, banlisted, unreachable, capsHasF,
                profileCount, notFailing);
        } catch (RuntimeException e) {
            return TransitPeer.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    /**
     * Read-only effective exploratory-pool settings snapshot.
     * Observation only via `getInboundSettings` / `getOutboundSettings`;
     * never calls any `set*Settings` mutator.
     */
    public static ExploratorySettings snapshotExploratorySettings(RouterContext ctx) {
        if (ctx == null) {
            return ExploratorySettings.unavailable("null-argument");
        }
        try {
            TunnelPoolSettings inbound = null;
            TunnelPoolSettings outbound = null;
            try {
                inbound = ctx.tunnelManager().getInboundSettings();
            } catch (RuntimeException re) {
                return ExploratorySettings.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            try {
                outbound = ctx.tunnelManager().getOutboundSettings();
            } catch (RuntimeException re) {
                return ExploratorySettings.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            if (inbound == null || outbound == null) {
                return ExploratorySettings.unavailable("settings-null");
            }
            int inLen;
            int inVar;
            int inQty;
            int outLen;
            int outVar;
            int outQty;
            try {
                inLen = inbound.getLength();
                inVar = inbound.getLengthVariance();
                inQty = inbound.getQuantity();
                outLen = outbound.getLength();
                outVar = outbound.getLengthVariance();
                outQty = outbound.getQuantity();
            } catch (RuntimeException re) {
                return ExploratorySettings.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            return ExploratorySettings.ok(inLen, inVar, inQty, outLen, outVar, outQty);
        } catch (RuntimeException e) {
            return ExploratorySettings.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    private static boolean tunnelContainsPeer(TunnelInfo info, Hash peer) {
        try {
            int length = info.getLength();
            for (int i = 0; i < length && i < 8; i++) {
                try {
                    Hash hop = info.getPeer(i);
                    if (hop != null && hop.equals(peer)) {
                        return true;
                    }
                } catch (RuntimeException re) {
                    continue;
                }
            }
        } catch (RuntimeException re) {
            return false;
        }
        return false;
    }

    /**
     * Read-only exploratory-tunnel snapshot for the paired-tunnel gate.
     * Counts genuine non-zero tunnels (`getLength() &gt; 1`) and records
     * Router-C membership read-only; no peer hashes or paths are exposed
     * beyond the caller-supplied Router-C echo performed by the launcher.
     * Never builds, installs, or mutates tunnels.
     */
    public static ExploratoryTunnels snapshotExploratoryTunnels(
            RouterContext ctx, Hash routerC) {
        if (ctx == null || routerC == null) {
            return ExploratoryTunnels.unavailable("null-argument");
        }
        try {
            TunnelPool inboundPool = null;
            TunnelPool outboundPool = null;
            try {
                inboundPool = ctx.tunnelManager().getInboundExploratoryPool();
            } catch (RuntimeException re) {
                inboundPool = null;
            }
            try {
                outboundPool = ctx.tunnelManager().getOutboundExploratoryPool();
            } catch (RuntimeException re) {
                outboundPool = null;
            }
            if (inboundPool == null && outboundPool == null) {
                return ExploratoryTunnels.unavailable("exploratory-pools-null");
            }
            int inCount = 0;
            int outCount = 0;
            int inNonzero = 0;
            int outNonzero = 0;
            boolean inC = false;
            boolean outC = false;
            boolean zeroHop = false;
            if (inboundPool != null) {
                List<TunnelInfo> tunnels = null;
                try {
                    tunnels = inboundPool.listTunnels();
                } catch (RuntimeException re) {
                    tunnels = null;
                }
                if (tunnels != null) {
                    int checked = 0;
                    for (TunnelInfo info : tunnels) {
                        if (checked >= 8 || info == null) {
                            break;
                        }
                        checked++;
                        inCount++;
                        boolean nonzero = false;
                        try {
                            nonzero = info.getLength() > 1;
                        } catch (RuntimeException re) {
                            nonzero = false;
                        }
                        if (nonzero) {
                            inNonzero++;
                        } else {
                            zeroHop = true;
                        }
                        if (tunnelContainsPeer(info, routerC)) {
                            inC = true;
                        }
                    }
                }
            }
            if (outboundPool != null) {
                List<TunnelInfo> tunnels = null;
                try {
                    tunnels = outboundPool.listTunnels();
                } catch (RuntimeException re) {
                    tunnels = null;
                }
                if (tunnels != null) {
                    int checked = 0;
                    for (TunnelInfo info : tunnels) {
                        if (checked >= 8 || info == null) {
                            break;
                        }
                        checked++;
                        outCount++;
                        boolean nonzero = false;
                        try {
                            nonzero = info.getLength() > 1;
                        } catch (RuntimeException re) {
                            nonzero = false;
                        }
                        if (nonzero) {
                            outNonzero++;
                        } else {
                            zeroHop = true;
                        }
                        if (tunnelContainsPeer(info, routerC)) {
                            outC = true;
                        }
                    }
                }
            }
            return ExploratoryTunnels.ok(
                Math.min(inCount, 8), Math.min(outCount, 8),
                Math.min(inNonzero, 8), Math.min(outNonzero, 8),
                inC, outC, zeroHop);
        } catch (RuntimeException e) {
            return ExploratoryTunnels.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
