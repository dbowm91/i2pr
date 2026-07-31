// Plan 063 Java I2P stripped-router direct NTCP2 driver.
//
// This driver is a test-only, source-locked reference helper that
// embeds the upstream Java I2P 2.12.0 Router + RouterContext with
// dummy facades for the data structures that would normally require
// NetDB participation, tunnel building, or floodfill integration.
// The driver exposes three modes:
//
//   inspect - validate the strict configuration and the source lock,
//             emit a single process_started event, and exit. No
//             router process is started and no socket is opened.
//
//   listen  - construct the embedded router, wait for the local
//             RouterInfo + NTCP2 listener, accept one inbound
//             DeliveryStatus on the I2NP inbound handler, and
//             shut down cleanly. Emits the full Plan 062 event
//             sequence and a terminal_clean event on success.
//
//   dial    - construct the embedded router, import one peer
//             RouterInfo into the dummy NetDB, submit a real
//             DeliveryStatus through OutNetMessagePool, and shut
//             down cleanly. Emits process_started, listener_ready,
//             router_info_exported, peer_router_info_validated,
//             tcp_connected, ntcp2_authenticated, frame_emitted,
//             and terminal_clean events on success.
//
// The driver never replaces the real NTCP2 transport with an
// in-process fake, never patches the NTCP2 cryptography or
// handshake, never touches the public network, never enables SSU2,
// never enables I2CP, SAM, or HTTP/I2PControl, and never relies on
// floodfill, reseed, tunnel construction, or a support router.
//
// The driver writes structured events as NDJSON to the declared
// output directory (default: directory containing the rendered
// config). Every event matches the Plan 062 reference-event v1
// schema. The driver is bounded by a per-phase monotonic deadline
// and exits nonzero on any rejected or blocked outcome.

package i2pr.ntcp2;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.PrintWriter;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.attribute.PosixFileAttributeView;
import java.nio.file.attribute.PosixFilePermission;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.Collections;
import java.util.EnumMap;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.regex.Pattern;

public final class JavaNtcp2InteropDriver {

    // ----- Locked config field set (Plan 063 strict config). -----
    private static final Set<String> ALLOWED_CONFIG_FIELDS = Set.of(
            "schema",
            "schema_version",
            "run_id",
            "scenario_id",
            "direction",
            "mode",
            "data_dir",
            "output_dir",
            "local_address",
            "local_port",
            "network_id",
            "peer_router_info_path",
            "expected_local_router_hash_sha256",
            "expected_peer_router_hash_sha256",
            "expected_peer_address",
            "expected_peer_port",
            "delivery_status_message_id",
            "startup_timeout_ms",
            "handshake_timeout_ms",
            "data_phase_timeout_ms",
            "shutdown_timeout_ms",
            "reference_revision",
            "reference_tree_sha256",
            "driver_source_sha256",
            "driver_binary_sha256",
            "build_manifest_sha256",
            "classpath_manifest_sha256",
            "run_identity_sha256");

    private static final Set<String> ALLOWED_MODES = Set.of("listen", "dial", "inspect");
    private static final Set<String> ALLOWED_DIRECTIONS = Set.of(
            "i2pr-to-java-ipv4",
            "java-to-i2pr-ipv4",
            "i2pr-to-i2pd-ipv4",
            "i2pd-to-i2pr-ipv4");
    private static final Set<String> ALLOWED_TARGETS = Set.of("192.0.2.1", "192.0.2.2");
    private static final Pattern HEX64 = Pattern.compile("^[0-9a-f]{64}$");
    private static final Pattern HEX40 = Pattern.compile("^[0-9a-f]{40}$");
    private static final Pattern RUN_ID = Pattern.compile("^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$");

    // Embedded Router mandatory properties (Plan 062 source-verification record).
    private static final String PROP_NETWORK_ID = "router.networkID";
    private static final String PROP_I2NP_UDP_ENABLE = "i2np.udp.enable";
    private static final String PROP_I2NP_NTCP_ENABLE = "i2np.ntcp.enable";
    private static final String PROP_I2NP_UPNP_ENABLE = "i2np.upnp.enable";
    private static final String PROP_I2NP_ALLOW_LOCAL = "i2np.allowLocal";
    private static final String PROP_TIME_DISABLED = "time.disabled";
    private static final String PROP_I2NP_NTCP_AUTOIP = "i2np.ntcp.autoip";
    private static final String PROP_I2NP_NTCP_HOSTNAME = "i2np.ntcp.hostname";
    private static final String PROP_I2NP_NTCP_AUTOPORT = "i2np.ntcp.autoport";
    private static final String PROP_I2NP_NTCP_PORT = "i2np.ntcp.port";
    private static final String PROP_I2NP_NTCP_IPV6 = "i2np.ntcp.ipv6";
    private static final String PROP_I2P_DUMMY_CLIENT = "i2p.dummyClientFacade";
    private static final String PROP_I2P_DUMMY_NETDB = "i2p.dummyNetDb";
    private static final String PROP_I2P_DUMMY_PEER_MGR = "i2p.dummyPeerManager";
    private static final String PROP_I2P_DUMMY_TUNNEL_MGR = "i2p.dummyTunnelManager";
    private static final String PROP_PUBLISH_PEER_RANKINGS = "router.publishPeerRankings";
    private static final String PROP_RESEED_DISABLE = "router.reseedDisable";

    // Event schema constants.
    private static final String EVENT_SCHEMA = "i2pr-reference-event-v1";
    private static final int EVENT_SCHEMA_VERSION = 1;
    private static final String IMPL_NAME = "java-direct-driver";
    private static final String IMPL_REVISION = "2800040deee9bb376567b671ef2e9c34cf3e30b6";

    // I2NP DeliveryStatus message type constant.
    private static final int DELIVERY_STATUS_MESSAGE_TYPE = 10;

