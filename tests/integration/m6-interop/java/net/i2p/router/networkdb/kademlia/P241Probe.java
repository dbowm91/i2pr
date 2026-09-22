// Plan 241 WP — test-only read-only Router-B RouterInfo visibility probe
// for the Streaming one-hop client-tunnel lane.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P240Probe.java`.
// Never compiled into or against the exact-pinned source checkout.
// It MUST NOT patch or replace any Java I2P class, MUST NOT mutate
// NetDB/profile/tunnel/banlist/queue/stat state, MUST NOT use reflection
// or private-field access, MUST NOT store RouterInfos (in either facade),
// MUST NOT install or build tunnels, MUST NOT force connections, MUST NOT
// promote tiers, MUST NOT create profiles, MUST NOT read private queues,
// MUST NOT create rates.
//
// Plan 241 §4/§8 — exact-pinned `IterativeSearchJob.sendQuery()` reads
// Router B RI from `ctx.netDb().lookupRouterInfoLocally(peer)` for
// router-global send preparation, but the zero-hop safety guard checks
// `_facade.lookupLocallyWithoutValidation(peer)`, where `_facade` is the
// helper client NetDB. Main-NetDB B-RI presence and helper-client B-RI
// absence are therefore compatible facts. This probe records both sides
// as diagnostic facts using local reads only:
//
// Read-only public-API surface:
//   - main Router-B RI: `KademliaNetworkDatabaseFacade`
//     `lookupRouterInfoLocally` (validated) +
//     `lookupLocallyWithoutValidation` (raw, narrowed to RouterInfo);
//   - helper-client Router-B RI: `ctx.clientNetDb(clientDbid)` resolved
//     exactly like `P224LsProbe.snapshotClient` (Kademlia + `isClientDb`,
//     else explicitly non-client), then the same two local reads.
//
// All facts are booleans only. No peer paths, keys, tags, payloads, queue
// contents, destinations, hashes, or raw log text are exposed. A client
// facade that falls back to the main DB is reported as
// `client-resolved=false`, never as client presence.
package net.i2p.router.networkdb.kademlia;

import net.i2p.data.DatabaseEntry;
import net.i2p.data.Hash;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.RouterContext;
import net.i2p.router.networkdb.kademlia.KademliaNetworkDatabaseFacade;
import net.i2p.router.NetworkDatabaseFacade;

public final class P241Probe {

    private P241Probe() {
    }

    public static final class Bri {
        public final boolean mainRawPresent;
        public final boolean mainValidPresent;
        public final boolean clientResolved;
        public final boolean clientRawPresent;
        public final boolean clientValidPresent;
        public final String error;

        private Bri(
                boolean mainRawPresent,
                boolean mainValidPresent,
                boolean clientResolved,
                boolean clientRawPresent,
                boolean clientValidPresent,
                String error) {
            this.mainRawPresent = mainRawPresent;
            this.mainValidPresent = mainValidPresent;
            this.clientResolved = clientResolved;
            this.clientRawPresent = clientRawPresent;
            this.clientValidPresent = clientValidPresent;
            this.error = error;
        }

        public static Bri unavailable(String reason) {
            return new Bri(false, false, false, false, false, reason);
        }
    }

    private static boolean rawRouterInfoPresent(
            KademliaNetworkDatabaseFacade facade, Hash peer) {
        try {
            DatabaseEntry raw = facade.lookupLocallyWithoutValidation(peer);
            return raw instanceof RouterInfo;
        } catch (RuntimeException re) {
            return false;
        }
    }

    private static boolean validRouterInfoPresent(
            KademliaNetworkDatabaseFacade facade, Hash peer) {
        try {
            RouterInfo ri = facade.lookupRouterInfoLocally(peer);
            return ri != null;
        } catch (RuntimeException re) {
            return false;
        }
    }

    /**
     * Read-only Router-B RI visibility snapshot across the main NetDB and
     * the helper client NetDB. Observation only; never stores, publishes,
     * or searches; the accessors below perform local reads only and
     * cannot prime either facade with a network lookup. Never copies or
     * stores B RI into the helper client DB.
     */
    public static Bri snapshotBri(RouterContext ctx, Hash bHash, Hash clientDbid) {
        if (ctx == null || bHash == null || clientDbid == null) {
            return Bri.unavailable("null-argument");
        }
        try {
            if (!(ctx.netDb() instanceof KademliaNetworkDatabaseFacade)) {
                return Bri.unavailable("main-facade-not-kademlia");
            }
            KademliaNetworkDatabaseFacade mainFacade =
                    (KademliaNetworkDatabaseFacade) ctx.netDb();
            boolean mainRaw = rawRouterInfoPresent(mainFacade, bHash);
            boolean mainValid = validRouterInfoPresent(mainFacade, bHash);
            NetworkDatabaseFacade ndb = null;
            try {
                ndb = ctx.clientNetDb(clientDbid);
            } catch (RuntimeException re) {
                ndb = null;
            }
            if (ndb == null) {
                return new Bri(mainRaw, mainValid, false, false, false,
                        "client-db-unresolved");
            }
            if (!(ndb instanceof KademliaNetworkDatabaseFacade)) {
                return new Bri(mainRaw, mainValid, false, false, false,
                        "client-facade-not-kademlia");
            }
            KademliaNetworkDatabaseFacade clientFacade =
                    (KademliaNetworkDatabaseFacade) ndb;
            if (!clientFacade.isClientDb()) {
                return new Bri(mainRaw, mainValid, false, false, false,
                        "client-fallback-to-main");
            }
            boolean clientRaw = rawRouterInfoPresent(clientFacade, bHash);
            boolean clientValid = validRouterInfoPresent(clientFacade, bHash);
            return new Bri(mainRaw, mainValid, true, clientRaw, clientValid, null);
        } catch (RuntimeException e) {
            return Bri.unavailable(
                "probe-threw-" + e.getClass().getSimpleName());
        }
    }
}
