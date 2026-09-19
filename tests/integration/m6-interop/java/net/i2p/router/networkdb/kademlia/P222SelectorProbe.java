// Plan 222 WP B — test-only exact client-lookup preflight probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P220SelectorProbe.java`. Never compiled
// into or against the exact-pinned source checkout. It MUST NOT patch
// or replace any Java I2P class, MUST NOT mutate NetDB state, MUST NOT
// use reflection or private-field access.
//
// Production equivalence (exact-pinned Java I2P 2.13.0
// `9134f808337b401e8e53c73734c81fab04280c9d`, verified by decompiling
// `IterativeSearchJob` constructor + `runJob` + `FloodfillPeerSelector`
// overloads against the staged jars):
//   - `_rkey = ctx.routingKeyGenerator().getRoutingKey(key);`
//   - `default_total = 3 if facade.floodfillEnabled() &&
//      ctx.router().getUptime() > 30 min (1800000 ms) else 5`
//   - `total_search_limit = ctx.getProperty("netdb.searchLimit", default_total)`
//   - `selector_width = total_search_limit + EXTRA_PEERS` where
//     `EXTRA_PEERS = 1` (the `iconst_1; iadd` in `runJob`)
//   - selector call is the 3-argument overload
//     `selectFloodfillParticipants(routingKey, selectorWidth, kbs)`
//     which internally supplies the router self-ignore singleton;
//     the probe must not pass an empty exclude set as an authoritative
//     input.
//   - the client DB path is `ctx.clientNetDb(clientDbid)` where
//     `clientDbid` is the helper source Destination hash
//     (`_from.calculateHash()` in OCMOSJ); the probe uses the client
//     facade's live k-buckets and peer selector read-only.
//   - client `getAllRouters()` returns empty in pinned source, so an
//     empty exact selector means CLIENT-NETDB-NO-LOOKUP-PEER with no
//     fallback rescue.
//
// The P220 probe is retained as historical evidence only and must not
// satisfy any P222 terminal.
package net.i2p.router.networkdb.kademlia;

import java.util.ArrayList;
import java.util.List;
import net.i2p.data.Hash;
import net.i2p.data.LeaseSet;
import net.i2p.kademlia.KBucketSet;
import net.i2p.router.NetworkDatabaseFacade;
import net.i2p.router.RouterContext;

public final class P222SelectorProbe {

    /**
     * EXTRA_PEERS equivalent value 1, matching pinned
     * `IterativeSearchJob.EXTRA_PEERS = 1` (`iconst_1; iadd` before the
     * `selectFloodfillParticipants` call in `runJob`). Documented here
     * so the static checker can require it; the selector width below
     * is always `total_search_limit + EXTRA_PEERS`.
     */
    public static final int EXTRA_PEERS = 1;

    /** Thirty minutes in milliseconds, matching the `1800000l` compare. */
    public static final long FLOODFILL_UPTIME_THRESHOLD_MS = 1800000L;

    /** Property key matching pinned `ctx.getProperty("netdb.searchLimit", ...)`. */
    public static final String PROP_SEARCH_LIMIT = "netdb.searchLimit";

    private P222SelectorProbe() {
    }

    public static final class Result {
        /** True when `clientNetDb(clientDbid)` returned a facade. */
        public final boolean clientDbResolved;
        /** True when that facade `isClientDb()` (not main fallback). */
        public final boolean clientDbIsClient;
        /** Target hash hex (lowercase) as supplied. */
        public final String targetHashHex;
        /** Routing key hex derived via `routingKeyGenerator().getRoutingKey`. */
        public final String routingKeyHex;
        /** Whether the routing key differs from the raw target (normally true). */
        public final boolean routingKeyDiffers;
        /** `facade.floodfillEnabled()` value used for the width decision. */
        public final boolean facadeFloodfillEnabled;
        /** `ctx.router().getUptime()` milliseconds at probe time. */
        public final long routerUptimeMs;
        /** Effective `netdb.searchLimit` after the default rule. */
        public final int netdbSearchLimitEffective;
        /** `selector_extra_peers` (always 1). */
        public final int selectorExtraPeers;
        /** Exact selector width (`netdb.searchLimit` + EXTRA_PEERS). */
        public final int selectorWidth;
        /** Live k-bucket population observed at probe time. */
        public final int kbucketSize;
        /** Selector output for the routing key (possibly empty, bounded). */
        public final List<Hash> selected;
        /** True when Router B is in the selected set. */
        public final boolean containsB;
        /** True when the client facade holds the target LeaseSet locally. */
        public final boolean targetLsPresentBeforeSend;
        /** Simple class name of the present LeaseSet, or null. */
        public final String targetLsType;
        /** Machine-readable reason when the probe cannot observe. Null on success. */
        public final String error;

        private Result(
                boolean clientDbResolved,
                boolean clientDbIsClient,
                String targetHashHex,
                String routingKeyHex,
                boolean routingKeyDiffers,
                boolean facadeFloodfillEnabled,
                long routerUptimeMs,
                int netdbSearchLimitEffective,
                int selectorExtraPeers,
                int selectorWidth,
                int kbucketSize,
                List<Hash> selected,
                boolean containsB,
                boolean targetLsPresentBeforeSend,
                String targetLsType,
                String error) {
            this.clientDbResolved = clientDbResolved;
            this.clientDbIsClient = clientDbIsClient;
            this.targetHashHex = targetHashHex;
            this.routingKeyHex = routingKeyHex;
            this.routingKeyDiffers = routingKeyDiffers;
            this.facadeFloodfillEnabled = facadeFloodfillEnabled;
            this.routerUptimeMs = routerUptimeMs;
            this.netdbSearchLimitEffective = netdbSearchLimitEffective;
            this.selectorExtraPeers = selectorExtraPeers;
            this.selectorWidth = selectorWidth;
            this.kbucketSize = kbucketSize;
            this.selected = selected;
            this.containsB = containsB;
            this.targetLsPresentBeforeSend = targetLsPresentBeforeSend;
            this.targetLsType = targetLsType;
            this.error = error;
        }