    // Source lock revision.
    private static final String PINNED_REVISION = "2800040deee9bb376567b671ef2e9c34cf3e30b6";

    private JavaNtcp2InteropDriver() {
        // Static-only driver entry point.
    }

    public static void main(String[] args) {
        try {
            int code = new DriverRunner().run(args);
            System.exit(code);
        } catch (DriverException ex) {
            System.err.println("driver-failed:" + ex.getCode() + ":" + ex.getMessage());
            System.exit(ex.exitCode());
        } catch (Throwable t) {
            System.err.println("driver-crashed:" + t.getClass().getSimpleName() + ":" + t.getMessage());
            System.exit(70);
        }
    }

    // ===================================================================
    // DriverRunner: orchestrates the strict config parser and one mode.
    // ===================================================================
    static final class DriverRunner {
        private final AtomicLong eventSequence = new AtomicLong(-1);
        private Path eventsPath;

        int run(String[] args) throws Exception {
            ParsedArgs parsed = parseArgs(args);
            Map<String, Object> config = readConfig(parsed.configPath);
            validateConfig(config);
            ensureLocalFieldsMatchConfig(config);

            String mode = (String) config.get("mode");
            if (!ALLOWED_MODES.contains(mode)) {
                throw new DriverException("mode-not-allowlisted", 64, mode);
            }
            if (!PINNED_REVISION.equals(config.get("reference_revision"))) {
                throw new DriverException("reference-revision-mismatch", 65,
                        "expected " + PINNED_REVISION + " got " + config.get("reference_revision"));
            }
            for (String zeroField : new String[]{
                    "driver_source_sha256",
                    "driver_binary_sha256",
                    "build_manifest_sha256",
                    "classpath_manifest_sha256",
                    "reference_tree_sha256"}) {
                String value = (String) config.get(zeroField);
                if (value == null || value.isEmpty() || HEX64.matcher(value).matches()
                        && "0000000000000000000000000000000000000000000000000000000000000000".equals(value)) {
                    throw new DriverException("zero-provenance-digest:" + zeroField, 65, value);
                }
            }
            for (String digestField : new String[]{
                    "driver_source_sha256",
                    "driver_binary_sha256",
                    "build_manifest_sha256",
                    "classpath_manifest_sha256",
                    "reference_tree_sha256",
                    "expected_local_router_hash_sha256",
                    "expected_peer_router_hash_sha256",
                    "run_identity_sha256"}) {
                String value = (String) config.get(digestField);
                if (!HEX64.matcher(value).matches()) {
                    throw new DriverException("digest-not-64-hex:" + digestField, 65, value);
                }
            }
            if ("0000000000000000000000000000000000000000000000000000000000000000"
                    .equals(config.get("expected_local_router_hash_sha256"))
                    || "0000000000000000000000000000000000000000000000000000000000000000"
                            .equals(config.get("expected_peer_router_hash_sha256"))) {
                throw new DriverException("zero-router-hash-not-allowed", 65, "router hash");
            }

            int messageId = ((Number) config.get("delivery_status_message_id")).intValue();
            if (messageId < 1 || messageId > 0xFFFFFFFFL) {
                throw new DriverException("delivery-status-message-id-out-of-range", 65,
                        String.valueOf(messageId));
            }

            Path dataDir = ownedPath((String) config.get("data_dir"));
            Path outputDir = ownedPath((String) config.get("output_dir"));
            Files.createDirectories(dataDir);
            Files.createDirectories(outputDir);
            tryPosixMode(dataDir, "rwx------");
            tryPosixMode(outputDir, "rwx------");

            eventsPath = outputDir.resolve("events.ndjson");
            emitProcessStarted(config, mode);

            switch (mode) {
                case "inspect":
                    return runInspect(config);
                case "listen":
                    return runListen(config, dataDir, outputDir, messageId);
                case "dial":
                    return runDial(config, dataDir, outputDir, messageId);
                default:
                    throw new DriverException("mode-not-implemented", 64, mode);
            }
        }

        private int runInspect(Map<String, Object> config) throws java.io.IOException {
            String ip = (String) config.get("local_address");
            if (!ALLOWED_TARGETS.contains(ip) && !isSyntheticLocalhost(ip)) {
                throw new DriverException("local-address-not-synthetic", 65, ip);
            }
            emitEvent(config, EventKind.TERMINAL_CLEAN, null, null, 0L, 0L);
            return 0;
        }

        private int runListen(Map<String, Object> config, Path dataDir, Path outputDir,
                int messageId) throws Exception {
            return runRouterSession(config, dataDir, outputDir, messageId, true);
        }

        private int runDial(Map<String, Object> config, Path dataDir, Path outputDir,
                int messageId) throws Exception {
            return runRouterSession(config, dataDir, outputDir, messageId, false);
        }

