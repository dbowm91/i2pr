// Plan 196 — M6 Java I2P controlled first-run topology corrective.
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
//   <java-data-dir>   -- disposable per-run I2P config + log + pid dir
//   <ssu2-host>       -- UDP transport bind host (loopback only)
//   <ssu2-port>       -- UDP transport bind port (loopback only)
//   <sam-port>        -- SAM bridge TCP bind port (loopback only)
//   <i2cp-port>       -- I2CP server TCP bind port (loopback only)
//
// Environment contract:
//   - VMCommSystem is never enabled (no `i2p.vmCommSystem=true`);
//   - public reseed URLs are never configured;
//   - the SAM bridge is the only client app started on load;
//   - all listeners bind to loopback only.

import java.io.File;
import java.io.FileWriter;
import java.io.IOException;
import java.util.Properties;

import net.i2p.data.DataHelper;
import net.i2p.router.Router;

public final class ControlledRouter {

    private static final String[] FORBIDDEN_VMCOMM_KEYS = new String[] {
        "i2p.vmCommSystem",
    };

    public static void main(String[] args) throws Exception {
        if (args.length != 5) {
            System.err.println(
                "usage: ControlledRouter <java-data-dir> <ssu2-host> <ssu2-port> <sam-port> <i2cp-port>");
            System.exit(64);
        }

        final String javaData = args[0];
        final String ssu2Host = args[1];
        final String ssu2Port = args[2];
        final String samPort = args[3];
        final String i2cpPort = args[4];

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
        props.setProperty("router.networkDatabase.dbDir", netDbDir.getAbsolutePath() + "/");
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

        System.out.println("ControlledRouter: starting router with ssu2="
            + ssu2Host + ":" + ssu2Port
            + " i2cp=127.0.0.1:" + i2cpPort
            + " sam=127.0.0.1:" + samPort
            + " datadir=" + dataDir.getAbsolutePath());

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
}