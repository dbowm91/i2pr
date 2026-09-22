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
// Plan 242 §6 — extended client-tunnel observation. Adds a separate
// `snapshotClientTunnelsExtended(ctx, clientDbid, routerA, routerB, routerC)`
// accessor that records, for each direction's installed pool,
// tunnel-length-including-local, remote-hop count, first/last remote role as
// `B` / `C` / `other-controlled` / `unknown`, plus `contains_b`, `contains_c`,
// `exact_one_remote_hop`, and the retained `exact_one_remote_hop_via_c`
// diagnostic. No arbitrary peer hashes are persisted; only the bounded role
// label is emitted. An `unknown` peer inside the controlled topology is a
// fail-closed testable signal (the harness later decides whether it stops).
// The original `snapshotClientTunnels` accessor stays untouched for
// backwards compatibility with Plans 227/228/229/230/231/240/241.
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

    // Plan 242 §6 — extended client-pool observation. Bounded labels and
    // counts only; never peer hashes, key material, payloads, or raw log
    // text. Role mapping is by controlled-router identity; any peer that
    // is neither A, B, nor C is reported as `unknown` so the harness can
    // fail closed before using the row. The accessor appends to the same
    // TSV shape as the basic Tunnels accessor; old readers ignore the
    // additional fields.
    public static final class Role {
        public static final String B = "B";
        public static final String C = "C";
        public static final String OTHER_CONTROLLED = "other-controlled";
        public static final String UNKNOWN = "unknown";
        public static final String NONE = "none";
    }

    public static final class ExtendedTunnels {
        public final Tunnels base;
        // Tunnel length including the local router (one value per
        // direction, capped at 4 — Java caps tunnel length below 8 and
        // a controlled topology with 3 routers cannot produce longer
        // paths anyway).
        public final int inboundTunnelLengthIncludingLocal;
        public final int outboundTunnelLengthIncludingLocal;
        // Remote hop count only (excluding the local router).
        public final int inboundRemoteHopCount;
        public final int outboundRemoteHopCount;
        // First and last remote hop role, by controlled-router identity.
        // "none" when zero remote hops exist; "unknown" for an out-of-
        // controlled-topology peer.
        public final String inboundFirstRemoteRole;
        public final String inboundLastRemoteRole;
        public final String outboundFirstRemoteRole;
        public final String outboundLastRemoteRole;
        // Boolean role-membership facts (any tunnel that includes the
        // role in its remote hops counts, even on a partially-controlled
        // path).
        public final boolean inboundContainsB;
        public final boolean outboundContainsB;
        public final boolean inboundContainsC;
        public final boolean outboundContainsC;
        // Genuine one-remote-hop status (length = 2 with the local
        // router at one end and exactly one non-local remote hop).
        public final boolean inboundExactOneRemoteHop;
        public final boolean outboundExactOneRemoteHop;
        // Diagnostic retained from the basic accessor: genuine one
        // remote hop via the controlled Router C only.
        public final boolean inboundExactOneRemoteHopViaC;
        public final boolean outboundExactOneRemoteHopViaC;
        // Counts (any peer in the controlled topology, plus overall
        // zero-hop counts already in `base`). These are diagnostic only
        // and do not change the gate semantics.
        public final int inboundNonzeroCount;
        public final int outboundNonzeroCount;
        // Any peer observed that is neither A, B, nor C. The harness
        // classifies this as P242-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL.
        public final boolean inboundUnexpectedPeerObserved;
        public final boolean outboundUnexpectedPeerObserved;
        public final String error;

        private ExtendedTunnels(
                Tunnels base,
                int inboundTunnelLengthIncludingLocal,
                int outboundTunnelLengthIncludingLocal,
                int inboundRemoteHopCount,
                int outboundRemoteHopCount,
                String inboundFirstRemoteRole,
                String inboundLastRemoteRole,
                String outboundFirstRemoteRole,
                String outboundLastRemoteRole,
                boolean inboundContainsB,
                boolean outboundContainsB,
                boolean inboundContainsC,
                boolean outboundContainsC,
                boolean inboundExactOneRemoteHop,
                boolean outboundExactOneRemoteHop,
                boolean inboundExactOneRemoteHopViaC,
                boolean outboundExactOneRemoteHopViaC,
                int inboundNonzeroCount,
                int outboundNonzeroCount,
                boolean inboundUnexpectedPeerObserved,
                boolean outboundUnexpectedPeerObserved,
                String error) {
            this.base = base;
            this.inboundTunnelLengthIncludingLocal = inboundTunnelLengthIncludingLocal;
            this.outboundTunnelLengthIncludingLocal = outboundTunnelLengthIncludingLocal;
            this.inboundRemoteHopCount = inboundRemoteHopCount;
            this.outboundRemoteHopCount = outboundRemoteHopCount;
            this.inboundFirstRemoteRole = inboundFirstRemoteRole;
            this.inboundLastRemoteRole = inboundLastRemoteRole;
            this.outboundFirstRemoteRole = outboundFirstRemoteRole;
            this.outboundLastRemoteRole = outboundLastRemoteRole;
            this.inboundContainsB = inboundContainsB;
            this.outboundContainsB = outboundContainsB;
            this.inboundContainsC = inboundContainsC;
            this.outboundContainsC = outboundContainsC;
            this.inboundExactOneRemoteHop = inboundExactOneRemoteHop;
            this.outboundExactOneRemoteHop = outboundExactOneRemoteHop;
            this.inboundExactOneRemoteHopViaC = inboundExactOneRemoteHopViaC;
            this.outboundExactOneRemoteHopViaC = outboundExactOneRemoteHopViaC;
            this.inboundNonzeroCount = inboundNonzeroCount;
            this.outboundNonzeroCount = outboundNonzeroCount;
            this.inboundUnexpectedPeerObserved = inboundUnexpectedPeerObserved;
            this.outboundUnexpectedPeerObserved = outboundUnexpectedPeerObserved;
            this.error = error;
        }

        public static ExtendedTunnels unavailable(Tunnels base, String reason) {
            return new ExtendedTunnels(
                base == null ? Tunnels.unavailable(reason) : base,
                0, 0, 0, 0,
                Role.NONE, Role.NONE, Role.NONE, Role.NONE,
                false, false, false, false,
                false, false, false, false,
                0, 0,
                false, false,
                reason);
        }
    }

    /** Plan 242 §3/§6 — classify a remote hop hash against the controlled topology. */
    private static String classifyPeerRole(Hash peer, Hash routerA, Hash routerB, Hash routerC) {
        try {
            if (peer == null) return Role.NONE;
            if (routerB != null && peer.equals(routerB)) return Role.B;
            if (routerC != null && peer.equals(routerC)) return Role.C;
            if (routerA != null && peer.equals(routerA)) return Role.OTHER_CONTROLLED;
            return Role.UNKNOWN;
        } catch (RuntimeException re) {
            return Role.UNKNOWN;
        }
    }

    private static int remoteHopCount(TunnelInfo info, Hash localHash) {
        try {
            int length = info.getLength();
            int remote = 0;
            for (int i = 0; i < length; i++) {
                Hash peer = info.getPeer(i);
                if (peer == null) continue;
                if (localHash != null && peer.equals(localHash)) continue;
                remote++;
            }
            return remote;
        } catch (RuntimeException re) {
            return 0;
        }
    }

    private static boolean pathContainsRole(
            TunnelInfo info, Hash localHash, Hash target, boolean requireC) {
        try {
            int length = info.getLength();
            for (int i = 0; i < length; i++) {
                Hash peer = info.getPeer(i);
                if (peer == null) continue;
                if (localHash != null && peer.equals(localHash)) continue;
                if (requireC) {
                    if (target != null && peer.equals(target)) return true;
                } else {
                    // For "exact one remote hop via X", length must be 2
                    // and the non-local peer must equal X.
                    if (length != 2) return false;
                    if (target != null && peer.equals(target)) return true;
                }
            }
            return false;
        } catch (RuntimeException re) {
            return false;
        }
    }

    /**
     * Read-only extended client-tunnel snapshot. Records bounded role and
     * length facts only; never peer hashes, key material, or raw log text.
     * Identical control semantics as the basic accessor (`base` is computed
     * first) so a Plan-227-style row still drives the original gate.
     * Never installs, builds, or mutates tunnels.
     */
    public static ExtendedTunnels snapshotClientTunnelsExtended(
            RouterContext ctx,
            Hash clientDbid,
            Hash routerA,
            Hash routerB,
            Hash routerC) {
        if (ctx == null || clientDbid == null
            || routerA == null || routerB == null || routerC == null) {
            return ExtendedTunnels.unavailable(
                Tunnels.unavailable("null-argument"), "null-argument");
        }
        Tunnels base;
        try {
            base = snapshotClientTunnels(ctx, clientDbid, routerC);
        } catch (RuntimeException re) {
            return ExtendedTunnels.unavailable(
                Tunnels.unavailable("base-threw"),
                "base-threw-" + re.getClass().getSimpleName());
        }
        if (base.error != null) {
            return ExtendedTunnels.unavailable(base, base.error);
        }
        try {
            Hash localHash = ctx.routerHash();
            // The helper IS the local router (Router A). We still pass it
            // through so that probes run from a non-A controlled router
            // never mislabel the local router as `other-controlled`.
            Hash helperLocal = routerA;
            TunnelPool inboundPool = ctx.tunnelManager().getInboundPool(clientDbid);
            TunnelPool outboundPool = ctx.tunnelManager().getOutboundPool(clientDbid);
            int[] innLen = {0};
            int[] outLen = {0};
            int[] innRemote = {0};
            int[] outRemote = {0};
            String[] innFirst = {Role.NONE};
            String[] innLast = {Role.NONE};
            String[] outFirst = {Role.NONE};
            String[] outLast = {Role.NONE};
            boolean[] innContainsB = {false};
            boolean[] outContainsB = {false};
            boolean[] innContainsC = {false};
            boolean[] outContainsC = {false};
            boolean[] innExactC = {false};
            boolean[] outExactC = {false};
            boolean[] innUnexpect = {false};
            boolean[] outUnexpect = {false};
            int[] innNonzero = {0};
            int[] outNonzero = {0};
            // Track `exact_one_remote_hop` (length 2 with one non-local
            // remote) too — gated by include of any controlled peer.
            boolean[] innExactOne = {false};
            boolean[] outExactOne = {false};

            if (inboundPool != null) {
                List<TunnelInfo> tunnels = inboundPool.listTunnels();
                if (tunnels != null && !tunnels.isEmpty()) {
                    TunnelInfo first = tunnels.get(0);
                    if (first != null) {
                        innLen[0] = boundedLength(first.getLength());
                        int remote = remoteHopCount(first, helperLocal);
                        innRemote[0] = remote;
                        if (remote > 0) {
                            String[] firstLastRoles = firstLastRoles(first, helperLocal, routerA, routerB, routerC);
                            innFirst[0] = firstLastRoles[0];
                            innLast[0] = firstLastRoles[1];
                        }
                    }
                    int checked = 0;
                    for (TunnelInfo info : tunnels) {
                        if (checked >= 8 || info == null) break;
                        checked++;
                        if (info.getLength() > 1) {
                            innNonzero[0]++;
                        }
                        if (pathContainsRole(info, helperLocal, routerB, true)) innContainsB[0] = true;
                        if (pathContainsRole(info, helperLocal, routerC, true)) innContainsC[0] = true;
                        if (tunnelIsExactOneViaC(info, helperLocal, routerC)) innExactC[0] = true;
                        if (tunnelIsExactOne(info, helperLocal, routerA, routerB, routerC)) innExactOne[0] = true;
                        if (pathHasUnexpected(info, helperLocal, routerA, routerB, routerC)) innUnexpect[0] = true;
                    }
                }
            }
            if (outboundPool != null) {
                List<TunnelInfo> tunnels = outboundPool.listTunnels();
                if (tunnels != null && !tunnels.isEmpty()) {
                    TunnelInfo first = tunnels.get(0);
                    if (first != null) {
                        outLen[0] = boundedLength(first.getLength());
                        int remote = remoteHopCount(first, helperLocal);
                        outRemote[0] = remote;
                        if (remote > 0) {
                            String[] firstLastRoles = firstLastRoles(first, helperLocal, routerA, routerB, routerC);
                            outFirst[0] = firstLastRoles[0];
                            outLast[0] = firstLastRoles[1];
                        }
                    }
                    int checked = 0;
                    for (TunnelInfo info : tunnels) {
                        if (checked >= 8 || info == null) break;
                        checked++;
                        if (info.getLength() > 1) {
                            outNonzero[0]++;
                        }
                        if (pathContainsRole(info, helperLocal, routerB, true)) outContainsB[0] = true;
                        if (pathContainsRole(info, helperLocal, routerC, true)) outContainsC[0] = true;
                        if (tunnelIsExactOneViaC(info, helperLocal, routerC)) outExactC[0] = true;
                        if (tunnelIsExactOne(info, helperLocal, routerA, routerB, routerC)) outExactOne[0] = true;
                        if (pathHasUnexpected(info, helperLocal, routerA, routerB, routerC)) outUnexpect[0] = true;
                    }
                }
            }
            return new ExtendedTunnels(
                base,
                innLen[0], outLen[0],
                innRemote[0], outRemote[0],
                innFirst[0], innLast[0], outFirst[0], outLast[0],
                innContainsB[0], outContainsB[0],
                innContainsC[0], outContainsC[0],
                innExactOne[0], outExactOne[0],
                innExactC[0], outExactC[0],
                innNonzero[0], outNonzero[0],
                innUnexpect[0], outUnexpect[0],
                null);
        } catch (RuntimeException e) {
            return ExtendedTunnels.unavailable(
                base, "extended-threw-" + e.getClass().getSimpleName());
        }
    }

    private static int boundedLength(int length) {
        // Cap to 4 — bounded observation. Lengths beyond 8 are not
        // producible by stock Java given 3 controlled peers.
        if (length < 0) return 0;
        if (length > 4) return 4;
        return length;
    }

    private static boolean tunnelIsExactOne(
            TunnelInfo info, Hash localHash,
            Hash routerA, Hash routerB, Hash routerC) {
        try {
            if (info.getLength() != 2) return false;
            for (int i = 0; i < 2; i++) {
                Hash peer = info.getPeer(i);
                if (peer == null) return false;
                if (localHash != null && peer.equals(localHash)) continue;
                // The non-local peer must be one of the controlled routers,
                // and the tunnel must include only one remote hop.
                return peer.equals(routerA) || peer.equals(routerB) || peer.equals(routerC);
            }
            return false;
        } catch (RuntimeException re) {
            return false;
        }
    }

    private static String[] firstLastRoles(
            TunnelInfo info, Hash localHash,
            Hash routerA, Hash routerB, Hash routerC) {
        String[] result = {Role.NONE, Role.NONE};
        try {
            int length = info.getLength();
            String firstSeen = Role.NONE;
            String lastSeen = Role.NONE;
            for (int i = 0; i < length; i++) {
                Hash peer = info.getPeer(i);
                if (peer == null) continue;
                if (localHash != null && peer.equals(localHash)) continue;
                String role = classifyPeerRole(peer, routerA, routerB, routerC);
                if (firstSeen.equals(Role.NONE)) {
                    firstSeen = role;
                }
                lastSeen = role;
            }
            result[0] = firstSeen;
            result[1] = lastSeen;
            return result;
        } catch (RuntimeException re) {
            return result;
        }
    }

    private static boolean pathHasUnexpected(
            TunnelInfo info, Hash localHash,
            Hash routerA, Hash routerB, Hash routerC) {
        try {
            int length = info.getLength();
            for (int i = 0; i < length; i++) {
                Hash peer = info.getPeer(i);
                if (peer == null) continue;
                if (localHash != null && peer.equals(localHash)) continue;
                String role = classifyPeerRole(peer, routerA, routerB, routerC);
                if (Role.UNKNOWN.equals(role)) return true;
            }
            return false;
        } catch (RuntimeException re) {
            return false;
        }
    }
}