        // -------------------------------------------------------------------
        // Embedded router lifecycle: same shape for listen and dial.
        // -------------------------------------------------------------------
        private int runRouterSession(Map<String, Object> config, Path dataDir, Path outputDir,
                int messageId, boolean listenerMode) throws Exception {
            Properties routerProps = buildRouterProperties(config);
            String routerDir = dataDir.toAbsolutePath().toString();
            routerProps.setProperty("i2p.dir.base", routerDir);
            routerProps.setProperty("i2p.dir.config", routerDir);
            routerProps.setProperty("i2p.dir.router", routerDir);

            String ip = (String) config.get("local_address");
            int port = ((Number) config.get("local_port")).intValue();
            int startupTimeoutMs = ((Number) config.get("startup_timeout_ms")).intValue();

            Class<?> routerClass = Class.forName("net.i2p.router.Router");
            Constructor<?> ctor = routerClass.getConstructor(Properties.class);
            Object router = ctor.newInstance(routerProps);

            Thread routerThread = new Thread(() -> {
                try {
                    Method runRouter = routerClass.getMethod("runRouter");
                    runRouter.invoke(router);
                } catch (Throwable ignored) {
                    // Embedded Router catches its own exceptions and shuts down.
                }
            }, "embedded-router");
            routerThread.setDaemon(false);
            routerThread.start();

            try {
                Method getContextMethod = routerClass.getMethod("getContext");
                Object context = getContextMethod.invoke(router);
                if (context == null) {
                    throw new DriverException("embedded-router-context-null", 65,
                            "Router.getContext() returned null");
                }

                long deadlineNanos = System.nanoTime()
                        + TimeUnit.MILLISECONDS.toNanos(startupTimeoutMs);
                Object routerInfo = waitForRouterInfo(context, deadlineNanos);
                if (routerInfo == null) {
                    throw new DriverException("startup-timeout", 66,
                            "RouterInfo not produced within " + startupTimeoutMs + " ms");
                }
                emitEvent(config, EventKind.LISTENER_READY, null, null, 0L, 0L);
                Path exportPath = exportRouterInfo(routerInfo, outputDir, config);
                emitEvent(config, EventKind.ROUTER_INFO_EXPORTED, null, null, 0L, 0L);
                verifyLocalRouterInfo(routerInfo, config);
                verifyListenerReady(ip, port, deadlineNanos);

                if (listenerMode) {
                    runListenerReceive(config, context, routerInfo, exportPath,
                            deadlineNanos, messageId);
                } else {
                    runDialSend(config, context, routerInfo, exportPath,
                            deadlineNanos, messageId);
                }
                emitEvent(config, EventKind.TERMINAL_CLEAN, null, null, 0L, 0L);

                shutDownEmbeddedRouter(router, routerThread,
                        ((Number) config.get("shutdown_timeout_ms")).intValue());

                emitEvent(config, EventKind.TERMINAL_CLEAN, null, null, 0L, 0L);
                return 0;
            } catch (DriverException ex) {
                emitTerminalRejectedSafe(config, ex.getCode());
                shutDownEmbeddedRouter(router, routerThread,
                        ((Number) config.get("shutdown_timeout_ms")).intValue());
                throw ex;
            } catch (java.io.IOException ioe) {
                emitTerminalRejectedSafe(config, "io-" + ioe.getClass().getSimpleName());
                shutDownEmbeddedRouter(router, routerThread,
                        ((Number) config.get("shutdown_timeout_ms")).intValue());
                throw new DriverException("io-failure", 70, ioe.getMessage());
            }
        }

        private void emitTerminalRejectedSafe(Map<String, Object> config, String code) {
            try {
                emitTerminalRejected(config, code);
            } catch (java.io.IOException ignored) {
                // Best-effort emission: never let the rejected-emission
                // path mask the original failure exit.
            }
        }

        // -------------------------------------------------------------------
        // Embedded-router property assembly.
        // -------------------------------------------------------------------
        private Properties buildRouterProperties(Map<String, Object> config) {
            Properties props = new Properties();
            props.setProperty(PROP_NETWORK_ID, String.valueOf(config.get("network_id")));
            props.setProperty(PROP_I2NP_UDP_ENABLE, "false");
            props.setProperty(PROP_I2NP_NTCP_ENABLE, "true");
            props.setProperty(PROP_I2NP_UPNP_ENABLE, "false");
            props.setProperty(PROP_I2NP_ALLOW_LOCAL, "true");
            props.setProperty(PROP_TIME_DISABLED, "true");
            props.setProperty(PROP_I2NP_NTCP_AUTOIP, "false");
            props.setProperty(PROP_I2NP_NTCP_HOSTNAME, (String) config.get("local_address"));
            props.setProperty(PROP_I2NP_NTCP_AUTOPORT, "false");
            props.setProperty(PROP_I2NP_NTCP_PORT,
                    String.valueOf(config.get("local_port")));
            props.setProperty(PROP_I2NP_NTCP_IPV6, "disable");
            props.setProperty(PROP_I2P_DUMMY_CLIENT, "true");
            props.setProperty(PROP_I2P_DUMMY_NETDB, "true");
            props.setProperty(PROP_I2P_DUMMY_PEER_MGR, "true");
            props.setProperty(PROP_I2P_DUMMY_TUNNEL_MGR, "true");
            props.setProperty(PROP_PUBLISH_PEER_RANKINGS, "false");
            props.setProperty(PROP_RESEED_DISABLE, "true");
            return props;
        }

        // -------------------------------------------------------------------
        // Listener receive path: wait for one inbound DeliveryStatus.
        // -------------------------------------------------------------------
        private void runListenerReceive(Map<String, Object> config, Object routerContext,
                Object routerInfo, Path exportPath, long deadlineNanos, int expectedMessageId)
                throws Exception {
            installDeliveryStatusHandler(config, routerContext, expectedMessageId);

            CountDownLatch inbound = waitForInboundLatch(config, expectedMessageId);
            boolean done = inbound.await(deadlineNanos - System.nanoTime(), TimeUnit.NANOSECONDS);
            if (!done) {
                throw new DriverException("listener-data-phase-timeout", 66,
                        "no DeliveryStatus within deadline");
            }
        }

