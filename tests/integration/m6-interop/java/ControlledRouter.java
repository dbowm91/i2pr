// Plan 196 — M6 Java I2P controlled first-run topology corrective;
// Plan 219 — M6 Java reverse-delivery root-cause investigation
// read-only diagnostic surface (retained history; superseded attribution).
// Plan 220 — M6 Java Plan 219 diagnostic-attribution corrective:
// protocol-correct RouterHash identity (lowercase hex, never raw
// RouterInfo byte slices, never standard Base64) plus a read-only
// same-package FloodfillPeerSelector probe. The J219-* commands below
// are frozen history; the P220-* commands are the authoritative path
// the destination driver consumes at its post-bootstrap epoch.
//
// Test-only launcher. Compiled against the exact-pinned Java I2P 2.13.0
// staged `lib/` jars into the ephemeral scratch build directory. Never
// compiled into or against the exact-pinned source checkout.
//
// The launcher invokes the stock public `net.i2p.router.Router(Properties)`
// + `setKillVMOnEnd(false)` + `runRouter()` lifecycle, exactly the same
// pre-start property-injection pattern the exact-pinned upstream
// `net.i2p.router.MultiRouter` utility uses for isolated router instances.
// It does NOT call private/internal Java methods to seed RouterInfo,
// install tunnels, populate NetDB, create destinations, or manipulate
// Streaming state.
//
// Args:
//
//   <java-data-dir>     -- disposable per-run I2P config + log + pid dir
//   <ssu2-host>         -- UDP transport bind host (loopback only)
//   <ssu2-port>         -- UDP transport bind port (loopback only)
//   <sam-port>          -- SAM bridge TCP bind port (loopback only)
//   <i2cp-port>         -- I2CP server TCP bind port (loopback only)
//   [j219-control-port] -- Plan 219 read-only diagnostic TCP port
//                          (loopback only; "0" or absent disables)
//
// Environment contract:
//   - VMCommSystem is never enabled (no `i2p.vmCommSystem=true`);
//   - public reseed URLs are never configured;
//   - the SAM bridge is the only client app started on load;
//   - all listeners bind to loopback only.
//
// Plan 219 diagnostic contract:
//   - the j219-control-port is bound to 127.0.0.1 only;
//   - the listener accepts only read-only commands
//     (`J219-SNAPSHOT`, `J219-CAPABILITIES <b64-hash>`,
//     `J219-PEERS-FLOODFILL`, `J219-MAIN-ROUTER-COUNT`,
//     `J219-STORED-RI <b64-hash>`, `J219-CLIENT-DB-LOOKUP-PEER-COUNT`,
//     `QUIT`);
//   - every response is a pre-formed, sanitized, single-line
//     `J219-EV <key>=<value> [...]` row, never a Java object
//     repr, never payload bytes, never signing keys;
//   - the listener never calls a mutator on `Router`,
//     `RouterContext`, `NetworkDatabaseFacade`, `PeerManager`,
//     or `FloodfillPeerSelector`. Read-only accessors only.
//
// Plan 220 diagnostic contract (authoritative):
//   - the same port additionally accepts the `P220-*` read-only
//     commands (`P220-SNAPSHOT`, `P220-STORED-RI <hex-hash>`,
//     `P220-CAPABILITIES <hex-hash>`, `P220-PEERS-FLOODFILL`,
//     `P220-MAIN-ROUTER-COUNT`, `P220-SELECTOR <target-hex> <b-hex>`,
//     `QUIT`);
// Plan 222 diagnostic contract (authoritative for client-NetDB):
//   - the same port additionally accepts
//     `P222-CLIENT-LOOKUP-PREFLIGHT <client-dbid-hex> <target-hex> <b-hex>`
//     which resolves `clientNetDb(clientDbid)`, derives the routing key
//     via `routingKeyGenerator().getRoutingKey`, reproduces the
//     `netdb.searchLimit` + EXTRA_PEERS width, and runs the
//     production-equivalent 3-argument selector overload read-only.
// Plan 223 diagnostic contract (read-only, no state mutation):
//   - `P223-DEST-INSPECT <dest-b64>` parses one Destination via the public
//     `new Destination(String)` API and returns hash/enc-type/pubkey-len;
//   - `P223-BRANCH <client-dbid-hex> <target-hex> <source-hex>` returns the
//     bounded C1/C2/C3 discriminator (source LeaseSetKeys, target LS2 as
//     stored, exact `getEncryptionKey(supported)` intersection) read-only.
// Plan 224 diagnostic contract (attribution only, read-only, no state mutation):
//   - `P224-MAIN-LS <target-hex>` returns the read-only main-NetDB LS
//     snapshot for one target hash (`P224-EV kind=main-ls ...` with raw
//     vs validated presence, entry type, received-as-published/reply,
//     received-by, LS2 unpublished flag, lease/key counts and key type
//     codes only, latest lease time, current flag);
//   - `P224-CLIENT-LS <client-dbid-hex> <target-hex>` returns the same
//     snapshot through `clientNetDb(clientDbid)` (`P224-EV
//     kind=client-ls ...` plus client_db_resolved/is_client); a
//     main-DB fallback is explicitly non-client and never satisfies
//     client authority;
//   - `P224-HASH-B64 <hash-hex>` renders one 32-byte hash exactly as the
//     pinned JVM logs it (`Hash.toBase64()`) so the whitelist-only
//     sanitizer can correlate exact-target log lines without
//     reimplementing the I2P Base64 alphabet;
//   - every snapshot uses local read-only accessors only
//     (`lookupLocallyWithoutValidation`, `lookupLeaseSetLocally`,
//     `getReceivedAsPublished`, `isCurrent`, `getEncryptionKeys`);
//     no `store`, no `registerKeys`, no reflection, no remote lookup
//     that could prime a client sub-DB.
//   - RouterHash values travel as 32-byte lowercase hex. Standard
//     (RFC 4648) Base64 MUST NOT be used to construct a query
//     hash; Java `Hash.toBase64()` (I2P Base64) is echoed only for
//     human correlation alongside the hex form;
//   - every P220 response is a single-line
//     `P220-EV <key>=<value> [...]` row;
//   - `P220-SELECTOR` delegates to the test-only same-package
//     `P220SelectorProbe`, which calls the package-visible
//     `FloodfillPeerSelector.selectFloodfillParticipants` on the
//     live k-buckets read-only and returns sanitized counts plus
//     target-membership booleans. It MUST NOT patch or replace any
//     Java I2P class.
// Plan 225 diagnostic contract (observability only, read-only, no state
// mutation):
//   - `P225-LOGGER-CONFIG` reads the running LogManager's effective default
//     and exact lookup-class levels. It does not reload, set, or mutate the
//     logger configuration;
//   - `P225-HASH-B32` renders one exact Hash in the Base32 form used by the
//     pinned InboundMessageDistributor log, so client-tunnel DSM correlation
//     does not confuse a client DBID's Base64 form with its b32.i2p label;
//   - every P225 response is one bounded, sanitized line and contains no
//     log text, keys, tags, payloads, or private material.
// Plan 228 diagnostic contract (attribution only, read-only, no state
// mutation):
//   - `P228-TUNNEL-INFRA` returns the bounded router tunnel-infrastructure
//     snapshot (`P228-EV kind=tunnel-infra ...` with free/inbound/outbound
//     counts plus exploratory nonzero-hop counts; no peer paths);
//   - `P228-CLIENT-POOLS <client-dbid-hex>` returns bounded client-pool
//     presence/counts for one helper client DBID;
//   - `P228-LOGGER-CONFIG` reads the running LogManager's effective levels
//     for the exact build-path scopes (TunnelPool, TunnelPeerSelector,
//     ClientPeerSelector, BuildExecutor, BuildRequestor, BuildHandler,
//     BuildMessageProcessor, BuildReplyHandler); it never reloads or
//     mutates the logger configuration;
//   - every P228 response is one bounded `P228-EV ...` line with booleans,
//     bounded counts, and response/status codes only, never peer paths,
//     keys, tags, payloads, or raw log text.
// Plan 229 diagnostic contract (reference-topology corrective only,
// read-only, no state mutation):
//   - `P229-TRANSIT-PEER <router-c-hex>` returns the bounded ordinary
//     profile/selectability snapshot for Router C on this router's main
//     NetDB (`P229-EV kind=transit-peer ...` with raw/validated presence,
//     profile presence via `selectAllPeers` + `getProfileNonblocking`
//     (never `addProfile` / `getOrCreateProfile*`), selectability,
//     banlist, `wasUnreachable`, caps `f` membership, profile and
//     not-failing counts);
//   - `P229-EXPLORATORY-SETTINGS` returns the effective exploratory pool
//     settings (`P229-EV kind=exploratory-settings ...` with lengths,
//     variances, quantities; observation only, never `set*Settings`);
//   - `P229-EXPLORATORY-TUNNELS <router-c-hex>` returns the bounded
//     exploratory install snapshot (`P229-EV kind=exploratory-tunnels ...`
//     with per-direction counts, nonzero counts, C presence, zero-hop
//     fallback presence; no peer paths);
//   - every P229 response is one bounded `P229-EV ...` line with booleans,
//     bounded counts, and hex hashes only.
// Plan 229 role contract (startup configuration only, never private state):
//   - optional 7th launcher argument selects the controlled-topology role
//     (`service` / `publication` / `transit`; unknown values fail closed);
//   - `service` and `publication` keep `router.floodfillParticipant=true`,
//     `transit` sets `router.floodfillParticipant=false`;
//   - `service` additionally applies Java's stock small-router exploratory
//     profile (`router.inboundPool.length=1`,
//     `router.inboundPool.lengthVariance=1`,
//     `router.outboundPool.length=1`,
//     `router.outboundPool.lengthVariance=1`); no quantity, backup,
//     allowZeroHop, explicitPeers, timeout, or paired-tunnel property is
//     ever set here.
// Plan 230 diagnostic contract (reachability-capability/profile-bootstrap
// corrective only, read-only, no state mutation):
//   - `P230-CAPABILITY <router-c-hex>` returns the bounded Router-C
//     capability snapshot on this router's main NetDB (`P230-EV
//     kind=capability ...` with raw/validated presence, selectability,
//     banlist, `caps_has_r/u/f/l/e/g`, bandwidth tier, observed-RI
//     SHA-256, profile presence via `selectAllPeers` +
//     `getProfileNonblocking` (never `addProfile` /
//     `getOrCreateProfile*` / `heardAbout`), profile and not-failing
//     counts, the local `shouldCreate` inputs `local_floodfill_enabled`
//     / `local_max_share_bandwidth` / `local_comm_status`, and the
//     derived `heard_about_creation_eligible` fact). The deprecated
//     `ProfileOrganizer.isFailing(Hash)` is never called: the historical
//     P229 `unreachable` signal is non-authoritative;
//   - `P230-SELF-VIEW` returns this router's own communication-system
//     status plus its current self RouterInfo capabilities
//     (`P230-EV kind=self-view ...`), so a stale pre-correction copy
//     can be distinguished from a newly published RI;
//   - every P230 response is one bounded `P230-EV ...` line with
//     booleans, bounded counts, tier/status tokens, and hex hashes only.
// Plan 227 diagnostic contract (reference-harness corrective only,
// read-only, no state mutation):
//   - `P227-PEER-ELIGIBILITY <router-c-hex>` returns bounded main-NetDB
//     presence/validity plus `ProfileOrganizer.isSelectable`, comm-system
//     established (diagnostic only), and banlist facts for Router C;
//     no profile creation, tier promotion, connection forcing, or NetDB
//     store;
//   - `P227-CLIENT-TUNNELS <client-dbid-hex> <router-c-hex>` resolves the
//     live inbound/outbound client pools through public tunnel-manager
//     accessors and inspects installed tunnels read-only; one remote hop
//     via C is the exact local+Router-C path (`getLength() == 2`), never
//     a hard-coded length name alone; zero-hop is `getLength() <= 1`;
//   - every P227 response is one bounded `P227-EV ...` line with booleans,
//     counts, and hex hashes only.

