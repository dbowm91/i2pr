// Plan 240 WP — test-only read-only Router-B lookup-candidate readiness
// probe for the streaming response epoch.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P239Probe.java`.
// Never compiled into or against the exact-pinned source checkout.
// It MUST NOT patch or replace any Java I2P class, MUST NOT mutate
// NetDB/profile/tunnel/banlist/queue/stat state, MUST NOT use reflection
// or private-field access, MUST NOT store RouterInfos, MUST NOT install
// or build tunnels, MUST NOT force connections, MUST NOT promote tiers,
// MUST NOT create profiles (never `addProfile`, never
// `getOrCreateProfile*`, never `heardAbout`), MUST NOT read private
// queues, MUST NOT create rates.
//
// Read-only public-API surface:
//   - Router-B RI: `KademliaNetworkDatabaseFacade.lookupRouterInfoLocally`
//     (validated presence + capabilities + bandwidth tier + publish time);
//   - floodfill index: `PeerManagerFacade.getPeersByCapability('f')`
//     membership (the exact pinned `FloodfillPeerSelector` capability
//     source; read-only set membership only);
//   - banlist: `Banlist.isBanlistedForever` (read-only);
//   - profile: `ProfileOrganizer.getProfileNonblocking` (read-only, never
//     creates) + `PeerProfile.getLastSendFailed` recency bucket;
//   - comm: `CommSystemFacade.isEstablished` (read-only).
//
// All facts are bounded (tier truncated to 8 chars, age bucketed,
// booleans only). No peer paths, keys, tags, payloads, queue contents,
// destinations, hashes, or raw log text are exposed.
package net.i2p.router.networkdb.kademlia;

import java.util.Set;
import net.i2p.data.Hash;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.RouterContext;
import net.i2p.router.peermanager.PeerProfile;

public final class P240Probe {

    private P240Probe() {
    }

    /** Recency horizon for `b_last_send_failed_recent` (30 minutes). */
    public static final long SEND_FAILED_RECENT_MS = 30L * 60L * 1000L;

    /** Freshness horizon for the `fresh` age bucket (1 hour). */
    public static final long RI_FRESH_MS = 60L * 60L * 1000L;

    /** Staleness horizon for the `old` age bucket (3 hours, pinned selector). */
    public static final long RI_OLD_MS = 3L * 60L * 60L * 1000L;

    public static final class RouterB {
        public final boolean riPresent;
        public final boolean floodfillCapabilityInRi;
        public final boolean peerManagerFCapabilityIndexed;
        public final boolean banlistedForever;
        public final String riAgeBucket;
        public final String bandwidthTier;
        public final boolean profilePresent;
        public final boolean lastSendFailedRecent;
        public final boolean commEstablished;
        public final String error;

        private RouterB(
                boolean riPresent,
                boolean floodfillCapabilityInRi,
                boolean peerManagerFCapabilityIndexed,
                boolean banlistedForever,
                String riAgeBucket,
                String bandwidthTier,
                boolean profilePresent,
                boolean lastSendFailedRecent,
                boolean commEstablished,
                String error) {
            this.riPresent = riPresent;
            this.floodfillCapabilityInRi = floodfillCapabilityInRi;
            this.peerManagerFCapabilityIndexed = peerManagerFCapabilityIndexed;
            this.banlistedForever = banlistedForever;
            this.riAgeBucket = riAgeBucket;
            this.bandwidthTier = bandwidthTier;
            this.profilePresent = profilePresent;
            this.lastSendFailedRecent = lastSendFailedRecent;
            this.commEstablished = commEstablished;
            this.error = error;
        }

        public static RouterB unavailable(String reason) {
            return new RouterB(
                false, false, false, false,
                "unknown", "unknown", false, false, false,
                reason);
        }
    }

    private static String ageBucket(long ageMs) {
        if (ageMs < 0) {
            return "unknown";
        }
        if (ageMs < RI_FRESH_MS) {
            return "fresh";
        }
        if (ageMs < RI_OLD_MS) {
            return "recent";
        }
        return "old";
    }

    private static String boundedTier(String tier) {
        if (tier == null) {
            return "unknown";
        }
        if (tier.length() > 8) {
            return tier.substring(0, 8);
        }
        return tier;
    }

    /**
     * Read-only Router-B lookup-candidate readiness snapshot.
     * Observation only; never mutates NetDB/peer-manager/banlist/profile/
     * comm/stat state, never creates profiles or rates.
     */
    public static RouterB snapshotRouterB(RouterContext ctx, Hash bHash) {
        if (ctx == null || bHash == null) {
            return RouterB.unavailable("null-argument");
        }
        try {
            RouterInfo ri = null;
            try {
                if (!(ctx.netDb() instanceof KademliaNetworkDatabaseFacade)) {
                    return RouterB.unavailable("main-facade-not-kademlia");
                }
                ri = ((KademliaNetworkDatabaseFacade) ctx.netDb())
                    .lookupRouterInfoLocally(bHash);
            } catch (RuntimeException re) {
                ri = null;
            }
            boolean riPresent = (ri != null);
            boolean floodfillInRi = false;
            String tier = "unknown";
            String bucket = "unknown";
            if (riPresent) {
                try {
                    String caps = ri.getCapabilities();
                    floodfillInRi = caps != null
                        && caps.indexOf(
                            FloodfillNetworkDatabaseFacade.CAPABILITY_FLOODFILL) >= 0;
                } catch (RuntimeException re) {
                    floodfillInRi = false;
                }
                try {
                    tier = boundedTier(ri.getBandwidthTier());
                } catch (RuntimeException re) {
                    tier = "unknown";
                }
                try {
                    long age = ctx.clock().now() - ri.getPublished();
                    bucket = ageBucket(age);
                } catch (RuntimeException re) {
                    bucket = "unknown";
                }
            }
            boolean indexed = false;
            try {
                Set<Hash> floodfills =
                    ctx.peerManager().getPeersByCapability(
                        FloodfillNetworkDatabaseFacade.CAPABILITY_FLOODFILL);
                indexed = floodfills != null && floodfills.contains(bHash);
            } catch (RuntimeException re) {
                indexed = false;
            }
            boolean banlisted = false;
            try {
                banlisted = ctx.banlist().isBanlistedForever(bHash);
            } catch (RuntimeException re) {
                banlisted = false;
            }
            boolean profilePresent = false;
            boolean failedRecent = false;
            try {
                PeerProfile prof =
                    ctx.profileOrganizer().getProfileNonblocking(bHash);
                profilePresent = (prof != null);
                if (profilePresent) {
                    try {
                        long lastFailed = prof.getLastSendFailed();
                        long now = ctx.clock().now();
                        failedRecent = lastFailed > 0
                            && (now - lastFailed) < SEND_FAILED_RECENT_MS;
                    } catch (RuntimeException re) {
                        failedRecent = false;
                    }
                }
            } catch (RuntimeException re) {
                profilePresent = false;
                failedRecent = false;
            }
            boolean established = false;
            try {
                established = ctx.commSystem().isEstablished(bHash);
            } catch (RuntimeException re) {
                established = false;
            }
            return new RouterB(
                riPresent, floodfillInRi, indexed, banlisted,
                bucket, tier, profilePresent, failedRecent, established,
                null);
        } catch (RuntimeException e) {
            return RouterB.unavailable(
                "probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
