// Plan 220 WP D — test-only same-package FloodfillPeerSelector probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java`. Never compiled into or against the
// exact-pinned source checkout. It MUST NOT patch or replace any Java
// I2P class.
//
// Living in `net.i2p.router.networkdb.kademlia` solely so it can call
// the package-visible selector entry points read-only:
//   - `KademliaNetworkDatabaseFacade.getKBuckets()` (live k-buckets);
//   - `FloodfillPeerSelector.selectFloodfillParticipants(key, N,
//     exclude, kbuckets)` (actual selector ranking on live state).
//
// The probe answers one question: for the reverse-lookup routing key
// (the i2pr destination hash), what does the live selector return and
// is Router B in that set? The probe width N=3 is the documented
// standard floodfill lookup fanout the probe uses; it is echoed in
// every response so evidence never overclaims to be the helper
// OCMOSJ call itself. The OCMOSJ/client-NetDB stages remain
// independently Unknown unless exact-pinned log correlation proves
// them (Plan 220 WP F).
package net.i2p.router.networkdb.kademlia;

import java.util.Collections;
import java.util.List;
import net.i2p.data.Hash;
import net.i2p.kademlia.KBucketSet;
import net.i2p.router.RouterContext;

public final class P220SelectorProbe {

    /** Documented probe fanout width (standard floodfill lookup fanout). */
    public static final int PROBE_FANOUT = 3;

    private P220SelectorProbe() {
    }

    public static final class Result {
        /** Live k-bucket population observed at probe time. */
        public final int kbucketSize;
        /** Selector output for the target key (possibly empty). */
        public final List<Hash> selected;
        /** Machine-readable reason when the probe cannot observe. Null on success. */
        public final String error;

        private Result(int kbucketSize, List<Hash> selected, String error) {
            this.kbucketSize = kbucketSize;
            this.selected = selected;
            this.error = error;
        }

        public static Result ok(int kbucketSize, List<Hash> selected) {
            return new Result(kbucketSize, selected, null);
        }

        public static Result unavailable(String reason) {
            return new Result(-1, Collections.emptyList(), reason);
        }
    }

    /**
     * Runs the live {@code FloodfillPeerSelector} for {@code targetKey}
     * read-only. Never mutates router, NetDB, k-bucket, profile, or
     * selector state. Any failure (wrong facade/selector shape,
     * uninitialized k-buckets, profile read error) is returned as an
     * explicit {@code unavailable} reason so the caller records
     * Unknown instead of a synthesized zero.
     */
    public static Result selectForKey(RouterContext ctx, Hash targetKey, Hash bHash) {
        if (ctx == null || targetKey == null || bHash == null) {
            return Result.unavailable("null-argument");
        }
        try {
            if (!(ctx.netDb() instanceof KademliaNetworkDatabaseFacade)) {
                return Result.unavailable("facade-not-kademlia");
            }
            KademliaNetworkDatabaseFacade facade =
                (KademliaNetworkDatabaseFacade) ctx.netDb();
            PeerSelector selector = facade.getPeerSelector();
            if (!(selector instanceof FloodfillPeerSelector)) {
                return Result.unavailable("selector-not-floodfill");
            }
            FloodfillPeerSelector floodfill = (FloodfillPeerSelector) selector;
            KBucketSet<Hash> kbs = facade.getKBuckets();
            if (kbs == null) {
                return Result.unavailable("kbuckets-uninitialized");
            }
            int kbucketSize = kbs.size();
            List<Hash> selected = floodfill.selectFloodfillParticipants(
                targetKey, PROBE_FANOUT, Collections.emptySet(), kbs);
            if (selected == null) {
                return Result.unavailable("selector-returned-null");
            }
            return Result.ok(kbucketSize, selected);
        } catch (RuntimeException e) {
            return Result.unavailable(
                "selector-threw-" + e.getClass().getSimpleName());
        }
    }
}