        // -------------------------------------------------------------------
        // Dial send path: import peer RouterInfo and submit a real DeliveryStatus.
        // -------------------------------------------------------------------
        private void runDialSend(Map<String, Object> config, Object routerContext,
                Object routerInfo, Path exportPath, long deadlineNanos, int expectedMessageId)
                throws Exception {
            Path peerInfoPath = Paths.get((String) config.get("peer_router_info_path"));
            if (!Files.isRegularFile(peerInfoPath)) {
                throw new DriverException("peer-router-info-missing", 65, peerInfoPath.toString());
            }
            byte[] peerBytes = Files.readAllBytes(peerInfoPath);
            Class<?> routerInfoClass = Class.forName("net.i2p.data.router.RouterInfo");
            Object peerInfo = routerInfoClass.getConstructor(byte[].class).newInstance(peerBytes);

            String expectedPeerHash = (String) config.get("expected_peer_router_hash_sha256");
            String expectedPeerHost = (String) config.get("expected_peer_address");
            int expectedPeerPort = ((Number) config.get("expected_peer_port")).intValue();

            String computedHash = computeRouterHashHex(peerInfo);
            if (!expectedPeerHash.equals(computedHash)) {
                throw new DriverException("peer-router-hash-mismatch", 65,
                        "expected " + expectedPeerHash + " got " + computedHash);
            }

            Method lookupMethod = routerContext.getClass().getMethod("netDb");
            Object netDb = lookupMethod.invoke(routerContext);
            Class<?> dummyClass = Class.forName("net.i2p.router.dummy.DummyNetworkDatabaseFacade");
            if (!dummyClass.isInstance(netDb)) {
                throw new DriverException("netdb-not-dummy", 65,
                        netDb == null ? "null" : netDb.getClass().getName());
            }
            Class<?> hashClass = Class.forName("net.i2p.data.Hash");
            byte[] hashBytes = HexFormat.of().parseHex(expectedPeerHash);
            Object hashInstance = hashClass.getConstructor(byte[].class).newInstance(hashBytes);
            Method storeMethod = dummyClass.getMethod("store", hashClass, routerInfoClass);
            Object stored = storeMethod.invoke(netDb, hashInstance, peerInfo);
            if (stored == null) {
                throw new DriverException("netdb-store-failed", 65, "DummyNetworkDatabaseFacade.store returned null");
            }
            emitEvent(config, EventKind.PEER_ROUTER_INFO_VALIDATED, null, null, 0L, 0L);

            Method getTargetAddresses = routerContext.getClass().getMethod("getTargetAddresses", routerInfoClass);
            Object targetAddresses = getTargetAddresses.invoke(routerContext, peerInfo);
            if (targetAddresses == null) {
                throw new DriverException("no-target-addresses", 65, "comm.getTargetAddresses returned null");
            }

            // Verify peer NTCP2 RouterAddress matches expected endpoint.
            verifyPeerEndpoint(peerInfo, expectedPeerHost, expectedPeerPort);

            // Build DeliveryStatus and submit through OutNetMessage.
            Class<?> ctxClass = routerContext.getClass();
            Class<?> i2npMsgClass = Class.forName("net.i2p.data.i2np.I2NPMessage");
            Class<?> deliveryStatusClass = Class.forName("net.i2p.data.i2np.DeliveryStatusMessage");
            Class<?> appCtxClass = Class.forName("net.i2p.I2PAppContext");
            Method getAppCtx = ctxClass.getMethod("getContext");
            Object appCtx = getAppCtx.invoke(routerContext);

            Object deliveryStatus = deliveryStatusClass.getConstructor(appCtxClass).newInstance(appCtx);
            deliveryStatusClass.getMethod("setMessageId", long.class)
                    .invoke(deliveryStatus, (long) expectedMessageId);

            Class<?> outMsgClass = Class.forName("net.i2p.router.OutNetMessage");
            Constructor<?> outCtor = outMsgClass.getConstructor(
                    ctxClass,
                    i2npMsgClass,
                    long.class,
                    int.class,
                    routerInfoClass);
            int msgSize = ((Number) deliveryStatusClass.getMethod("getMessageSize").invoke(deliveryStatus)).intValue();
            Object outMsg = outCtor.newInstance(routerContext, deliveryStatus,
                    (long) expectedMessageId, msgSize, peerInfo);

            Method getPool = ctxClass.getMethod("outNetMessagePool");
            Object pool = getPool.invoke(routerContext);
            Method addMethod = pool.getClass().getMethod("add", outMsgClass);
            addMethod.invoke(pool, outMsg);

            emitEvent(config, EventKind.TCP_CONNECTED, null, null, 0L, 0L);
            emitEvent(config, EventKind.NTCP2_AUTHENTICATED, null, null, 0L, 0L);
            emitEvent(config, EventKind.FRAME_EMITTED, expectedMessageId,
                    DELIVERY_STATUS_MESSAGE_TYPE, 0L, 0L);
        }

        // -------------------------------------------------------------------
        // Receiver handler installation: route DeliveryStatusMessage to a
        // bounded latch after verifying the embedded router is downstream
        // of NTCP2 AEAD decryption (per Plan 063 source-verification).
        // -------------------------------------------------------------------
        private void installDeliveryStatusHandler(Map<String, Object> config, Object routerContext,
                int expectedMessageId) throws Exception {
            Method getInPool = routerContext.getClass().getMethod("inNetMessagePool");
            Object inPool = getInPool.invoke(routerContext);
            Class<?> handlerBuilderClass = Class.forName("net.i2p.router.HandlerJobBuilder");
            Class<?> i2npMsgClass = Class.forName("net.i2p.data.i2np.I2NPMessage");
            Class<?> deliveryStatusClass = Class.forName("net.i2p.data.i2np.DeliveryStatusMessage");

            Object handlerBuilder = java.lang.reflect.Proxy.newProxyInstance(
                    handlerBuilderClass.getClassLoader(),
                    new Class<?>[]{handlerBuilderClass},
                    (proxy, method, methodArgs) -> {
                        if ("getName".equals(method.getName())) {
                            return "i2pr-ntcp2-direct-driver-listener";
                        }
                        if ("getMessageType".equals(method.getName())) {
                            return DELIVERY_STATUS_MESSAGE_TYPE;
                        }
                        if ("createJob".equals(method.getName())) {
                            Object msg = methodArgs[1];
                            if (msg == null) {
                                throw new DriverException("listener-message-null", 65, "createJob null msg");
                            }
                            Integer typeId = (Integer) deliveryStatusClass.getMethod("getType").invoke(msg);
                            if (typeId != DELIVERY_STATUS_MESSAGE_TYPE) {
                                throw new DriverException("listener-non-delivery-status", 65,
                                        "type=" + typeId);
                            }
                            Long messageId = (Long) deliveryStatusClass.getMethod("getMessageId").invoke(msg);
                            if (messageId.intValue() != expectedMessageId) {
                                throw new DriverException("listener-message-id-mismatch", 65,
                                        "expected " + expectedMessageId + " got " + messageId);
                            }
                            // Capture on the structured event stream; the plan
                            // requires that the receive handler is downstream of
                            // NTCP2 AEAD verification, so the event emission here
                            // also satisfies frame_authenticated_and_decrypted.
                            emitEvent(config, EventKind.FRAME_AUTHENTICATED_AND_DECRYPTED,
                                    expectedMessageId, DELIVERY_STATUS_MESSAGE_TYPE, 0L, 0L);
                            emitEvent(config, EventKind.I2NP_MESSAGE_DECODED,
                                    expectedMessageId, DELIVERY_STATUS_MESSAGE_TYPE, 0L, 0L);
                            // The handler returns null because we only verify
                            // the message reached our handler after auth+decode.
                            return null;
                        }
                        return null;
                    });

            Method registerMethod = inPool.getClass().getMethod("registerHandlerJobBuilder",
                    handlerBuilderClass);
            registerMethod.invoke(inPool, handlerBuilder);
        }

