// Plan 230 WP A — test-only read-only reachability-capability /
// profile-bootstrap observation probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P229Probe.java`.
// Never compiled into or against the exact-pinned source checkout.
// It MUST NOT patch or replace any Java I2P class, MUST NOT mutate
// NetDB/profile/tunnel/banlist state, MUST NOT use reflection or
// private-field access, MUST NOT store RouterInfos, MUST NOT install or
// build tunnels, MUST NOT force connections, MUST NOT promote tiers,
// MUST NOT create profiles (never `addProfile`, never
// `getOrCreateProfile*` from a probe path, never `heardAbout` from a
// probe path).
//
// Read-only public-API surface:
//   - Router-C-as-observed-by-A: `KademliaNetworkDatabaseFacade`
//     `lookupLocallyWithoutValidation` (raw presence) +
//     `lookupRouterInfoLocally` (validated presence + capabilities +
//     bandwidth tier + RI SHA-256),
//     `RouterContext.profileOrganizer().selectAllPeers()` membership plus
//     `getProfileNonblocking(Hash)` (read-only, never creates),
//     `isSelectable(Hash)`, `countNotFailingPeers()`,
//     `RouterContext.banlist().isBanlisted(Hash)`,
//     observer inputs for the exact pinned
//     `ProfileManagerImpl.shouldCreate(caps)` predicate:
//     `RouterContext.netDb().floodfillEnabled()` and
//     `RouterContext.bandwidthLimiter().getMaxShareBandwidth()` plus
//     `RouterContext.commSystem().getStatus()`;
//   - self view: own `RouterInfo` capabilities/bandwidth-tier/RI-SHA plus
//     own communication-system status, so a stale pre-correction copy
//     observed by A can be distinguished from a newly published RI.
//
// The historical P229 `unreachable` signal (mapped to the deprecated,
// unconditionally-false failing-signal accessor) is NOT
// consumed here: Plan 230 §4.11 forbids it as reachability authority.
// This probe does not consult that signal at all.
//
// All facts are bounded (counts clamped to 0..64, strings truncated to
// fixed ceilings) and carry no key material, payloads, peer paths, or
// raw log text. No peer hash is echoed except the caller-supplied
// Router-C echo already performed by the launcher.
package net.i2p.router.networkdb.kademlia;

import java.security.MessageDigest;
import java.util.Set;
import net.i2p.data.Hash;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.RouterContext;
import net.i2p.router.peermanager.PeerProfile;

public final class P230Probe {

    private P230Probe() {
    }

    /** Exact-pinned `ProfileManagerImpl.shouldCreate(caps)` share floor. */
    static final int SHARE_BANDWIDTH_FLOOR_BYTES = 128 * 1024;

    public static final class Capability {
        public final boolean mainRawPresent;
        public final boolean mainValidPresent;
        public final boolean selectable;
        public final boolean banlisted;
        public final boolean capsHasR;
        public final boolean capsHasU;
        public final boolean capsHasF;
        public final boolean capsHasL;
        public final boolean capsHasE;
        public final boolean capsHasG;
        public final String bandwidthTier;
        public final String cRiSha256Hex;
        public final boolean profilePresent;
        public final int profileCount;
        public final int notFailingCount;
        public final boolean localFloodfillEnabled;
        public final int localMaxShareBandwidth;
        public final String localCommStatus;
        public final boolean heardAboutCreationEligible;
        public final String error;

        private Capability(
                boolean mainRawPresent,
                boolean mainValidPresent,
                boolean selectable,
                boolean banlisted,
                boolean capsHasR,
                boolean capsHasU,
                boolean capsHasF,
                boolean capsHasL,
                boolean capsHasE,
                boolean capsHasG,
                String bandwidthTier,
                String cRiSha256Hex,
                boolean profilePresent,
                int profileCount,
                int notFailingCount,
                boolean localFloodfillEnabled,
                int localMaxShareBandwidth,
                String localCommStatus,
                boolean heardAboutCreationEligible,
                String error) {
            this.mainRawPresent = mainRawPresent;
            this.mainValidPresent = mainValidPresent;
            this.selectable = selectable;
            this.banlisted = banlisted;
            this.capsHasR = capsHasR;
            this.capsHasU = capsHasU;
            this.capsHasF = capsHasF;
            this.capsHasL = capsHasL;
            this.capsHasE = capsHasE;
            this.capsHasG = capsHasG;
            this.bandwidthTier = bandwidthTier;
            this.cRiSha256Hex = cRiSha256Hex;
            this.profilePresent = profilePresent;
            this.profileCount = profileCount;
            this.notFailingCount = notFailingCount;
            this.localFloodfillEnabled = localFloodfillEnabled;
            this.localMaxShareBandwidth = localMaxShareBandwidth;
            this.localCommStatus = localCommStatus;
            this.heardAboutCreationEligible = heardAboutCreationEligible;
            this.error = error;
        }

        public static Capability unavailable(String reason) {
            return new Capability(false, false, false, false,
                false, false, true, false, false, false,
                "unknown", "unknown", false, 0, 0,
                false, 0, "unknown", false, reason);
        }