import java.io.BufferedReader;
import java.io.File;
import java.io.FileWriter;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Properties;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;

import net.i2p.data.Base64;
import net.i2p.data.DataHelper;
import net.i2p.data.Hash;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.PeerManagerFacade;
import net.i2p.router.Router;
import net.i2p.router.RouterContext;
import net.i2p.router.networkdb.kademlia.KademliaNetworkDatabaseFacade;
import net.i2p.router.networkdb.kademlia.P220SelectorProbe;
import net.i2p.router.networkdb.kademlia.P222SelectorProbe;
import net.i2p.router.networkdb.kademlia.P223BranchProbe;
import net.i2p.router.networkdb.kademlia.P224LsProbe;
import net.i2p.router.networkdb.kademlia.P227Probe;
import net.i2p.router.networkdb.kademlia.P228Probe;
import net.i2p.router.networkdb.kademlia.P229Probe;
import net.i2p.router.networkdb.kademlia.P230Probe;
import net.i2p.util.Log;

public final class ControlledRouter {

    private static final String[] FORBIDDEN_VMCOMM_KEYS = new String[] {
        "i2p.vmCommSystem",
    };

    public static void main(String[] args) throws Exception {
        if (args.length != 5 && args.length != 6 && args.length != 7) {
            System.err.println(
                "usage: ControlledRouter <java-data-dir> <ssu2-host> <ssu2-port> <sam-port> <i2cp-port> [j219-control-port] [role]");
            System.exit(64);
        }

        final String javaData = args[0];
        final String ssu2Host = args[1];
        final String ssu2Port = args[2];
        final String samPort = args[3];
        final String i2cpPort = args[4];
        // Plan 219 — optional diagnostic port; "0" or absent
        // disables the read-only J219 control server.
        final String j219ControlPort = args.length >= 6 ? args[5] : "0";
        // Plan 229 — optional controlled-topology role; absent preserves
        // the legacy invocation (floodfill enabled, stock exploratory
        // settings). Unknown values fail closed.
        final String roleArg = args.length >= 7 ? args[6] : "";
        final String role;
        if (roleArg == null || roleArg.isEmpty()) {
            role = "legacy";
        } else if ("service".equals(roleArg)
                || "publication".equals(roleArg)
                || "transit".equals(roleArg)) {
            role = roleArg;
        } else {
            System.err.println(
                "ControlledRouter: unknown role '" + roleArg
                    + "' (expected service|publication|transit)");
            System.exit(64);
            return;
        }

        for (String forbidden : FORBIDDEN_VMCOMM_KEYS) {
            String v = System.getProperty(forbidden);
            if (v != null && (v.equalsIgnoreCase("true") || v.equals("1"))) {
                System.err.println(
                    "ControlledRouter: forbidden JVM property " + forbidden
                        + "=" + v + " (Plan 196 §4 forbids VMCommSystem)");
                System.exit(70);
            }
        }

        File dataDir = new File(javaData);
        if (!dataDir.isAbsolute()) {
            System.err.println(
                "ControlledRouter: java-data-dir must be absolute: " + javaData);
            System.exit(64);
        }
        File logsDir = new File(dataDir, "logs");
        File pidFile = new File(dataDir, "router.pid");
        File routerConfig = new File(dataDir, "router.config");
        File clientsConfig = new File(dataDir, "clients.config");
        File appDir = new File(dataDir, "app");
        File routerDir = new File(dataDir, "router");
        File peerProfilesDir = new File(dataDir, "peerProfiles");
        File netDbDir = new File(dataDir, "netDb");
        File keyBackupDir = new File(dataDir, "keyBackup");
        File reseedBlock = new File(dataDir, "noreseed.i2p");

        for (File d : new File[] { dataDir, logsDir, appDir, routerDir,
                                   peerProfilesDir, netDbDir, keyBackupDir }) {
            if (!d.exists() && !d.mkdirs()) {
                System.err.println(
                    "ControlledRouter: cannot create dir " + d);
                System.exit(70);
            }
        }

        // Write the deterministic disposable `clients.config` (one SAM
        // bridge only). Plan 196 §5.3 — never mutate the exact-pinned
        // `${JAVA_CACHE}/clients.config`.
        writeClientsConfig(clientsConfig, samPort, i2cpPort);

        // Plan 196 §5.5 — mirror the four-file fallback chain in
        // ReseedChecker.java:107-109 so the loader sees at least one
        // no-reseed flag regardless of any future property rename.
        try {
            if (!reseedBlock.createNewFile()) {
                // already exists, fine
            }
        } catch (IOException e) {
            System.err.println(
                "ControlledRouter: cannot write noreseed.i2p: " + e.getMessage());
        }

        Properties props = new Properties();

        // Required directory properties the exact-pinned Router reads
        // before any thread starts.
        props.setProperty("i2p.dir.base", System.getProperty("i2p.dir.base", dataDir.getAbsolutePath()));
        props.setProperty("i2p.dir.config", dataDir.getAbsolutePath());
        props.setProperty("i2p.dir.log", logsDir.getAbsolutePath());
        props.setProperty("i2p.dir.pid", dataDir.getAbsolutePath());
        props.setProperty("i2p.dir.router", routerDir.getAbsolutePath());
        props.setProperty("i2p.dir.app", appDir.getAbsolutePath());

        props.setProperty("router.configLocation", routerConfig.getAbsolutePath());
        props.setProperty("router.clientConfigFile", clientsConfig.getAbsolutePath());
        props.setProperty("router.pingFile", new File(dataDir, "router.ping").getAbsolutePath());
        // Plan 217 §6.C / §3.5 — `router.networkDatabase.dbDir` MUST be
        // relative to `i2p.dir.router` (Java's
        // `PersistentDataStore.java:643` constructs the final NetDB path
        // as `i2p.dir.router + dbDir`). Supplying an absolute path here
        // produced the recorded doubled-path bug
        // (`${dataDir}/router/${dataDir}/netDb/...`). Use a
        // single-segment relative path so the join is well-formed.
        props.setProperty("router.networkDatabase.dbDir", "../netDb/");
        props.setProperty("router.keyBackupDir", keyBackupDir.getAbsolutePath() + "/");
        props.setProperty("router.profileDir", peerProfilesDir.getAbsolutePath() + "/");
        props.setProperty("router.tunnelPoolFile", new File(dataDir, "tunnelPool.dat").getAbsolutePath());
        props.setProperty("router.sessionKeys.location", new File(dataDir, "sessionKeys.dat").getAbsolutePath());
        props.setProperty("router.info.location", new File(dataDir, "router.info").getAbsolutePath());
        props.setProperty("router.keys.location", new File(dataDir, "router.keys").getAbsolutePath());

        // Plan 196 §5.2 — exact-pinned upstream property names. The
        // obsolete Plan 194 keys (which the static checker now
        // rejects) are NOT used.
        props.setProperty("router.reseedDisable", "true");
        // Plan 229 WP A — role-aware floodfill startup configuration.
        // Router C is a non-floodfill transit participant; A (service)
        // and B (publication) retain floodfill for compatibility with
        // earlier controlled-topology evidence. Legacy invocations
        // (no role argument) retain floodfill enabled.
        if ("transit".equals(role)) {
            props.setProperty("router.floodfillParticipant", "false");
        } else {
            props.setProperty("router.floodfillParticipant", "true");
        }
        // Plan 229 WP B — Router A only uses Java's stock small-router
        // exploratory profile (pinned
        // `installer/resources/small/router.config`). Ordinary public
        // router configuration properties consumed by
        // `TunnelPoolSettings.readFromProperties()`; not a tunnel-policy
        // patch. No quantity, backup quantity, allowZeroHop,
        // explicitPeers, random key, timeout, or paired-tunnel property
        // is set here.
        if ("service".equals(role)) {
            props.setProperty("router.inboundPool.length", "1");
            props.setProperty("router.inboundPool.lengthVariance", "1");
            props.setProperty("router.outboundPool.length", "1");
            props.setProperty("router.outboundPool.lengthVariance", "1");
        }
        props.setProperty("router.rebuildKeys", "false");
        props.setProperty("router.rejectStartupTime", "0");
        props.setProperty("router.newsRefreshFrequency", "0");
        props.setProperty("router.updateDisabled", "true");
        props.setProperty("time.disabled", "true");
        // The exact-pinned upstream `blocklist.txt` includes the
        // Team Cymru bogon list (which covers 127.0.0.0/8); for the
        // controlled loopback topology we MUST disable the blocklist
        // so Java accepts SessionRequest/TokenRequest from 127.0.0.1.
        // The Plan 196 controlled-launcher does not talk to public
        // peers, so this is fail-closed at the daemon boundary.
        props.setProperty("router.blocklist.enable", "false");

        // Loopback-only UDP transport. NTCP/SSU are disabled because
        // the lane only counts SSU2; disabling the legacy transports
        // keeps the controlled profile self-consistent.
        props.setProperty("logger.defaultLevel", "DEBUG");
        props.setProperty("i2np.udp.host", ssu2Host);
        props.setProperty("i2np.udp.port", ssu2Port);
        props.setProperty("i2np.udp.internalPort", ssu2Port);
        props.setProperty("i2np.udp.addressSources", "local");
        props.setProperty("i2np.udp.enable", "true");
        props.setProperty("i2np.ntcp.enable", "false");
        props.setProperty("i2np.ntcp2.enable", "false");
        props.setProperty("i2np.ntcp.hostname", ssu2Host);
        props.setProperty("i2np.ntcp.port", "0");
        props.setProperty("i2np.allowLocal", "true");
        props.setProperty("i2np.upnp.enable", "false");

        // I2CP server binds loopback only on the harness-selected port.
        // Plan 201 Branch C/D corrective — when i2cpPort is "0", skip
        // the I2CP server properties so Java does not bind the port.
        if (!"0".equals(i2cpPort)) {
            props.setProperty("i2cp.tcp.bindAllInterfaces", "false");
            props.setProperty("i2cp.tcp.host", "127.0.0.1");
            props.setProperty("i2cp.tcp.port", i2cpPort);
            props.setProperty("i2cp.port", i2cpPort);
        }

        // Plan 196 §5.5 — belt-and-braces: even though router.reseedDisable
        // is true, also override any URL the Router might persist.
        props.setProperty("i2p.reseedURL", "https://127.0.0.1:1/disabled");

        // Disable router console noise (we never want a webapp launch).
        props.setProperty("routerconsole.enable", "false");

        // Plan 196 §5.2 — prewrite the controlled Properties to the
        // router.config file before constructing Router. The exact-pinned
        // upstream `MultiRouter` uses the same `DataHelper.storeProps`
        // pattern (MultiRouter.java: buildRouterProps) so the file on
        // disk reflects our controlled topology from the very first
        // saveConfig() invocation; without this prewrite the Router's
        // early saveConfig() persists only the keys it has just
        // generated (SSU2 initial keypair, random pool keys, version
        // metadata) and the controlled-topology keys never reach disk.
        if (!routerConfig.exists()) {
            File parent = routerConfig.getParentFile();
            if (parent != null && !parent.exists() && !parent.mkdirs()) {
                System.err.println(
                    "ControlledRouter: cannot create router.config parent dir "
                        + parent);
                System.exit(70);
            }
            try {
                DataHelper.storeProps(props, routerConfig);
            } catch (IOException e) {
                System.err.println(
                    "ControlledRouter: cannot prewrite router.config: "
                        + e.getMessage());
                System.exit(70);
            }
        }

        try (FileWriter w = new FileWriter(pidFile)) {
            w.write(Long.toString(ProcessHandle.current().pid()));
        } catch (IOException e) {
            System.err.println(
                "ControlledRouter: cannot write router.pid: " + e.getMessage());
        }

        final Router router = new Router(props);
        router.setKillVMOnEnd(false);

        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            try {
                router.shutdownGracefully();
            } catch (Throwable t) {
                // best-effort; the harness owns cleanup
            }
        }, "ControlledRouter-ShutdownHook"));

        // Plan 219 — read-only J219 diagnostic control server.
        // Optional; bound to 127.0.0.1 only when a non-zero
        // j219-control-port is supplied. The server answers
        // bounded read-only commands (snapshot / capabilities /
        // peers-floodfill / main-router-count / stored-ri /
        // client-db-lookup-peer-count / quit) using only public
        // RouterContext accessors. It MUST NOT mutate router
        // state. Sanitized output only.
        if (!"0".equals(j219ControlPort)) {
            try {
                int port = Integer.parseInt(j219ControlPort);
                J219DiagnosticServer diagnostic = new J219DiagnosticServer(router, port);
                diagnostic.start();
            } catch (NumberFormatException nfe) {
                System.err.println(
                    "ControlledRouter: j219-control-port must be an integer: "
                        + j219ControlPort);
                System.exit(64);
            }
        }

        System.out.println("ControlledRouter: starting router with ssu2="
            + ssu2Host + ":" + ssu2Port
            + " i2cp=127.0.0.1:" + i2cpPort
            + " sam=127.0.0.1:" + samPort
            + " datadir=" + dataDir.getAbsolutePath()
            + " role=" + role
            + " j219-control-port=" + j219ControlPort);

        router.runRouter();

        // The Router's runRouter() blocks until shutdown; we add an
        // explicit busy-wait fallback in case runRouter() ever returns
        // early so the JVM stays alive until the harness shuts us down.
        while (router.isAlive()) {
            try {
                Thread.sleep(1000);
            } catch (InterruptedException ie) {
                Thread.currentThread().interrupt();
                break;
            }
        }
    }

    private static void writeClientsConfig(File target, String samPort, String i2cpPort) throws IOException {
        try (FileWriter w = new FileWriter(target)) {
            w.write("# ControlledRouter disposable clients.config (Plan 196 §5.3)\n");
            // Plan 201 Branch C/D corrective — when samPort is "0", the
            // tunnel-participant router does not bind the SAM bridge or
            // I2CP server. Skipping the client app entry means Java's
            // Router never starts SAMBridge/I2cpServer threads on port 0.
            if ("0".equals(samPort) && "0".equals(i2cpPort)) {
                w.write("// Tunnel-participant: no SAM bridge or I2CP server started.\n");
            } else {
                w.write("# Only the SAM bridge is started. No router console,\n");
                w.write("# browser launcher, eepsite, or i2ptunnel is loaded.\n");
                w.write("clientApp.0.main=net.i2p.sam.SAMBridge\n");
                w.write("clientApp.0.name=SAM application bridge\n");
                w.write("clientApp.0.args=sam.keys 127.0.0.1 " + samPort
                    + " i2cp.tcp.host=127.0.0.1 i2cp.tcp.port=" + i2cpPort + "\n");
                w.write("clientApp.0.startOnLoad=true\n");
            }
        }
    }

    /**
     * Plan 219 §6 read-only diagnostic control server. Lives in
     * the same JVM as the controlled {@link Router} and answers
     * a small bounded set of read-only commands using only
     * public RouterContext accessors. NEVER mutates router state,
     * NEVER logs sensitive material, NEVER accepts generic
     * Java commands. Each response is a single
     * {@code J219-EV <key>=<value> [...]} line. The harness
     * scopes its writes to
     * {@code target/interop/m6-java-evidence/j219-snapshots-*.tsv}.
     */
    private static final class J219DiagnosticServer {

        private static final String RESPONSE_PREFIX = "J219-EV ";
        private static final String READY_TOKEN = "J219-READY role=";
        private static final String ERROR_TOKEN = "J219-ERROR ";

        private final Router router;
        private final int port;
        private ServerSocket socket;
        private Thread acceptThread;
        private final AtomicReference<String> role = new AtomicReference<>("unset");

        J219DiagnosticServer(Router router, int port) {
            this.router = router;
            this.port = port;
        }

        /**
         * Optional caller-side hint that names this router
         * ("A" / "B" / "C"). The harness sets this via the
         * {@code J219-ROLE} command immediately after the
         * server accepts its first connection; until then the
         * server emits {@code role=unset}.
         */
        void start() throws IOException {
            // Plan 219 diagnostic contract — loopback only, no
            // public bind. The harness reserves the port and
            // hands the SAME port to ControlledRouter; a future
            // port collision is a typed IOException.
            socket = new ServerSocket(port, 8, InetAddress.getByName("127.0.0.1"));
            acceptThread = new Thread(this::acceptLoop, "J219Diagnostic-Accept");
            acceptThread.setDaemon(true);
            acceptThread.start();
        }

        private void acceptLoop() {
            while (!socket.isClosed()) {
                try {
                    Socket client = socket.accept();
                    Thread worker = new Thread(() -> handle(client),
                        "J219Diagnostic-Worker");
                    worker.setDaemon(true);
                    worker.start();
                } catch (IOException ioe) {
                    if (socket.isClosed()) {
                        return;
                    }
                    // best-effort; the harness owns cleanup
                }
            }
        }

        private void handle(Socket client) {
            try (Socket socketHandle = client;
                 BufferedReader input = new BufferedReader(
                     new InputStreamReader(socketHandle.getInputStream(),
                         StandardCharsets.US_ASCII));
                 PrintWriter output = new PrintWriter(
                     socketHandle.getOutputStream(), true)) {
                String line;
                while ((line = input.readLine()) != null) {
                    String trimmed = line.trim();
                    if (trimmed.isEmpty()) {
                        continue;
                    }
                    String response = dispatch(trimmed);
                    output.println(response);
                    if (trimmed.startsWith("QUIT")) {
                        socketHandle.close();
                        return;
                    }
                }
            } catch (IOException ioe) {
                // best-effort
            }
        }

        private String dispatch(String line) {
            String[] parts = line.split(" ");
            String command = parts[0];
            try {
                switch (command) {
                    case "J219-ROLE":
                        if (parts.length < 2) {
                            return ERROR_TOKEN + "missing-role-argument";
                        }
                        String requested = parts[1];
                        if (!requested.matches("[A-C]")) {
                            return ERROR_TOKEN + "role-must-be-A-B-or-C";
                        }
                        role.set(requested);
                        return READY_TOKEN + requested;
                    case "J219-SNAPSHOT":
                        return snapshotSelf();
                    case "J219-CAPABILITIES":
                        if (parts.length < 2) {
                            return ERROR_TOKEN + "missing-hash-argument";
                        }
                        return lookupCapabilities(parts[1]);
                    case "J219-STORED-RI":
                        if (parts.length < 2) {
                            return ERROR_TOKEN + "missing-hash-argument";
                        }
                        return lookupStored(parts[1]);
                    case "J219-PEERS-FLOODFILL":
                        return peersFloodfill();
                    case "J219-MAIN-ROUTER-COUNT":
                        return mainRouterCount();
                    case "J219-CLIENT-DB-LOOKUP-PEER-COUNT":
                        return clientDbLookupPeerCount();
                    // Plan 220 — authoritative hex-hash path. The J219-*
                    // cases above are frozen history; the P220-* cases
                    // below are the only inputs the corrected
                    // classifier consumes.
                    case "P220-SNAPSHOT":
                        return p220SnapshotSelf();
                    case "P220-CAPABILITIES":
                        if (parts.length < 2) {
                            return p220Error("missing-hash-argument");
                        }
                        return p220LookupCapabilities(parts[1]);
                    case "P220-STORED-RI":
                        if (parts.length < 2) {
                            return p220Error("missing-hash-argument");
                        }
                        return p220LookupStored(parts[1]);
                    case "P220-PEERS-FLOODFILL":
                        return p220PeersFloodfill();
                    case "P220-MAIN-ROUTER-COUNT":
                        return p220MainRouterCount();
                    case "P220-SELECTOR":
                        if (parts.length < 3) {
                            return p220Error("missing-hash-arguments");
                        }
                        return p220Selector(parts[1], parts[2]);
                    case "P222-CLIENT-LOOKUP-PREFLIGHT":
                        if (parts.length < 4) {
                            return p222Error("missing-hash-arguments");
                        }
                        return p222ClientLookupPreflight(parts[1], parts[2], parts[3]);
                    case "P223-DEST-INSPECT":
                        if (parts.length < 2) {
                            return p223Error("missing-dest-argument");
                        }
                        return P223BranchProbe.inspectDestination(parts[1]);
                    case "P223-BRANCH":
                        if (parts.length < 4) {
                            return p223Error("missing-hash-arguments");
                        }
                        return p223Branch(parts[1], parts[2], parts[3]);
                    case "P224-MAIN-LS":
                        if (parts.length < 2) {
                            return p224Error("missing-hash-argument");
                        }
                        return p224MainLs(parts[1]);
                    case "P224-CLIENT-LS":
                        if (parts.length < 3) {
                            return p224Error("missing-hash-arguments");
                        }
                        return p224ClientLs(parts[1], parts[2]);
                    case "P224-HASH-B64":
                        if (parts.length < 2) {
                            return p224Error("missing-hash-argument");
                        }
                        return p224HashB64(parts[1]);
                    case "P225-LOGGER-CONFIG":
                        return p225LoggerConfig();
                    case "P225-HASH-B32":
                        if (parts.length < 2) {
                            return p225Error("missing-hash-argument");
                        }
                        return p225HashB32(parts[1]);
                    case "P227-PEER-ELIGIBILITY":
                        if (parts.length < 2) {
                            return p227Error("missing-hash-argument");
                        }
                        return p227PeerEligibility(parts[1]);
                    case "P227-CLIENT-TUNNELS":
                        if (parts.length < 3) {
                            return p227Error("missing-hash-arguments");
                        }
                        return p227ClientTunnels(parts[1], parts[2]);
                    case "P228-TUNNEL-INFRA":
                        return p228TunnelInfra();
                    case "P228-CLIENT-POOLS":
                        if (parts.length < 2) {
                            return p228Error("missing-hash-argument");
                        }
                        return p228ClientPools(parts[1]);
                    case "P228-LOGGER-CONFIG":
                        return p228LoggerConfig();
                    case "P229-TRANSIT-PEER":
                        if (parts.length < 2) {
                            return p229Error("missing-hash-argument");
                        }
                        return p229TransitPeer(parts[1]);
                    case "P229-EXPLORATORY-SETTINGS":
                        return p229ExploratorySettings();
                    case "P229-EXPLORATORY-TUNNELS":
                        if (parts.length < 2) {
                            return p229Error("missing-hash-argument");
                        }
                        return p229ExploratoryTunnels(parts[1]);
                    case "P230-CAPABILITY":
                        if (parts.length < 2) {
                            return p230Error("missing-hash-argument");
                        }
                        return p230Capability(parts[1]);
                    case "P230-SELF-VIEW":
                        return p230SelfView();
                    case "PING":
                        return "PONG";
                    case "QUIT":
                        return "BYE";
                    default:
                        return ERROR_TOKEN + "unknown-command";
                }
            } catch (Throwable t) {
                return ERROR_TOKEN + "internal-error " + t.getClass().getSimpleName();
            }
        }

        private RouterContext context() {
            return router.getContext();
        }

        private KademliaNetworkDatabaseFacade mainNetDb() {
            return (KademliaNetworkDatabaseFacade) context().netDb();
        }

        private PeerManagerFacade peerManager() {
            return context().peerManager();
        }

        private String snapshotSelf() {
            StringBuilder sb = new StringBuilder(RESPONSE_PREFIX);
            sb.append("kind=snapshot ");
            sb.append("role=").append(role.get()).append(' ');
            RouterInfo self = mainNetDb().lookupRouterInfoLocally(context().routerHash());
            if (self == null) {
                sb.append("self_ri_present=false");
                return sb.toString();
            }
            sb.append("self_ri_present=true ");
            sb.append("self_router_hash=").append(self.getIdentity().getHash().toBase64()).append(' ');
            sb.append("self_routerinfo_sha256=").append(sha256Hex(self.toByteArray())).append(' ');
            sb.append("self_published_seconds=").append(self.getPublished() / 1000L).append(' ');
            sb.append("self_capabilities=").append(self.getCapabilities()).append(' ');
            sb.append("self_bandwidth_tier=").append(self.getBandwidthTier()).append(' ');
            sb.append("self_has_floodfill_capability=")
                .append(self.getCapabilities().indexOf('f') >= 0).append(' ');
            sb.append("self_ssu2_address_count=").append(countSsu2Addresses(self));
            return sb.toString();
        }

        private String lookupCapabilities(String b64Hash) {
            Hash hash;
            try {
                hash = new Hash(Base64.decode(b64Hash));
            } catch (IllegalArgumentException iae) {
                return ERROR_TOKEN + "invalid-base64";
            }
            RouterInfo info = mainNetDb().lookupRouterInfoLocally(hash);
            if (info == null) {
                return RESPONSE_PREFIX + "kind=capabilities hash=" + b64Hash
                    + " stored=false";
            }
            return RESPONSE_PREFIX + "kind=capabilities hash=" + b64Hash
                + " stored=true "
                + "published_seconds=" + (info.getPublished() / 1000L)
                + " capabilities=\"" + info.getCapabilities() + "\""
                + " has_floodfill_capability=" + (info.getCapabilities().indexOf('f') >= 0)
                + " bandwidth_tier=" + info.getBandwidthTier()
                + " ssu2_address_count=" + countSsu2Addresses(info);
        }

        private String lookupStored(String b64Hash) {
            Hash hash;
            try {
                hash = new Hash(Base64.decode(b64Hash));
            } catch (IllegalArgumentException iae) {
                return ERROR_TOKEN + "invalid-base64";
            }
            RouterInfo info = mainNetDb().lookupRouterInfoLocally(hash);
            if (info == null) {
                return RESPONSE_PREFIX + "kind=stored-ri hash=" + b64Hash
                    + " present=false";
            }
            return RESPONSE_PREFIX + "kind=stored-ri hash=" + b64Hash
                + " present=true "
                + "routerinfo_sha256=" + sha256Hex(info.toByteArray())
                + " published_seconds=" + (info.getPublished() / 1000L)
                + " has_floodfill_capability=" + (info.getCapabilities().indexOf('f') >= 0);
        }

        private String peersFloodfill() {
            Set<Hash> floodfill = peerManager().getPeersByCapability('f');
            StringBuilder sb = new StringBuilder(RESPONSE_PREFIX);
            sb.append("kind=peers-floodfill ");
            sb.append("count=").append(floodfill.size()).append(' ');
            int shown = 0;
            for (Hash h : floodfill) {
                if (shown >= 4) {
                    break;
                }
                sb.append("peer_").append(shown).append('=').append(h.toBase64()).append(' ');
                shown++;
            }
            sb.append("truncated=").append(floodfill.size() > shown);
            return sb.toString();
        }

        private String mainRouterCount() {
            Set<RouterInfo> routers = mainNetDb().getRouters();
            return RESPONSE_PREFIX + "kind=main-router-count count=" + routers.size();
        }

        /**
         * {@link PeerManager} exposes the candidates that
         * {@link net.i2p.router.networkdb.kademlia.FloodfillPeerSelector}
         * consumes via {@code peerManager().getPeersByCapability('f')}.
         * The client-specific lookup's "fallback peer set" is the
         * same membership plus any arbitrary known routers the
         * {@code IterativeSearchJob.runJob()} fallback reaches; in
         * this controlled loopback topology the latter is empty,
         * so the cardinality of the floodfill capability index is
         * a faithful signal of the helper's lookup-peer budget.
         */
        private String clientDbLookupPeerCount() {
            Set<Hash> floodfill = peerManager().getPeersByCapability('f');
            return RESPONSE_PREFIX + "kind=client-db-lookup-peer-count count="
                + floodfill.size();
        }

        private String p220Error(String reason) {
            return "P220-ERROR " + reason;
        }

        /**
         * Plan 220 WP B — parses a 32-byte lowercase-hex RouterHash.
         * Returns null when the argument is not exactly 64 hex
         * characters. Standard (RFC 4648) Base64 is never accepted
         * here: I2P {@link Hash#toBase64()} uses I2P Base64
         * semantics, so a standard-Base64 query hash may address a
         * different router (Plan 219 D220-2).
         */
        private Hash p220ParseHexHash(String hex) {
            if (hex == null || hex.length() != 64) {
                return null;
            }
            byte[] raw = new byte[32];
            for (int i = 0; i < 32; i++) {
                int hi = Character.digit(hex.charAt(2 * i), 16);
                int lo = Character.digit(hex.charAt(2 * i + 1), 16);
                if (hi < 0 || lo < 0) {
                    return null;
                }
                raw[i] = (byte) ((hi << 4) | lo);
            }
            return new Hash(raw);
        }

        private static String p220HexLower(byte[] raw) {
            StringBuilder hex = new StringBuilder(raw.length * 2);
            for (byte b : raw) {
                hex.append(String.format("%02x", b & 0xff));
            }
            return hex.toString();
        }

        private String p220SnapshotSelf() {
            StringBuilder sb = new StringBuilder("P220-EV ");
            sb.append("kind=snapshot ");
            sb.append("role=").append(role.get()).append(' ');
            RouterInfo self = mainNetDb().lookupRouterInfoLocally(context().routerHash());
            if (self == null) {
                sb.append("self_ri_present=false");
                return sb.toString();
            }
            Hash selfHash = self.getIdentity().getHash();
            sb.append("self_ri_present=true ");
            sb.append("self_router_hash_hex=").append(p220HexLower(selfHash.getData())).append(' ');
            sb.append("self_router_hash_b64=").append(selfHash.toBase64()).append(' ');
            sb.append("self_routerinfo_sha256=").append(sha256Hex(self.toByteArray())).append(' ');
            sb.append("self_published_seconds=").append(self.getPublished() / 1000L).append(' ');
            sb.append("self_capabilities=").append(self.getCapabilities()).append(' ');
            sb.append("self_bandwidth_tier=").append(self.getBandwidthTier()).append(' ');
            sb.append("self_has_floodfill_capability=")
                .append(self.getCapabilities().indexOf('f') >= 0).append(' ');
            sb.append("self_ssu2_address_count=").append(countSsu2Addresses(self));
            return sb.toString();
        }

        /**
         * Plan 220 WP C — exact stored-RouterInfo evidence for one
         * query hash. Presence alone is never enough: the response
         * echoes the query hash, the stored identity hash, and
         * whether the two are byte-equal, plus the stored record's
         * SHA-256, published timestamp, capabilities, and `f`
         * membership. A "stored B RI lacks f" classification is
         * allowed only when `stored_identity_match=true` and the
         * stored record demonstrably lacks `f`.
         */
        private String p220LookupStored(String hexHash) {
            Hash hash = p220ParseHexHash(hexHash);
            if (hash == null) {
                return p220Error("invalid-hex-hash");
            }
            RouterInfo info = mainNetDb().lookupRouterInfoLocally(hash);
            if (info == null) {
                return "P220-EV kind=stored-ri query_hash_hex=" + hexHash
                    + " query_hash_b64=" + hash.toBase64()
                    + " present=false";
            }
            Hash storedHash = info.getIdentity().getHash();
            String storedHex = p220HexLower(storedHash.getData());
            return "P220-EV kind=stored-ri query_hash_hex=" + hexHash
                + " query_hash_b64=" + hash.toBase64()
                + " present=true "
                + "stored_router_hash_hex=" + storedHex
                + " stored_router_hash_b64=" + storedHash.toBase64()
                + " stored_identity_match=" + storedHex.equalsIgnoreCase(hexHash)
                + " routerinfo_sha256=" + sha256Hex(info.toByteArray())
                + " published_seconds=" + (info.getPublished() / 1000L)
                + " capabilities=\"" + info.getCapabilities() + "\""
                + " has_floodfill_capability=" + (info.getCapabilities().indexOf('f') >= 0);
        }

        private String p220LookupCapabilities(String hexHash) {
            Hash hash = p220ParseHexHash(hexHash);
            if (hash == null) {
                return p220Error("invalid-hex-hash");
            }
            RouterInfo info = mainNetDb().lookupRouterInfoLocally(hash);
            if (info == null) {
                return "P220-EV kind=capabilities query_hash_hex=" + hexHash
                    + " stored=false";
            }
            return "P220-EV kind=capabilities query_hash_hex=" + hexHash
                + " stored=true "
                + "published_seconds=" + (info.getPublished() / 1000L)
                + " capabilities=\"" + info.getCapabilities() + "\""
                + " has_floodfill_capability=" + (info.getCapabilities().indexOf('f') >= 0)
                + " bandwidth_tier=" + info.getBandwidthTier()
                + " ssu2_address_count=" + countSsu2Addresses(info);
        }

        private String p220PeersFloodfill() {
            Set<Hash> floodfill = peerManager().getPeersByCapability('f');
            StringBuilder sb = new StringBuilder("P220-EV ");
            sb.append("kind=peers-floodfill ");
            sb.append("count=").append(floodfill.size()).append(' ');
            int shown = 0;
            for (Hash h : floodfill) {
                if (shown >= 4) {
                    break;
                }
                sb.append("peer_").append(shown).append("_hex=")
                    .append(p220HexLower(h.getData())).append(' ');
                shown++;
            }
            sb.append("truncated=").append(floodfill.size() > shown);
            // Plan 220 WP D — the banlist / explicit-ignore state has
            // no public read-only accessor on PeerManagerFacade, so
            // the driver records it as Unknown rather than defaulting
            // it to false.
            sb.append(" banlist_observable=false");
            return sb.toString();
        }

        private String p220MainRouterCount() {
            Set<RouterInfo> routers = mainNetDb().getRouters();
            return "P220-EV kind=main-router-count count=" + routers.size();
        }

        /**
         * Plan 220 WP D — actual {@code FloodfillPeerSelector}
         * output for the reverse-lookup routing key, computed
         * read-only on the live k-buckets by the test-only
         * same-package probe. This is never synthesized from
         * PeerManager membership: the selector ranks live
         * k-bucket contents, while PeerManager reports profile
         * capability-index membership.
         */
        private String p220Selector(String targetHex, String bHex) {
            Hash target = p220ParseHexHash(targetHex);
            Hash bHash = p220ParseHexHash(bHex);
            if (target == null || bHash == null) {
                return p220Error("invalid-hex-hash");
            }
            P220SelectorProbe.Result result =
                P220SelectorProbe.selectForKey(context(), target, bHash);
            if (result.error != null) {
                return "P220-EV kind=selector target_hash_hex=" + targetHex
                    + " b_hash_hex=" + bHex
                    + " observable=false reason=" + result.error;
            }
            return "P220-EV kind=selector target_hash_hex=" + targetHex
                + " b_hash_hex=" + bHex
                + " observable=true "
                + "selector_input_kbucket_size=" + result.kbucketSize
                + " selector_count=" + result.selected.size()
                + " selector_contains_b=" + result.selected.contains(bHash)
                + " probe_inputs=key-target,N-3,exclude-empty,live-kbuckets";
        }

        private String p222Error(String reason) {
            return "P222-ERROR " + reason;
        }

        private String p223Error(String reason) {
            return "P223-ERROR " + reason;
        }

        private String p224Error(String reason) {
            return "P224-ERROR " + reason;
        }

        private String p225Error(String reason) {
            return "P225-ERROR " + reason;
        }

        private String p227Error(String reason) {
            return "P227-ERROR " + reason;
        }

        private String p228Error(String reason) {
            return "P228-ERROR " + reason;
        }

        private String p229Error(String reason) {
            return "P229-ERROR " + reason;
        }

        private String p230Error(String reason) {
            return "P230-ERROR " + reason;
        }

        /**
         * Plan 230 WP A — read-only Router-C capability snapshot on
         * this router's main NetDB, plus the local observer inputs for
         * the exact pinned `ProfileManagerImpl.shouldCreate(caps)`
         * predicate. Uses only public read-only accessors via P230Probe;
         * never creates a profile, never calls the deprecated
         * `ProfileOrganizer.isFailing(Hash)`, never promotes tiers,
         * never forces connections, never stores RouterInfos.
         */
        private String p230Capability(String routerCHex) {
            Hash routerC = p220ParseHexHash(routerCHex);
            if (routerC == null) {
                return p230Error("invalid-hex-hash");
            }
            P230Probe.Capability result =
                P230Probe.snapshotCapability(context(), mainNetDb(), routerC);
            if (result.error != null) {
                return "P230-EV kind=capability"
                    + " router_c_hex=" + routerCHex
                    + " observable=false reason=" + result.error;
            }
            return "P230-EV kind=capability"
                + " router_c_hex=" + routerCHex
                + " observable=true"
                + " main_raw_present=" + result.mainRawPresent
                + " main_valid_present=" + result.mainValidPresent
                + " selectable=" + result.selectable
                + " banlisted=" + result.banlisted
                + " caps_has_r=" + result.capsHasR
                + " caps_has_u=" + result.capsHasU
                + " caps_has_f=" + result.capsHasF
                + " caps_has_l=" + result.capsHasL
                + " caps_has_e=" + result.capsHasE
                + " caps_has_g=" + result.capsHasG
                + " bandwidth_tier=" + result.bandwidthTier
                + " c_ri_sha256=" + result.cRiSha256Hex
                + " profile_present=" + result.profilePresent
                + " profile_count=" + result.profileCount
                + " not_failing_count=" + result.notFailingCount
                + " local_floodfill_enabled=" + result.localFloodfillEnabled
                + " local_max_share_bandwidth=" + result.localMaxShareBandwidth
                + " local_comm_status=" + result.localCommStatus
                + " heard_about_creation_eligible=" + result.heardAboutCreationEligible;
        }

        /**
         * Plan 230 WP A/D — read-only self view: this router's own
         * communication-system status plus its current self RouterInfo
         * capabilities, so a stale pre-correction copy observed by
         * Router A can be distinguished from a newly published RI.
         * Observation only; never mutates state.
         */
        private String p230SelfView() {
            P230Probe.SelfView result =
                P230Probe.snapshotSelf(context(), mainNetDb());
            if (result.error != null) {
                return "P230-EV kind=self-view"
                    + " observable=false reason=" + result.error;
            }
            return "P230-EV kind=self-view"
                + " observable=true"
                + " self_comm_status=" + result.selfCommStatus
                + " self_caps_has_r=" + result.selfCapsHasR
                + " self_caps_has_u=" + result.selfCapsHasU
                + " self_caps_has_f=" + result.selfCapsHasF
                + " self_caps_has_l=" + result.selfCapsHasL
                + " self_caps_has_e=" + result.selfCapsHasE
                + " self_caps_has_g=" + result.selfCapsHasG
                + " self_bandwidth_tier=" + result.selfBandwidthTier
                + " self_ri_sha256=" + result.selfRiSha256Hex;
        }

        /**
         * Plan 229 WP C — read-only Router-C transit-peer snapshot on
         * this router's main NetDB. Uses only public read-only accessors
         * via P229Probe; never creates a profile, never promotes tiers,
         * never forces connections, never stores RouterInfos.
         */
        private String p229TransitPeer(String routerCHex) {
            Hash routerC = p220ParseHexHash(routerCHex);
            if (routerC == null) {
                return p229Error("invalid-hex-hash");
            }
            P229Probe.TransitPeer result =
                P229Probe.snapshotTransitPeer(context(), mainNetDb(), routerC);
            if (result.error != null) {
                return "P229-EV kind=transit-peer"
                    + " router_c_hex=" + routerCHex
                    + " observable=false reason=" + result.error;
            }
            return "P229-EV kind=transit-peer"
                + " router_c_hex=" + routerCHex
                + " observable=true"
                + " main_raw_present=" + result.mainRawPresent
                + " main_valid_present=" + result.mainValidPresent
                + " profile_present=" + result.profilePresent
                + " selectable=" + result.selectable
                + " banlisted=" + result.banlisted
                + " unreachable=" + result.unreachable
                + " caps_has_f=" + result.capsHasF
                + " profile_count=" + result.profileCount
                + " not_failing_count=" + result.notFailingCount;
        }

        /**
         * Plan 229 WP B — read-only effective exploratory-pool settings.
         * Observation only via `getInboundSettings` /
         * `getOutboundSettings`; never mutates pool settings.
         */
        private String p229ExploratorySettings() {
            P229Probe.ExploratorySettings result =
                P229Probe.snapshotExploratorySettings(context());
            if (result.error != null) {
                return "P229-EV kind=exploratory-settings"
                    + " observable=false reason=" + result.error;
            }
            return "P229-EV kind=exploratory-settings"
                + " observable=true"
                + " inbound_length=" + result.inboundLength
                + " inbound_variance=" + result.inboundVariance
                + " inbound_quantity=" + result.inboundQuantity
                + " outbound_length=" + result.outboundLength
                + " outbound_variance=" + result.outboundVariance
                + " outbound_quantity=" + result.outboundQuantity;
        }

        /**
         * Plan 229 WP D — read-only exploratory-tunnel install snapshot.
         * Counts genuine non-zero tunnels and records Router-C membership
         * read-only; no peer paths exposed. Never builds, installs, or
         * mutates tunnels.
         */
        private String p229ExploratoryTunnels(String routerCHex) {
            Hash routerC = p220ParseHexHash(routerCHex);
            if (routerC == null) {
                return p229Error("invalid-hex-hash");
            }
            P229Probe.ExploratoryTunnels result =
                P229Probe.snapshotExploratoryTunnels(context(), routerC);
            if (result.error != null) {
                return "P229-EV kind=exploratory-tunnels"
                    + " router_c_hex=" + routerCHex
                    + " observable=false reason=" + result.error;
            }
            return "P229-EV kind=exploratory-tunnels"
                + " router_c_hex=" + routerCHex
                + " observable=true"
                + " inbound_exploratory_count=" + result.inboundExploratoryCount
                + " outbound_exploratory_count=" + result.outboundExploratoryCount
                + " inbound_nonzero_count=" + result.inboundNonzeroCount
                + " outbound_nonzero_count=" + result.outboundNonzeroCount
                + " inbound_c_present=" + result.inboundCPresent
                + " outbound_c_present=" + result.outboundCPresent
                + " zero_hop_fallback_present=" + result.zeroHopFallbackPresent;
        }

        /**
         * Plan 228 WP A — read-only router tunnel-infrastructure snapshot.
         * Uses only the public TunnelManagerFacade accessors via
         * P228Probe; never builds, installs, or mutates tunnels. No peer
         * paths, keys, or log text exposed.
         */
        private String p228TunnelInfra() {
            P228Probe.Infra result =
                P228Probe.snapshotTunnelInfra(context());
            if (result.error != null && !result.observable) {
                return "P228-EV kind=tunnel-infra"
                    + " observable=false reason=" + result.error;
            }
            return "P228-EV kind=tunnel-infra"
                + " observable=true"
                + " free_tunnel_count=" + result.freeTunnelCount
                + " inbound_tunnel_count=" + result.inboundTunnelCount
                + " outbound_tunnel_count=" + result.outboundTunnelCount
                + " inbound_exploratory_count=" + result.inboundExploratoryCount
                + " outbound_exploratory_count=" + result.outboundExploratoryCount
                + " inbound_exploratory_nonzero_count=" + result.inboundExploratoryNonzeroCount
                + " outbound_exploratory_nonzero_count=" + result.outboundExploratoryNonzeroCount;
        }

        /**
         * Plan 228 WP B/G — read-only client-pool presence/count snapshot
         * for one helper client DBID. Never installs, builds, or mutates
         * tunnels. No peer paths exposed.
         */
        private String p228ClientPools(String clientDbidHex) {
            Hash clientDbid = p220ParseHexHash(clientDbidHex);
            if (clientDbid == null) {
                return p228Error("invalid-hex-hash");
            }
            P228Probe.ClientPools result =
                P228Probe.snapshotClientPools(context(), clientDbid);
            if (result.error != null && !result.observable) {
                return "P228-EV kind=client-pools"
                    + " client_dbid_hex=" + clientDbidHex
                    + " observable=false reason=" + result.error;
            }
            return "P228-EV kind=client-pools"
                + " client_dbid_hex=" + clientDbidHex
                + " observable=true"
                + " inbound_pool_present=" + result.inboundPoolPresent
                + " outbound_pool_present=" + result.outboundPoolPresent
                + " inbound_tunnel_count=" + result.inboundTunnelCount
                + " outbound_tunnel_count=" + result.outboundTunnelCount;
        }

        /**
         * Plan 228 WP B-F — read the effective logger levels for the
         * exact build-path scopes from the running LogManager.
         * Observation only: never reloads, sets, or mutates configuration.
         */
        private String p228LoggerConfig() {
            try {
                net.i2p.util.LogManager manager = context().logManager();
                String defaultLevel = manager.getDefaultLimit();
                String tunnelPoolLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.TunnelPool"
                ).getMinimumPriority());
                String tpsLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.TunnelPeerSelector"
                ).getMinimumPriority());
                String cpsLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.ClientPeerSelector"
                ).getMinimumPriority());
                String execLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.BuildExecutor"
                ).getMinimumPriority());
                String reqLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.BuildRequestor"
                ).getMinimumPriority());
                String handlerLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.BuildHandler"
                ).getMinimumPriority());
                String procLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.BuildMessageProcessor"
                ).getMinimumPriority());
                String replyLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.pool.BuildReplyHandler"
                ).getMinimumPriority());
                boolean observable = "ERROR".equals(defaultLevel)
                    && ("DEBUG".equals(execLevel) || "INFO".equals(execLevel))
                    && ("DEBUG".equals(reqLevel) || "INFO".equals(reqLevel))
                    && ("DEBUG".equals(handlerLevel) || "INFO".equals(handlerLevel));
                return "P228-EV kind=logger-config"
                    + " observable=" + observable
                    + " default_level=" + defaultLevel
                    + " tunnel_pool_level=" + tunnelPoolLevel
                    + " tps_level=" + tpsLevel
                    + " cps_level=" + cpsLevel
                    + " build_executor_level=" + execLevel
                    + " build_requestor_level=" + reqLevel
                    + " build_handler_level=" + handlerLevel
                    + " build_processor_level=" + procLevel
                    + " build_reply_level=" + replyLevel;
            } catch (Throwable t) {
                return "P228-EV kind=logger-config observable=false reason=unreadable-"
                    + t.getClass().getSimpleName();
            }
        }

        /**
         * Plan 227 WP A — read-only Router-C eligibility on this router's
         * main NetDB. Uses only public read-only accessors; never creates
         * profiles, never promotes tiers, never forces connections, never
         * stores RouterInfos.
         */
        private String p227PeerEligibility(String routerCHex) {
            Hash routerC = p220ParseHexHash(routerCHex);
            if (routerC == null) {
                return p227Error("invalid-hex-hash");
            }
            P227Probe.Eligibility result =
                P227Probe.snapshotEligibility(context(), mainNetDb(), routerC);
            if (result.error != null) {
                return "P227-EV kind=peer-eligibility"
                    + " router_c_hex=" + routerCHex
                    + " observable=false reason=" + result.error;
            }
            return "P227-EV kind=peer-eligibility"
                + " router_c_hex=" + routerCHex
                + " observable=true"
                + " main_raw_present=" + result.mainRawPresent
                + " main_valid_present=" + result.mainValidPresent
                + " selectable=" + result.selectable
                + " established=" + result.established
                + " banlisted=" + result.banlisted;
        }

        /**
         * Plan 227 WP D — read-only installed client-tunnel snapshot for
         * one helper client DBID. Resolves live pools through public
         * tunnel-manager accessors and inspects installed tunnels
         * read-only. One remote hop via C is the exact local+Router-C
         * path (`getLength() == 2`); zero-hop is `getLength() <= 1`.
         */
        private String p227ClientTunnels(String clientDbidHex, String routerCHex) {
            Hash clientDbid = p220ParseHexHash(clientDbidHex);
            Hash routerC = p220ParseHexHash(routerCHex);
            if (clientDbid == null || routerC == null) {
                return p227Error("invalid-hex-hash");
            }
            P227Probe.Tunnels result =
                P227Probe.snapshotClientTunnels(context(), clientDbid, routerC);
            if (result.error != null) {
                return "P227-EV kind=client-tunnels"
                    + " client_dbid_hex=" + clientDbidHex
                    + " router_c_hex=" + routerCHex
                    + " observable=false reason=" + result.error;
            }
            return "P227-EV kind=client-tunnels"
                + " client_dbid_hex=" + clientDbidHex
                + " router_c_hex=" + routerCHex
                + " observable=true"
                + " client_resolved=" + result.clientResolved
                + " inbound_pool_present=" + result.inboundPoolPresent
                + " outbound_pool_present=" + result.outboundPoolPresent
                + " inbound_tunnel_count=" + result.inboundTunnelCount
                + " outbound_tunnel_count=" + result.outboundTunnelCount
                + " inbound_exact_one_remote_hop_via_c=" + result.inboundExactOneRemoteHopViaC
                + " outbound_exact_one_remote_hop_via_c=" + result.outboundExactOneRemoteHopViaC
                + " inbound_zero_hop_present=" + result.inboundZeroHopPresent
                + " outbound_zero_hop_present=" + result.outboundZeroHopPresent;
        }

        /**
         * Plan 224 WP B — read-only main-NetDB LS snapshot for one
         * target hash. Never stores, publishes, searches, or alters
         * NetDB state. Key type codes and counts only, never key
         * bytes or secret material.
         */
        private String p224MainLs(String targetHex) {
            Hash target = p220ParseHexHash(targetHex);
            if (target == null) {
                return p224Error("invalid-hex-hash");
            }
            P224LsProbe.Result result =
                P224LsProbe.snapshotMain(context(), mainNetDb(), target);
            if (result.error != null && !result.rawPresent && !result.validatedPresent) {
                return "P224-EV kind=main-ls"
                    + " target_hash_hex=" + targetHex
                    + " observable=false reason=" + result.error;
            }
            return "P224-EV kind=main-ls"
                + " target_hash_hex=" + targetHex
                + " observable=true"
                + " raw_present=" + result.rawPresent
                + " validated_present=" + result.validatedPresent
                + " entry_type=" + result.entryType
                + " received_as_published=" + triState(result.receivedAsPublished)
                + " received_as_reply=" + triState(result.receivedAsReply)
                + " received_by_hex=" + result.receivedByHex
                + " ls2_unpublished=" + result.ls2Unpublished
                + " lease_count=" + result.leaseCount
                + " key_count=" + result.keyCount
                + " key_types=" + keyTypesValue(result)
                + " latest_lease_ms=" + result.latestLeaseMs
                + " current=" + triState(result.current);
        }

        /**
         * Plan 224 WP B — read-only helper client-subDB LS snapshot.
         * A main-DB fallback is explicitly non-client and never
         * satisfies Plan-224 client authority. The two accessors are
         * local reads; no network lookup runs, so the sub-DB cannot
         * be primed by this snapshot.
         */
        private String p224ClientLs(String clientDbidHex, String targetHex) {
            Hash clientDbid = p220ParseHexHash(clientDbidHex);
            Hash target = p220ParseHexHash(targetHex);
            if (clientDbid == null || target == null) {
                return p224Error("invalid-hex-hash");
            }
            P224LsProbe.Result result =
                P224LsProbe.snapshotClient(context(), clientDbid, target);
            if (result.error != null) {
                return "P224-EV kind=client-ls"
                    + " client_dbid_hex=" + clientDbidHex
                    + " target_hash_hex=" + targetHex
                    + " observable=false reason=" + result.error
                    + " client_db_resolved=" + result.clientDbResolved
                    + " client_db_is_client=" + result.clientDbIsClient;
            }
            return "P224-EV kind=client-ls"
                + " client_dbid_hex=" + clientDbidHex
                + " target_hash_hex=" + targetHex
                + " observable=true"
                + " client_db_resolved=" + result.clientDbResolved
                + " client_db_is_client=" + result.clientDbIsClient
                + " raw_present=" + result.rawPresent
                + " validated_present=" + result.validatedPresent
                + " entry_type=" + result.entryType
                + " received_as_published=" + triState(result.receivedAsPublished)
                + " received_as_reply=" + triState(result.receivedAsReply)
                + " received_by_hex=" + result.receivedByHex
                + " ls2_unpublished=" + result.ls2Unpublished
                + " lease_count=" + result.leaseCount
                + " key_count=" + result.keyCount
                + " key_types=" + keyTypesValue(result)
                + " latest_lease_ms=" + result.latestLeaseMs
                + " current=" + triState(result.current);
        }

        /**
         * Plan 224 WP E — renders one 32-byte hash exactly as the
         * pinned JVM logs it (`Hash.toBase64()`, I2P alphabet) so the
         * whitelist-only sanitizer correlates exact-target log lines
         * without reimplementing the alphabet. Deterministic;
         * touches no router state.
         */
        private String p224HashB64(String hashHex) {
            Hash hash = p220ParseHexHash(hashHex);
            if (hash == null) {
                return p224Error("invalid-hex-hash");
            }
            String b64;
            try {
                b64 = hash.toBase64();
            } catch (RuntimeException re) {
                return "P224-EV kind=hash-b64 hash_hex=" + hashHex
                    + " observable=false reason=render-failed";
            }
            if (b64 == null || b64.isEmpty() || b64.length() > 128) {
                return "P224-EV kind=hash-b64 hash_hex=" + hashHex
                    + " observable=false reason=render-failed";
            }
            return "P224-EV kind=hash-b64 hash_hex=" + hashHex
                + " observable=true hash_b64=" + b64;
        }

        /**
         * Plan 225 — read the effective logger levels from the running
         * LogManager. This is deliberately an observation only: it does not
         * modify or reload the logger configuration file.
         * The exact fully-qualified scopes are the classes whose pinned log
         * messages carry the lookup-path facts.
         */
        private String p225LoggerConfig() {
            try {
                net.i2p.util.LogManager manager = context().logManager();
                String defaultLevel = manager.getDefaultLimit();
                String isjLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.networkdb.kademlia.IterativeSearchJob"
                ).getMinimumPriority());
                String dlmLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.networkdb.HandleDatabaseLookupMessageJob"
                ).getMinimumPriority());
                String ibmdLevel = Log.toLevelString(manager.getLog(
                    "net.i2p.router.tunnel.InboundMessageDistributor"
                ).getMinimumPriority());
                boolean observable = "ERROR".equals(defaultLevel)
                    && "INFO".equals(isjLevel)
                    && "DEBUG".equals(dlmLevel)
                    && "INFO".equals(ibmdLevel);
                return "P225-EV kind=logger-config"
                    + " observable=" + observable
                    + " default_level=" + defaultLevel
                    + " isj_level=" + isjLevel
                    + " dlm_level=" + dlmLevel
                    + " ibmd_level=" + ibmdLevel;
            } catch (Throwable t) {
                return "P225-EV kind=logger-config observable=false reason=unreadable-"
                    + t.getClass().getSimpleName();
            }
        }

        /**
         * Plan 225 — render the exact Base32 form used by the pinned Java
         * client-tunnel log. This is a deterministic Hash rendering only;
         * it performs no lookup and touches no router state.
         */
        private String p225HashB32(String hashHex) {
            Hash hash = p220ParseHexHash(hashHex);
            if (hash == null) {
                return "P225-ERROR invalid-hex-hash";
            }
            String b32;
            try {
                b32 = hash.toBase32();
            } catch (RuntimeException re) {
                return "P225-EV kind=hash-b32 hash_hex=" + hashHex
                    + " observable=false reason=render-failed";
            }
            // Hash.toBase32() returns the complete 60-character b32.i2p
            // hostname; the router log's `sent to:` field uses that same
            // hostname. Return only its 52-character label so the bounded
            // Rust sanitizer appends the suffix exactly once.
            if (b32 == null || b32.length() != 60 || !b32.endsWith(".b32.i2p")) {
                return "P225-EV kind=hash-b32 hash_hex=" + hashHex
                    + " observable=false reason=render-failed";
            }
            String label = b32.substring(0, 52);
            if (!label.equals(label.toLowerCase()) || !label.matches("[a-z2-7]{52}")) {
                return "P225-EV kind=hash-b32 hash_hex=" + hashHex
                    + " observable=false reason=render-failed";
            }
            return "P225-EV kind=hash-b32 hash_hex=" + hashHex
                + " observable=true hash_b32=" + label;
        }

        private static String triState(Boolean value) {
            if (value == null) {
                return "unknown";
            }
            return value ? "true" : "false";
        }

        private static String keyTypesValue(P224LsProbe.Result result) {
            if (result.keyCodes == null || result.keyCodes.isEmpty()) {
                if (result.keyCount < 0) {
                    return "unknown";
                }
                return "none";
            }
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < result.keyCodes.size(); i++) {
                if (i > 0) {
                    sb.append(",");
                }
                sb.append(result.keyCodes.get(i));
            }
            return sb.toString();
        }

        /**
         * Plan 223 WP C — bounded read-only status-17 branch discriminator.
         * Never registers keys, installs LeaseSets, or alters the client DB.
         */
        private String p223Branch(
                String clientDbidHex, String targetHex, String sourceHex) {
            Hash clientDbid = p220ParseHexHash(clientDbidHex);
            Hash target = p220ParseHexHash(targetHex);
            Hash source = p220ParseHexHash(sourceHex);
            if (clientDbid == null || target == null || source == null) {
                return p223Error("invalid-hex-hash");
            }
            P223BranchProbe.Result result =
                P223BranchProbe.branchForClient(context(), clientDbid, target, source);
            if (result.error != null) {
                return "P223-EV kind=branch"
                    + " client_dbid_hex=" + clientDbidHex
                    + " target_hash_hex=" + targetHex
                    + " source_hash_hex=" + sourceHex
                    + " observable=false reason=" + result.error
                    + " source_keys_present=" + result.sourceKeysPresent
                    + " target_ls_present=" + result.targetLsPresent;
            }
            StringBuilder sb = new StringBuilder("P223-EV kind=branch");
            sb.append(" client_dbid_hex=").append(clientDbidHex);
            sb.append(" target_hash_hex=").append(targetHex);
            sb.append(" source_hash_hex=").append(sourceHex);
            sb.append(" observable=true");
            sb.append(" source_keys_present=").append(result.sourceKeysPresent);
            sb.append(" source_supported_types=");
            if (result.sourceSupportedCodes.isEmpty()) {
                sb.append("none");
            } else {
                for (int i = 0; i < result.sourceSupportedCodes.size(); i++) {
                    if (i > 0) {
                        sb.append(",");
                    }
                    sb.append(result.sourceSupportedCodes.get(i));
                }
            }
            sb.append(" source_supports_elgamal=").append(result.sourceSupportsElgamal);
            sb.append(" source_supports_x25519=").append(result.sourceSupportsX25519);
            sb.append(" target_ls_present=").append(result.targetLsPresent);
            sb.append(" target_ls_type=").append(
                result.targetLsType == null ? "none" : result.targetLsType);
            sb.append(" target_destination_hash_match=").append(result.targetDestinationHashMatch);
            sb.append(" target_destination_enc_type=").append(result.targetDestinationEncType);
            sb.append(" target_key_count=").append(result.targetKeyCount);
            sb.append(" target_key_types=");
            if (result.targetKeyCodes.isEmpty()) {
                sb.append("none");
            } else {
                for (int i = 0; i < result.targetKeyCodes.size(); i++) {
                    if (i > 0) {
                        sb.append(",");
                    }
                    sb.append(result.targetKeyCodes.get(i));
                }
            }
            sb.append(" target_has_x25519=").append(result.targetHasX25519);
            sb.append(" selected_key_present=").append(result.selectedKeyPresent);
            sb.append(" selected_key_type=").append(result.selectedKeyType);
            return sb.toString();
        }

        /**
         * Plan 222 WP B — exact client-lookup preflight through the
         * helper client DBID. Parses all hashes as exact 32-byte
         * lowercase hex, resolves `clientNetDb(clientDbid)`, derives
         * the routing key via `routingKeyGenerator().getRoutingKey`,
         * reproduces the `netdb.searchLimit` + EXTRA_PEERS width, and
         * runs the production-equivalent 3-argument selector overload
         * read-only. Never mutates facade state. If the client facade
         * falls back to main, that is recorded explicitly.
         */
        private String p222ClientLookupPreflight(
                String clientDbidHex, String targetHex, String bHex) {
            Hash clientDbid = p220ParseHexHash(clientDbidHex);
            Hash target = p220ParseHexHash(targetHex);
            Hash bHash = p220ParseHexHash(bHex);
            if (clientDbid == null || target == null || bHash == null) {
                return p222Error("invalid-hex-hash");
            }
            P222SelectorProbe.Result result =
                P222SelectorProbe.preflightForClient(context(), clientDbid, target, bHash);
            if (result.error != null) {
                return "P222-EV kind=client-lookup-preflight"
                    + " client_dbid_hex=" + clientDbidHex
                    + " target_hash_hex=" + targetHex
                    + " b_hash_hex=" + bHex
                    + " observable=false reason=" + result.error
                    + " client_db_resolved=" + result.clientDbResolved
                    + " client_db_is_client=" + result.clientDbIsClient;
            }
            StringBuilder sb = new StringBuilder("P222-EV kind=client-lookup-preflight");
            sb.append(" client_dbid_hex=").append(clientDbidHex);
            sb.append(" target_hash_hex=").append(result.targetHashHex);
            sb.append(" routing_key_hex=").append(result.routingKeyHex);
            sb.append(" routing_key_differs=").append(result.routingKeyDiffers);
            sb.append(" b_hash_hex=").append(bHex);
            sb.append(" observable=true");
            sb.append(" client_db_resolved=").append(result.clientDbResolved);
            sb.append(" client_db_is_client=").append(result.clientDbIsClient);
            sb.append(" target_ls_present_before_send=").append(result.targetLsPresentBeforeSend);
            sb.append(" target_ls_type=").append(
                result.targetLsType == null ? "none" : result.targetLsType);
            sb.append(" facade_floodfill_enabled=").append(result.facadeFloodfillEnabled);
            sb.append(" router_uptime_ms=").append(result.routerUptimeMs);
            sb.append(" netdb_search_limit_effective=").append(result.netdbSearchLimitEffective);
            sb.append(" selector_extra_peers=").append(result.selectorExtraPeers);
            sb.append(" selector_width=").append(result.selectorWidth);
            sb.append(" selector_input_kbucket_size=").append(result.kbucketSize);
            sb.append(" selector_count=").append(result.selected.size());
            sb.append(" selector_contains_b=").append(result.containsB);
            sb.append(" selector_empty=").append(result.selected.isEmpty());
            int shown = 0;
            for (Hash h : result.selected) {
                if (shown >= 8) break;
                sb.append(" peer_").append(shown).append("_hex=")
                    .append(p220HexLower(h.getData()));
                shown++;
            }
            return sb.toString();
        }

        private String sha256Hex(byte[] bytes) {
            try {
                byte[] sum = MessageDigest.getInstance("SHA-256").digest(bytes);
                StringBuilder hex = new StringBuilder(sum.length * 2);
                for (byte b : sum) {
                    hex.append(String.format("%02x", b & 0xff));
                }
                return hex.toString();
            } catch (Exception e) {
                return "0".repeat(64);
            }
        }

        private int countSsu2Addresses(RouterInfo info) {
            int total = 0;
            // RouterInfo does not expose a public address iterator
            // we can rely on across minor versions; the
            // SSU2-published helper relies on its outer
            // capability string (the controlled-launcher sets
            // `i2np.udp.enable=true`). Treat `f`/`L`-bearing
            // capabilities as a coarse proxy: 1 means at least
            // one SSU2 address, 0 means none.
            String caps = info.getCapabilities();
            return (caps != null && (caps.indexOf('4') >= 0 || caps.indexOf('5') >= 0)) ? 1 : 0;
        }
    }
}