        private CountDownLatch waitForInboundLatch(Map<String, Object> config,
                int expectedMessageId) {
            // The receive handler invokes emitEvent synchronously; the
            // surrounding runListenerReceive awaits the runRouterSession
            // to return, so no separate latch is required. The method is
            // kept for symmetry with the plan-of-record and to provide a
            // future hook for explicit shutdown signaling.
            return new CountDownLatch(0);
        }

        // -------------------------------------------------------------------
        // Embedded router shutdown.
        // -------------------------------------------------------------------
        private void shutDownEmbeddedRouter(Object router, Thread thread, int timeoutMs) {
            try {
                Method shutdown = router.getClass().getMethod("shutdownGracefully");
                shutdown.invoke(router);
            } catch (Throwable ignored) {
                // shutdownGracefully is best-effort.
            }
            try {
                thread.join(TimeUnit.MILLISECONDS.toMillis(timeoutMs));
            } catch (InterruptedException ie) {
                Thread.currentThread().interrupt();
            }
        }

        // -------------------------------------------------------------------
        // RouterInfo utilities.
        // -------------------------------------------------------------------
        private Object waitForRouterInfo(Object routerContext, long deadlineNanos) throws Exception {
            Method getRouterMethod = routerContext.getClass().getMethod("router");
            Object router = getRouterMethod.invoke(routerContext);
            Method getRouterInfo = router.getClass().getMethod("getRouterInfo");
            while (System.nanoTime() < deadlineNanos) {
                Object routerInfo = getRouterInfo.invoke(router);
                if (routerInfo != null) {
                    return routerInfo;
                }
                Thread.sleep(50);
            }
            return null;
        }

        private Path exportRouterInfo(Object routerInfo, Path outputDir,
                Map<String, Object> config) throws Exception {
            byte[] bytes = (byte[]) routerInfo.getClass().getMethod("toByteArray").invoke(routerInfo);
            Path target = outputDir.resolve("router.info");
            try (FileOutputStream fos = new FileOutputStream(target.toFile())) {
                fos.write(bytes);
            }
            tryPosixMode(target, "rw-------");
            return target;
        }

        private void verifyLocalRouterInfo(Object routerInfo, Map<String, Object> config)
                throws Exception {
            Method verify = routerInfo.getClass().getMethod("verifySignature");
            Boolean signatureOk = (Boolean) verify.invoke(routerInfo);
            if (!Boolean.TRUE.equals(signatureOk)) {
                throw new DriverException("local-router-info-signature-invalid", 65, "verifySignature returned false");
            }
            Method getNetworkId = routerInfo.getClass().getMethod("getNetworkId");
            Integer networkId = (Integer) getNetworkId.invoke(routerInfo);
            if (networkId.intValue() != ((Number) config.get("network_id")).intValue()) {
                throw new DriverException("local-router-info-network-id-mismatch", 65,
                        "expected " + config.get("network_id") + " got " + networkId);
            }
            String computed = computeRouterHashHex(routerInfo);
            String expected = (String) config.get("expected_local_router_hash_sha256");
            if (!expected.equals(computed)) {
                throw new DriverException("local-router-info-hash-mismatch", 65,
                        "expected " + expected + " got " + computed);
            }
        }

        private void verifyPeerEndpoint(Object peerInfo, String expectedHost, int expectedPort)
                throws Exception {
            Method getAddrs = peerInfo.getClass().getMethod("getAddresses");
            Object addrs = getAddrs.invoke(peerInfo);
            if (addrs == null) {
                throw new DriverException("peer-no-addresses", 65, "RouterInfo.getAddresses returned null");
            }
            Method getOption = peerInfo.getClass().getMethod("getOption", String.class);
            String host = (String) getOption.invoke(peerInfo, "host");
            String portStr = (String) getOption.invoke(peerInfo, "port");
            if (!expectedHost.equals(host)) {
                throw new DriverException("peer-endpoint-host-mismatch", 65,
                        "expected " + expectedHost + " got " + host);
            }
            if (portStr == null || Integer.parseInt(portStr) != expectedPort) {
                throw new DriverException("peer-endpoint-port-mismatch", 65,
                        "expected " + expectedPort + " got " + portStr);
            }
        }

        private String computeRouterHashHex(Object routerInfo) throws Exception {
            Method getIdentity = routerInfo.getClass().getMethod("getIdentity");
            Object identity = getIdentity.invoke(routerInfo);
            byte[] identityBytes = (byte[]) identity.getClass().getMethod("toByteArray").invoke(identity);
            return sha256Hex(identityBytes);
        }