        public static Capability ok(
                boolean mainRawPresent,
                boolean mainValidPresent,
                boolean selectable,
                boolean banlisted,
                boolean capsHasR,
                boolean capsHasU,
                boolean capsHasF,
                boolean capsHasL,
                boolean capsHasE,
                boolean capsHasG,
                String bandwidthTier,
                String cRiSha256Hex,
                boolean profilePresent,
                int profileCount,
                int notFailingCount,
                boolean localFloodfillEnabled,
                int localMaxShareBandwidth,
                String localCommStatus,
                boolean heardAboutCreationEligible) {
            return new Capability(mainRawPresent, mainValidPresent,
                selectable, banlisted,
                capsHasR, capsHasU, capsHasF, capsHasL, capsHasE, capsHasG,
                bandwidthTier, cRiSha256Hex,
                profilePresent, profileCount, notFailingCount,
                localFloodfillEnabled, localMaxShareBandwidth, localCommStatus,
                heardAboutCreationEligible, null);
        }
    }

    public static final class SelfView {
        public final String selfCommStatus;
        public final boolean selfCapsHasR;
        public final boolean selfCapsHasU;
        public final boolean selfCapsHasF;
        public final boolean selfCapsHasL;
        public final boolean selfCapsHasE;
        public final boolean selfCapsHasG;
        public final String selfBandwidthTier;
        public final String selfRiSha256Hex;
        public final String error;

        private SelfView(
                String selfCommStatus,
                boolean selfCapsHasR,
                boolean selfCapsHasU,
                boolean selfCapsHasF,
                boolean selfCapsHasL,
                boolean selfCapsHasE,
                boolean selfCapsHasG,
                String selfBandwidthTier,
                String selfRiSha256Hex,
                String error) {
            this.selfCommStatus = selfCommStatus;
            this.selfCapsHasR = selfCapsHasR;
            this.selfCapsHasU = selfCapsHasU;
            this.selfCapsHasF = selfCapsHasF;
            this.selfCapsHasL = selfCapsHasL;
            this.selfCapsHasE = selfCapsHasE;
            this.selfCapsHasG = selfCapsHasG;
            this.selfBandwidthTier = selfBandwidthTier;
            this.selfRiSha256Hex = selfRiSha256Hex;
            this.error = error;
        }

        public static SelfView unavailable(String reason) {
            return new SelfView("unknown",
                false, false, true, false, false, false,
                "unknown", "unknown", reason);
        }

        public static SelfView ok(
                String selfCommStatus,
                boolean selfCapsHasR,
                boolean selfCapsHasU,
                boolean selfCapsHasF,
                boolean selfCapsHasL,
                boolean selfCapsHasE,
                boolean selfCapsHasG,
                String selfBandwidthTier,
                String selfRiSha256Hex) {
            return new SelfView(selfCommStatus,
                selfCapsHasR, selfCapsHasU, selfCapsHasF,
                selfCapsHasL, selfCapsHasE, selfCapsHasG,
                selfBandwidthTier, selfRiSha256Hex, null);
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

    private static String boundToken(String value, int maxLen, String fallback) {
        if (value == null || value.isEmpty()) {
            return fallback;
        }
        String trimmed = value.trim();
        if (trimmed.isEmpty()) {
            return fallback;
        }
        if (trimmed.length() > maxLen) {
            return trimmed.substring(0, maxLen);
        }
        return trimmed;
    }

    private static boolean capsHas(String caps, char flag) {
        return caps != null && caps.indexOf(flag) >= 0;
    }

    private static String sha256Hex(byte[] raw) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            byte[] out = digest.digest(raw);
            StringBuilder hex = new StringBuilder(out.length * 2);
            for (byte b : out) {
                hex.append(String.format("%02x", b & 0xff));
            }
            return hex.toString();
        } catch (Exception e) {
            return "unknown";
        }
    }

    /**
     * Exact-pinned `ProfileManagerImpl.shouldCreate(caps)` predicate,
     * evaluated read-only from already-observed facts. `caps` is the
     * observed Router-C RouterInfo capability string;
     * `localFloodfillEnabled` is this observer's
     * `netDb().floodfillEnabled()`; `localMaxShareBandwidth` is this
     * observer's `bandwidthLimiter().getMaxShareBandwidth()` in bytes.
     * Returns false for a null capability string (fail-closed).
     */
    public static boolean heardAboutCreationEligible(
            String caps,
            boolean localFloodfillEnabled,
            int localMaxShareBandwidth) {
        if (caps == null) {
            return false;
        }
        if (caps.indexOf('R') < 0) {
            return false;
        }
        if (caps.indexOf('f') >= 0) {
            return true;
        }
        boolean lowBandwidthExempt = caps.indexOf('L') < 0
            || (!localFloodfillEnabled
                && localMaxShareBandwidth < SHARE_BANDWIDTH_FLOOR_BYTES);
        if (!lowBandwidthExempt) {
            return false;
        }
        if (caps.indexOf('E') >= 0) {
            return false;
        }
        if (caps.indexOf('G') >= 0) {
            return false;
        }
        return true;
    }