        public static Result unavailable(String reason) {
            return new Result(false, false, null, null, false, false, -1, -1, EXTRA_PEERS, -1,
                    -1, new ArrayList<Hash>(), false, false, null, reason);
        }
    }

    private static String hexLower(byte[] raw) {
        StringBuilder out = new StringBuilder(raw.length * 2);
        for (byte b : raw) out.append(String.format("%02x", b & 0xff));
        return out.toString();
    }

    /**
     * Exact client-lookup preflight for the helper client DBID, target
     * Destination hash, and Router B hash. Read-only; never mutates
     * router, NetDB, k-bucket, profile, or selector state.
     */
    public static Result preflightForClient(
            RouterContext ctx, Hash clientDbid, Hash targetHash, Hash bHash) {
        if (ctx == null || clientDbid == null || targetHash == null || bHash == null) {
            return Result.unavailable("null-argument");
        }
        try {
            // B2: query the actual client facade via clientNetDb(clientDbid).
            NetworkDatabaseFacade ndb = ctx.clientNetDb(clientDbid);
            if (ndb == null) {
                return Result.unavailable("client-db-unresolved");
            }
            boolean resolved = true;
            if (!(ndb instanceof KademliaNetworkDatabaseFacade)) {
                return Result.unavailable("client-facade-not-kademlia");
            }
            KademliaNetworkDatabaseFacade clientFacade =
                    (KademliaNetworkDatabaseFacade) ndb;
            // If clientNetDb(clientDbid) falls back to the main DB the
            // caller must record that explicitly; it is not a pass.
            boolean isClient = clientFacade.isClientDb();

            // B3: derive the real routing key in Java via the pinned
            // router's own generator. Never reimplement in Rust.
            Hash routingKey = ctx.routingKeyGenerator().getRoutingKey(targetHash);
            if (routingKey == null) {
                return Result.unavailable("routing-key-null");
            }
            String targetHex = hexLower(targetHash.getData());
            String routingHex = hexLower(routingKey.getData());
            boolean differs = !targetHex.equalsIgnoreCase(routingHex);

            // B4: reproduce IterativeSearchJob selector width exactly.
            // default_total = 3 iff floodfillEnabled && uptime > 30 min
            // else 5; total = ctx.getProperty("netdb.searchLimit",
            // default_total); width = total + EXTRA_PEERS.
            boolean floodfillEnabled = false;
            if (clientFacade instanceof FloodfillNetworkDatabaseFacade) {
                floodfillEnabled =
                        ((FloodfillNetworkDatabaseFacade) clientFacade).floodfillEnabled();
            } else if (ctx.netDb() instanceof FloodfillNetworkDatabaseFacade) {
                floodfillEnabled =
                        ((FloodfillNetworkDatabaseFacade) ctx.netDb()).floodfillEnabled();
            }
            long uptimeMs = ctx.router().getUptime();
            int defaultTotal =
                    (floodfillEnabled && uptimeMs > FLOODFILL_UPTIME_THRESHOLD_MS) ? 3 : 5;
            int effective = ctx.getProperty(PROP_SEARCH_LIMIT, defaultTotal);
            // Clamp to a sane bounded range so a hostile property value
            // cannot drive an unbounded selector width.
            if (effective < 1) effective = 1;
            if (effective > 7) effective = 7;
            int selectorWidth = effective + EXTRA_PEERS;

            // Target LeaseSet presence through the client facade only;
            // never infer from main-NetDB state.
            boolean lsPresent = false;
            String lsType = null;
            try {
                LeaseSet ls = clientFacade.lookupLeaseSetLocally(targetHash);
                if (ls != null) {
                    lsPresent = true;
                    lsType = ls.getClass().getSimpleName();
                }
            } catch (RuntimeException re) {
                lsPresent = false;
                lsType = null;
            }

            // B5: production-equivalent selector overload. The pinned
            // `IterativeSearchJob.runJob()` calls the 3-argument
            // `selectFloodfillParticipants(routingKey, selectorWidth,
            // kbs)` overload, which internally supplies the router
            // self-ignore singleton. Calling the same overload here is
            // production-equivalent by construction.
            if (!(clientFacade.getPeerSelector() instanceof FloodfillPeerSelector)) {
                return Result.unavailable("selector-not-floodfill");
            }
            FloodfillPeerSelector floodfill =
                    (FloodfillPeerSelector) clientFacade.getPeerSelector();
            KBucketSet<Hash> kbs = clientFacade.getKBuckets();
            if (kbs == null) {
                return Result.unavailable("kbuckets-uninitialized");
            }
            int kbucketSize = kbs.size();
            List<Hash> selected =
                    floodfill.selectFloodfillParticipants(routingKey, selectorWidth, kbs);
            if (selected == null) {
                return Result.unavailable("selector-returned-null");
            }
            // Bound the retained set; the caller echoes at most 8.
            List<Hash> bounded = new ArrayList<Hash>(
                    selected.size() > 8 ? selected.subList(0, 8) : selected);
            boolean containsB = selected.contains(bHash);
            return new Result(resolved, isClient, targetHex, routingHex, differs,
                    floodfillEnabled, uptimeMs, effective, EXTRA_PEERS, selectorWidth,
                    kbucketSize, bounded, containsB, lsPresent, lsType, null);
        } catch (RuntimeException e) {
            return Result.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