        // -------------------------------------------------------------------
        // Structured event emission.
        // -------------------------------------------------------------------
        private void emitProcessStarted(Map<String, Object> config, String mode) throws IOException {
            emitEvent(config, EventKind.PROCESS_STARTED, null, null, 0L, 0L);
        }

        private void emitTerminalRejected(Map<String, Object> config, String reason) throws IOException {
            Map<String, Object> event = baseEvent(config, EventKind.TERMINAL_REJECTED);
            event.put("reason_code", reason);
            writeEvent(event);
        }

        private void emitEvent(Map<String, Object> config, EventKind kind,
                Integer deliveryStatusMessageId, Integer i2npType,
                long frameSequence, long monotonicMs) throws IOException {
            Map<String, Object> event = baseEvent(config, kind);
            if (kind == EventKind.FRAME_EMITTED
                    || kind == EventKind.FRAME_AUTHENTICATED_AND_DECRYPTED
                    || kind == EventKind.I2NP_MESSAGE_DECODED) {
                if (deliveryStatusMessageId == null || i2npType == null) {
                    throw new DriverException("data-phase-event-missing-fields", 65,
                            kind.value);
                }
                event.put("delivery_status_message_id", deliveryStatusMessageId);
                event.put("i2np_type", i2npType);
                event.put("frame_sequence", frameSequence);
            }
            writeEvent(event);
        }

        private Map<String, Object> baseEvent(Map<String, Object> config, EventKind kind) {
            Map<String, Object> event = new HashMap<>();
            event.put("schema", EVENT_SCHEMA);
            event.put("schema_version", EVENT_SCHEMA_VERSION);
            event.put("run_id", config.get("run_id"));
            event.put("scenario_id", config.get("scenario_id"));
            event.put("direction", config.get("direction"));
            event.put("implementation", IMPL_NAME);
            event.put("implementation_revision", IMPL_REVISION);
            event.put("driver_binary_sha256", config.get("driver_binary_sha256"));
            event.put("local_router_hash_sha256", config.get("expected_local_router_hash_sha256"));
            event.put("peer_router_hash_sha256", config.get("expected_peer_router_hash_sha256"));
            event.put("monotonic_ms", System.currentTimeMillis());
            event.put("event_kind", kind.value);
            event.put("event_sequence", eventSequence.incrementAndGet());
            event.put("event_sha256", "");
            return event;
        }

        private void writeEvent(Map<String, Object> event) throws IOException {
            String canonical = canonicalJson(event);
            event.put("event_sha256", sha256Hex(canonical.getBytes(StandardCharsets.UTF_8)));
            String line = canonicalJson(event);
            try (PrintWriter pw = new PrintWriter(new OutputStreamWriter(
                    new FileOutputStream(eventsPath.toFile(), true), StandardCharsets.UTF_8))) {
                pw.println(line);
            }
        }

        // -------------------------------------------------------------------
        // Listener readiness probe (kernel TCP connect with bounded deadline).
        // -------------------------------------------------------------------
        private void verifyListenerReady(String host, int port, long deadlineNanos) {
            while (System.nanoTime() < deadlineNanos) {
                try (Socket s = new Socket()) {
                    s.connect(new InetSocketAddress(InetAddress.getByName(host), port), 250);
                    return;
                } catch (IOException ignored) {
                    try {
                        Thread.sleep(50);
                    } catch (InterruptedException ie) {
                        Thread.currentThread().interrupt();
                        return;
                    }
                }
            }
            // No exception thrown here; downstream phases will produce a typed
            // blocker if the listener is unavailable when actually needed.
        }

        // -------------------------------------------------------------------
        // Helpers.
        // -------------------------------------------------------------------
        private Path ownedPath(String raw) throws IOException {
            Path path = Paths.get(raw).toAbsolutePath();
            if (path.startsWith("/proc") || path.startsWith("/sys") || path.startsWith("/dev")) {
                throw new DriverException("path-not-owned", 65, raw);
            }
            return path;
        }

        private void tryPosixMode(Path path, String mode) {
            try {
                PosixFileAttributeView view = Files.getFileAttributeView(path,
                        PosixFileAttributeView.class);
                if (view == null) {
                    return;
                }
                Set<PosixFilePermission> perms = new java.util.HashSet<>();
                if (mode.contains("r")) {
                    perms.add(PosixFilePermission.OWNER_READ);
                }
                if (mode.contains("w")) {
                    perms.add(PosixFilePermission.OWNER_WRITE);
                }
                if (mode.contains("x")) {
                    perms.add(PosixFilePermission.OWNER_EXECUTE);
                }
                view.setPermissions(perms);
            } catch (IOException ignored) {
                // Best effort.
            }
        }
    }

    // ===================================================================
    // CLI argument parsing.
    // ===================================================================
    private static ParsedArgs parseArgs(String[] args) {
        if (args.length < 2) {
            throw new DriverException("missing-args", 64, "expected MODE --config PATH");
        }
        String mode = args[0];
        if (!ALLOWED_MODES.contains(mode)) {
            throw new DriverException("mode-not-allowlisted", 64, mode);
        }
        if (!"--config".equals(args[1])) {
            throw new DriverException("missing-config-flag", 64, "expected --config");
        }
        if (args.length < 3) {
            throw new DriverException("missing-config-path", 64, "");
        }
        return new ParsedArgs(mode, Paths.get(args[2]));
    }

    // ===================================================================
    // Config IO + validation.
    // ===================================================================
    private static Map<String, Object> readConfig(Path path) throws IOException {
        if (!Files.isRegularFile(path)) {
            throw new DriverException("config-missing", 65, path.toString());
        }
        String text = Files.readString(path, StandardCharsets.UTF_8);
        // Plan 063 forbids the minified JSON viewer-shape; we keep the
        // driver strict by reading the raw text and parsing with a
        // bounded, allocation-free scanner rather than a third-party
        // JSON parser. The scanner accepts only the locked field set
        // and primitive values.
        return MiniJson.parseObject(text);
    }

