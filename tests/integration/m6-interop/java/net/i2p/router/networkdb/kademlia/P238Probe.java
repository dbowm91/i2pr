// Plan 238 WP B/C — test-only read-only Router-A I2CP-admission probe
// for the streaming response epoch.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P231Probe.java`.
// Never compiled into or against the exact-pinned source checkout.
// It MUST NOT patch or replace any Java I2P class, MUST NOT mutate
// NetDB/profile/tunnel/banlist/queue/stat state, MUST NOT use reflection
// or private-field access, MUST NOT store RouterInfos, MUST NOT install
// or build tunnels, MUST NOT force connections, MUST NOT promote tiers,
// MUST NOT create profiles, MUST NOT call `heardAbout`, MUST NOT read
// private queues, MUST NOT create rates.
//
// Read-only public-API surface:
//   - admission stats: `RouterContext.statManager().getRate(name)` +
//     `RateStat.getLifetimeEventCount()` for the four exact-pinned names
//     `client.distributeTime` (first Router-A I2CP admission stage,
//     added in `ClientMessageEventListener.handleSendMessage` after
//     `distributeMessage` returns),
//     `client.dispatchTime` / `client.dispatchSendTime` (OCMOSJ dispatch
//     path, added after `tunnelDispatcher().dispatchOutbound(...)`
//     returns), and `tunnel.dispatchOutboundTunnel` (tunnel-handoff
//     context, evidence only in this plan).
//     A missing rate reports -1 (unknown), never zero-as-fact.
//
// All facts are bounded (four signed counts) and carry only counts.
// No peer paths, keys, tags, payloads, queue contents, destinations,
// hashes, or raw log text are exposed.
package net.i2p.router.networkdb.kademlia;

import net.i2p.router.RouterContext;
import net.i2p.stat.RateStat;

public final class P238Probe {

    private P238Probe() {
    }

    /** Unknown lifetime count: the named rate was never created. */
    public static final long COUNT_UNKNOWN = -1;

    public static final class Admission {
        public final long distributeTime;
        public final long dispatchTime;
        public final long dispatchSendTime;
        public final long dispatchOutboundTunnel;
        public final String error;

        private Admission(
                long distributeTime,
                long dispatchTime,
                long dispatchSendTime,
                long dispatchOutboundTunnel,
                String error) {
            this.distributeTime = distributeTime;
            this.dispatchTime = dispatchTime;
            this.dispatchSendTime = dispatchSendTime;
            this.dispatchOutboundTunnel = dispatchOutboundTunnel;
            this.error = error;
        }

        public static Admission unavailable(String reason) {
            return new Admission(
                COUNT_UNKNOWN, COUNT_UNKNOWN, COUNT_UNKNOWN,
                COUNT_UNKNOWN, reason);
        }

        public static Admission ok(
                long distributeTime,
                long dispatchTime,
                long dispatchSendTime,
                long dispatchOutboundTunnel) {
            return new Admission(distributeTime, dispatchTime,
                dispatchSendTime, dispatchOutboundTunnel, null);
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

    /**
     * Read-only lifetime-event-count snapshot for the four Plan-238
     * Router-A admission stat names (the §6 admission/distpatch names
     * plus the tunnel-handoff count for context).
     * Observation only; never creates rates, never mutates counters.
     * A never-created rate reports -1 (unknown).
     */
    public static Admission snapshotAdmission(RouterContext ctx) {
        if (ctx == null) {
            return Admission.unavailable("null-argument");
        }
        try {
            long distributeTime = lifetimeCount(ctx, "client.distributeTime");
            long dispatchTime = lifetimeCount(ctx, "client.dispatchTime");
            long dispatchSendTime = lifetimeCount(ctx, "client.dispatchSendTime");
            long dispatchOutboundTunnel =
                lifetimeCount(ctx, "tunnel.dispatchOutboundTunnel");
            return Admission.ok(distributeTime, dispatchTime,
                dispatchSendTime, dispatchOutboundTunnel);
        } catch (RuntimeException e) {
            return Admission.unavailable(
                "probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
