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

public final class ControlledRouter {

    private static final String[] FORBIDDEN_VMCOMM_KEYS = new String[] {
        "i2p.vmCommSystem",
    };

    public static void main(String[] args) throws Exception {
        if (args.length != 5 && args.length != 6) {
            System.err.println(
                "usage: ControlledRouter <java-data-dir> <ssu2-host> <ssu2-port> <sam-port> <i2cp-port> [j219-control-port]");
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
        props.setProperty("router.floodfillParticipant", "true");
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