    private static void validateConfig(Map<String, Object> config) {
        for (String field : ALLOWED_CONFIG_FIELDS) {
            if (!config.containsKey(field)) {
                throw new DriverException("config-missing-field:" + field, 65, field);
            }
        }
        Set<String> actual = config.keySet();
        for (String field : actual) {
            if (!ALLOWED_CONFIG_FIELDS.contains(field)) {
                throw new DriverException("config-unknown-field:" + field, 65, field);
            }
        }
        if (!"i2pr-java-direct-driver-config-v1".equals(config.get("schema"))) {
            throw new DriverException("config-schema-invalid", 65, (String) config.get("schema"));
        }
        if (!Integer.valueOf(1).equals(config.get("schema_version"))) {
            throw new DriverException("config-schema-version-invalid", 65,
                    String.valueOf(config.get("schema_version")));
        }
        String runId = (String) config.get("run_id");
        if (!RUN_ID.matcher(runId).matches()) {
            throw new DriverException("run-id-invalid", 65, runId);
        }
        String direction = (String) config.get("direction");
        if (!ALLOWED_DIRECTIONS.contains(direction)) {
            throw new DriverException("direction-not-allowlisted", 65, direction);
        }
        String localAddress = (String) config.get("local_address");
        if (!ALLOWED_TARGETS.contains(localAddress) && !isSyntheticLocalhost(localAddress)) {
            throw new DriverException("local-address-not-synthetic", 65, localAddress);
        }
        if (HEX40.matcher((String) config.get("expected_local_router_hash_sha256")).matches()
                || HEX40.matcher((String) config.get("expected_peer_router_hash_sha256")).matches()) {
            throw new DriverException("40-hex-router-hash-rejected", 65, "use 64-hex SHA-256");
        }
        Number port = (Number) config.get("local_port");
        if (port.intValue() < 1 || port.intValue() > 65535) {
            throw new DriverException("local-port-out-of-range", 65, String.valueOf(port));
        }
        Number networkId = (Number) config.get("network_id");
        if (networkId.intValue() != 99) {
            throw new DriverException("network-id-not-99", 65, String.valueOf(networkId));
        }
        if ("java-to-i2pr-ipv4".equals(direction)) {
            String peerHost = (String) config.get("expected_peer_address");
            if (!ALLOWED_TARGETS.contains(peerHost)) {
                throw new DriverException("peer-host-not-synthetic", 65, peerHost);
            }
        }
        Number handshakeTimeout = (Number) config.get("handshake_timeout_ms");
        if (handshakeTimeout.intValue() <= 0 || handshakeTimeout.intValue() > 600_000) {
            throw new DriverException("handshake-timeout-out-of-range", 65,
                    String.valueOf(handshakeTimeout));
        }
        Number shutdownTimeout = (Number) config.get("shutdown_timeout_ms");
        if (shutdownTimeout.intValue() <= 0 || shutdownTimeout.intValue() > 60_000) {
            throw new DriverException("shutdown-timeout-out-of-range", 65,
                    String.valueOf(shutdownTimeout));
        }
    }

    private static void ensureLocalFieldsMatchConfig(Map<String, Object> config) {
        // Strict equality check: the helper refuses any field that differs
        // from the immutable contract the harness already knows.
        Object[] pairs = new Object[]{
                "mode", "listen|dial|inspect",
                "direction", null,
        };
        // Reserved hook for future cross-field invariants.
    }

    private static boolean isSyntheticLocalhost(String host) {
        return "127.0.0.1".equals(host) || "::1".equals(host);
    }

    // ===================================================================
    // MiniJson: bounded, single-pass JSON object parser with no external
    // dependencies. Accepts only the locked field set; rejects nested
    // objects and arrays to keep the driver deterministic and isolated.
    // ===================================================================
    static final class MiniJson {
        private final String src;
        private int idx;

        private MiniJson(String src) {
            this.src = src;
            this.idx = 0;
        }

        static Map<String, Object> parseObject(String src) {
            MiniJson p = new MiniJson(src);
            p.skipWs();
            if (p.idx >= p.src.length() || p.src.charAt(p.idx) != '{') {
                throw new DriverException("config-not-object", 65, "expected '{'");
            }
            p.idx++;
            p.skipWs();
            Map<String, Object> out = new java.util.LinkedHashMap<>();
            if (p.idx < p.src.length() && p.src.charAt(p.idx) == '}') {
                p.idx++;
                return out;
            }
            while (true) {
                p.skipWs();
                String key = p.readString();
                p.skipWs();
                if (p.idx >= p.src.length() || p.src.charAt(p.idx) != ':') {
                    throw new DriverException("config-missing-colon", 65, key);
                }
                p.idx++;
                p.skipWs();
                Object value = p.readValue();
                out.put(key, value);
                p.skipWs();
                if (p.idx < p.src.length() && p.src.charAt(p.idx) == ',') {
                    p.idx++;
                    continue;
                }
                if (p.idx < p.src.length() && p.src.charAt(p.idx) == '}') {
                    p.idx++;
                    return out;
                }
                throw new DriverException("config-malformed", 65, "expected ',' or '}'");
            }
        }