    /**
     * Read-only Router-C capability snapshot on this router's main NetDB,
     * plus the local observer inputs needed to interpret the exact pinned
     * creation predicate. Uses only public read-only
     * accessors; never creates profiles (only `selectAllPeers` membership
     * plus `getProfileNonblocking`), never promotes tiers, never forces
     * connections, never stores RouterInfos, never consults the
     * deprecated failing-signal accessor (Plan 230 §4.11: the historical
     * P229 `unreachable` signal is non-authoritative).
     * Fail-closed: unknown state reports the gate-failing value so the
     * harness stops instead of silently manufacturing the prerequisite.
     */
    public static Capability snapshotCapability(
            RouterContext ctx,
            KademliaNetworkDatabaseFacade mainFacade,
            Hash routerC) {
        if (ctx == null || mainFacade == null || routerC == null) {
            return Capability.unavailable("null-argument");
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
            String caps = null;
            String tier = "unknown";
            String riHex = "unknown";
            if (info != null) {
                try {
                    caps = info.getCapabilities();
                } catch (RuntimeException re) {
                    caps = null;
                }
                try {
                    tier = boundToken(info.getBandwidthTier(), 8, "unknown");
                } catch (RuntimeException re) {
                    tier = "unknown";
                }
                try {
                    riHex = sha256Hex(info.toByteArray());
                } catch (RuntimeException re) {
                    riHex = "unknown";
                }
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
                return Capability.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            boolean selectable = false;
            try {
                selectable = ctx.profileOrganizer().isSelectable(routerC);
            } catch (RuntimeException re) {
                return Capability.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            boolean banlisted = false;
            try {
                banlisted = ctx.banlist().isBanlisted(routerC);
            } catch (RuntimeException re) {
                banlisted = false;
            }
            int notFailing = 0;
            try {
                notFailing = clampCount(ctx.profileOrganizer().countNotFailingPeers());
            } catch (RuntimeException re) {
                notFailing = 0;
            }
            boolean localFloodfill = false;
            try {
                localFloodfill = ctx.netDb().floodfillEnabled();
            } catch (RuntimeException re) {
                return Capability.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            int localShare = 0;
            try {
                localShare = ctx.bandwidthLimiter().getMaxShareBandwidth();
                if (localShare < 0) {
                    localShare = 0;
                }
            } catch (RuntimeException re) {
                return Capability.unavailable(
                    "probe-threw-" + re.getClass().getSimpleName());
            }
            String commStatus = "unknown";
            try {
                Object status = ctx.commSystem().getStatus();
                if (status != null) {
                    commStatus = boundToken(status.toString(), 32, "unknown");
                }
            } catch (RuntimeException re) {
                commStatus = "unknown";
            }
            boolean eligible = heardAboutCreationEligible(caps, localFloodfill, localShare);
            return Capability.ok(rawPresent, validPresent, selectable, banlisted,
                capsHas(caps, 'R'), capsHas(caps, 'U'), capsHas(caps, 'f'),
                capsHas(caps, 'L'), capsHas(caps, 'E'), capsHas(caps, 'G'),
                tier, riHex, profilePresent, profileCount, notFailing,
                localFloodfill, localShare, commStatus, eligible);
        } catch (RuntimeException e) {
            return Capability.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    /**
     * Read-only self view: this router's own communication-system status
     * plus the capabilities of its current self RouterInfo, so a stale
     * pre-correction copy observed by Router A can be distinguished from
     * a newly published RouterInfo. Observation only; never mutates
     * publisher, transport, or NetDB state.
     */
    public static SelfView snapshotSelf(
            RouterContext ctx,
            KademliaNetworkDatabaseFacade mainFacade) {
        if (ctx == null || mainFacade == null) {
            return SelfView.unavailable("null-argument");
        }
        try {
            String commStatus = "unknown";
            try {
                Object status = ctx.commSystem().getStatus();
                if (status != null) {
                    commStatus = boundToken(status.toString(), 32, "unknown");
                }
            } catch (RuntimeException re) {
                commStatus = "unknown";
            }
            RouterInfo self = null;
            try {
                self = mainFacade.lookupRouterInfoLocally(ctx.routerHash());
            } catch (RuntimeException re) {
                self = null;
            }
            if (self == null) {
                return SelfView.unavailable("self-ri-absent");
            }
            String caps = null;
            String tier = "unknown";
            String riHex = "unknown";
            try {
                caps = self.getCapabilities();
            } catch (RuntimeException re) {
                caps = null;
            }
            try {
                tier = boundToken(self.getBandwidthTier(), 8, "unknown");
            } catch (RuntimeException re) {
                tier = "unknown";
            }
            try {
                riHex = sha256Hex(self.toByteArray());
            } catch (RuntimeException re) {
                riHex = "unknown";
            }
            return SelfView.ok(commStatus,
                capsHas(caps, 'R'), capsHas(caps, 'U'), capsHas(caps, 'f'),
                capsHas(caps, 'L'), capsHas(caps, 'E'), capsHas(caps, 'G'),
                tier, riHex);
        } catch (RuntimeException e) {
            return SelfView.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