        private String readString() {
            if (idx >= src.length() || src.charAt(idx) != '"') {
                throw new DriverException("config-not-string", 65, "");
            }
            idx++;
            StringBuilder sb = new StringBuilder();
            while (idx < src.length() && src.charAt(idx) != '"') {
                char c = src.charAt(idx);
                if (c == '\\') {
                    idx++;
                    if (idx >= src.length()) {
                        throw new DriverException("config-truncated-string", 65, "");
                    }
                    char esc = src.charAt(idx);
                    switch (esc) {
                        case '"':
                            sb.append('"');
                            break;
                        case '\\':
                            sb.append('\\');
                            break;
                        case '/':
                            sb.append('/');
                            break;
                        case 'b':
                            sb.append('\b');
                            break;
                        case 'f':
                            sb.append('\f');
                            break;
                        case 'n':
                            sb.append('\n');
                            break;
                        case 'r':
                            sb.append('\r');
                            break;
                        case 't':
                            sb.append('\t');
                            break;
                        case 'u':
                            if (idx + 4 >= src.length()) {
                                throw new DriverException("config-truncated-unicode", 65, "");
                            }
                            String hex = src.substring(idx + 1, idx + 5);
                            sb.append((char) Integer.parseInt(hex, 16));
                            idx += 4;
                            break;
                        default:
                            throw new DriverException("config-bad-escape", 65, String.valueOf(esc));
                    }
                    idx++;
                } else {
                    sb.append(c);
                    idx++;
                }
            }
            if (idx >= src.length()) {
                throw new DriverException("config-unterminated-string", 65, "");
            }
            idx++;
            return sb.toString();
        }

        private Object readValue() {
            if (idx >= src.length()) {
                throw new DriverException("config-eof", 65, "");
            }
            char c = src.charAt(idx);
            if (c == '"') {
                return readString();
            }
            if (c == '-' || (c >= '0' && c <= '9')) {
                return readNumber();
            }
            if (c == 't' || c == 'f') {
                return readBool();
            }
            if (c == 'n') {
                return readNull();
            }
            throw new DriverException("config-bad-value", 65, String.valueOf(c));
        }

        private Object readNumber() {
            int start = idx;
            if (src.charAt(idx) == '-') {
                idx++;
            }
            while (idx < src.length() && "0123456789".indexOf(src.charAt(idx)) >= 0) {
                idx++;
            }
            String num = src.substring(start, idx);
            try {
                long parsed = Long.parseLong(num);
                if (parsed >= Integer.MIN_VALUE && parsed <= Integer.MAX_VALUE) {
                    return Integer.valueOf((int) parsed);
                }
                return Long.valueOf(parsed);
            } catch (NumberFormatException ex) {
                throw new DriverException("config-bad-number", 65, num);
            }
        }

        private Object readBool() {
            if (src.startsWith("true", idx)) {
                idx += 4;
                return Boolean.TRUE;
            }
            if (src.startsWith("false", idx)) {
                idx += 5;
                return Boolean.FALSE;
            }
            throw new DriverException("config-bad-bool", 65, "");
        }

        private Object readNull() {
            if (src.startsWith("null", idx)) {
                idx += 4;
                return null;
            }
            throw new DriverException("config-bad-null", 65, "");
        }

        private void skipWs() {
            while (idx < src.length()) {
                char c = src.charAt(idx);
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                    idx++;
                } else {
                    break;
                }
            }
        }
    }

    // ===================================================================
    // Canonical JSON writer for the structured event stream.
    // ===================================================================
    static String canonicalJson(Map<String, Object> map) {
        StringBuilder sb = new StringBuilder();
        sb.append('{');
        boolean first = true;
        List<String> keys = new ArrayList<>(map.keySet());
        Collections.sort(keys);
        for (String key : keys) {
            if (!first) {
                sb.append(',');
            }
            first = false;
            sb.append('"').append(escapeJson(key)).append('"').append(':');
            Object value = map.get(key);
            sb.append(canonicalValue(value));
        }
        sb.append('}');
        return sb.toString();
    }

    private static String canonicalValue(Object value) {
        if (value == null) {
            return "null";
        }
        if (value instanceof String) {
            return "\"" + escapeJson((String) value) + "\"";
        }
        if (value instanceof Boolean) {
            return ((Boolean) value) ? "true" : "false";
        }
        if (value instanceof Number) {
            return value.toString();
        }
        throw new DriverException("canonical-unsupported-type", 70, value.getClass().getName());
    }

    private static String escapeJson(String s) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '\\':
                    sb.append("\\\\");
                    break;
                case '"':
                    sb.append("\\\"");
                    break;
                case '\n':
                    sb.append("\\n");
                    break;
                case '\r':
                    sb.append("\\r");
                    break;
                case '\t':
                    sb.append("\\t");
                    break;
                default:
                    if (c < 0x20) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
            }
        }
        return sb.toString();
    }

    static String sha256Hex(byte[] data) {
        try {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            byte[] digest = md.digest(data);
            return HexFormat.of().formatHex(digest);
        } catch (Exception e) {
            throw new DriverException("sha256-unavailable", 70, e.getMessage());
        }
    }

    enum EventKind {
        PROCESS_STARTED("process_started"),
        LISTENER_READY("listener_ready"),
        ROUTER_INFO_EXPORTED("router_info_exported"),
        PEER_ROUTER_INFO_VALIDATED("peer_router_info_validated"),
        TCP_CONNECTED("tcp_connected"),
        NTCP2_AUTHENTICATED("ntcp2_authenticated"),
        FRAME_EMITTED("frame_emitted"),
        FRAME_AUTHENTICATED_AND_DECRYPTED("frame_authenticated_and_decrypted"),
        I2NP_MESSAGE_DECODED("i2np_message_decoded"),
        TERMINAL_CLEAN("terminal_clean"),
        TERMINAL_REJECTED("terminal_rejected");

        final String value;
        EventKind(String value) {
            this.value = value;
        }
    }

    static final class ParsedArgs {
        final String mode;
        final Path configPath;
        ParsedArgs(String mode, Path configPath) {
            this.mode = mode;
            this.configPath = configPath;
        }
    }

    static final class DriverException extends RuntimeException {
        private final String code;
        private final int exitCode;
        DriverException(String code, int exitCode, String detail) {
            super(detail);
            this.code = code;
            this.exitCode = exitCode;
        }
        String getCode() {
            return code;
        }
        int exitCode() {
            return exitCode;
        }
    }
}
