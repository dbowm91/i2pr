// Plan 198 counted reference service; Plan 200 evidence-semantics
// corrective.  The I2P side uses only the public Streaming API.  The
// localhost control socket is test coordination only.
//
// Plan 200 §A.1 — `READY` (and the optional `STATUS` report)
// intentionally says only what the helper process can prove locally:
//
//   public helper process alive
//   I2CP session established
//   Destination public material available
//   control socket ready
//
// `READY` MUST NOT imply or assert that the destination's LeaseSet2
// is network-visible.  Network-visible publication is a separate
// question and is proved by the harness through ordinary I2NP
// DatabaseLookup/Store traffic, never by helper readiness.

import net.i2p.I2PAppContext;
import net.i2p.client.I2PClient;
import net.i2p.client.I2PClientFactory;
import net.i2p.client.I2PSession;
import net.i2p.client.streaming.I2PServerSocket;
import net.i2p.client.streaming.I2PSocket;
import net.i2p.client.streaming.I2PSocketManager;
import net.i2p.client.streaming.I2PSocketManagerFactory;
import net.i2p.crypto.SigType;
import net.i2p.data.Destination;
import net.i2p.stat.RateStat;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.PrintWriter;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

public final class ReferenceStreamingService {
    private static final String PIN = "9134f808337b401e8e53c73734c81fab04280c9d";
    // Plan 200 §A.1 — bounded, sanitized, non-publication facts only.
    private static final String PUBLIC_CLIENT_SESSION_CONNECTED = "1";
    private static final String PUBLIC_CLIENT_CONTROL_READY = "1";
    private static final AtomicLong HELPER_START_MS = new AtomicLong(System.currentTimeMillis());
    private static final Map<Integer, I2PSocket> SOCKETS = new ConcurrentHashMap<>();
    private static final List<Integer> ACCEPTED = Collections.synchronizedList(new ArrayList<>());
    private static final List<Integer> CONNECTED = Collections.synchronizedList(new ArrayList<>());
    private static final AtomicInteger NEXT_ID = new AtomicInteger(1);
    // Plan 234/235 — helper-local accept observability.  These counters are
    // deliberately bounded and expose only control-flow facts; they never
    // expose packet contents, keys, tags, or router internals.
    private static final int MAX_OBSERVATIONS = 1024;
    private static final AtomicInteger ACCEPT_REQUESTED = new AtomicInteger();
    private static final AtomicInteger ACCEPT_ENTERED = new AtomicInteger();
    private static final AtomicInteger ACCEPT_RETURNED = new AtomicInteger();
    private static final AtomicInteger ACCEPT_STORED = new AtomicInteger();
    private static final AtomicInteger ACCEPT_ERRORS = new AtomicInteger();
    // Plan 235 §B — public I2PSocket surface only.  These counters do not
    // claim that a SYN-ACK reached the Rust peer; they record whether the
    // socket returned by the stock public accept API exposed its public
    // input/output streams without a helper-side exception.
    private static final AtomicInteger SOCKET_SURFACE_ENTERED = new AtomicInteger();
    private static final AtomicInteger SOCKET_SURFACE_READY = new AtomicInteger();
    private static final AtomicInteger SOCKET_SURFACE_ERRORS = new AtomicInteger();
    // Plan 236 §6 — source-locked facts are emitted as constants from the
    // helper protocol. The separate source-lock script proves these names
    // against the exact checkout; the helper never claims that any stage was
    // observed merely because the source path exists.
    private static final String JAVA_SOURCE_PIN = PIN;
    private static final String JAVA_RESPONSE_SCHEDULER_CLASS =
        "net.i2p.client.streaming.impl.SchedulerReceived";
    private static final String JAVA_RESPONSE_SCHEDULER_METHOD = "eventOccurred";
    private static final String JAVA_RESPONSE_PACKET_KIND = "ACK_OR_SYN_ACK";
    private static final String JAVA_RESPONSE_SEND_METHOD =
        "Connection.sendPacket(PacketLocal)";
    private static final String JAVA_PACKETQUEUE_METHOD =
        "PacketQueue.enqueue(PacketLocal)";
    private static final String JAVA_I2PSESSION_SEND_METHOD =
        "boolean_sendMessage_SendMessageOptions";
    private static volatile boolean accepting;

    // Plan 237 §4–§5 — stock helper-JVM response observation. All facts
    // are bounded public counts from the exact-pinned implementation:
    // `stream.con.sendMessageSize` lifetime events via the public
    // `StatManager.getRate(...).getLifetimeEventCount()`, plus exact
    // source-locked DEBUG/WARN substrings scanned from the public
    // `LogManager.getBuffer().getMostRecentMessages()` console buffer.
    // No packet contents, keys, tags, plaintext payloads, or private
    // state. Counts are bounded by the console buffer size (512) and
    // MAX_OBSERVATIONS.
    private static final int P237_CONSOLE_BUFFER_SIZE = 512;
    // Plan 237 §9 corrective (counted attempt 1 finding): the pinned
    // `SchedulerReceived.eventOccurred()` source emits its DEBUG strings
    // through the superclass logger (`SchedulerImpl._log =
    // logManager().getLog(SchedulerImpl.class)`), so DEBUG must be
    // enabled for `SchedulerImpl` (actual) as well as the plan-named
    // `SchedulerReceived` (harmless, documents intent).
    private static final String P237_SCHEDULER_IMPL_CLASS =
        "net.i2p.client.streaming.impl.SchedulerImpl";
    private static final String P237_SCHEDULER_CLASS =
        "net.i2p.client.streaming.impl.SchedulerReceived";
    private static final String P237_CONNECTION_CLASS =
        "net.i2p.client.streaming.impl.Connection";
    private static final String P237_PACKETQUEUE_CLASS =
        "net.i2p.client.streaming.impl.PacketQueue";
    private static final String P237_SCHEDULER_SIGNAL = "received con... ";
    // Plan 237 §4.2 corrective (counted attempt 1 finding): the pinned
    // `Connection.ackImmediately()` "sending new ack" log fires only on
    // duplicate/fast-ack/close paths, never on the fresh SYN -> SYN-ACK
    // response path (which runs SchedulerReceived -> sendAvailable ->
    // sendPacket). The active stock signal inside the source-locked
    // `Connection.sendPacket(PacketLocal)` on a first-time response send
    // is the retransmit-timer log below (unique to sendPacket in the
    // exact-pinned streaming implementation).
    private static final String P237_SENDPACKET_SIGNAL = "Resend in ";
    private static final String P237_SEND_FAILED_SIGNAL = "Send failed for ";
    private static final String P237_SEND_EXCEPTION_SIGNAL = "Unable to send the packet";
    private static final String P237_SENDMESSAGE_STAT = "stream.con.sendMessageSize";

    private static void p237ConfigureStockObserver() {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null) return;
            context.logManager().setConsoleBufferSize(P237_CONSOLE_BUFFER_SIZE);
            Properties limits = new Properties();
            limits.setProperty(P237_SCHEDULER_IMPL_CLASS, "DEBUG");
            limits.setProperty(P237_SCHEDULER_CLASS, "DEBUG");
            limits.setProperty(P237_CONNECTION_CLASS, "DEBUG");
            limits.setProperty(P237_PACKETQUEUE_CLASS, "DEBUG");
            context.logManager().setLimits(limits);
        } catch (Throwable ignored) { }
    }

    private static long p237SendMessageSizeLifetimeEvents() {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.statManager() == null) return 0;
            RateStat rate = context.statManager().getRate(P237_SENDMESSAGE_STAT);
            if (rate == null) return 0;
            long count = rate.getLifetimeEventCount();
            return Math.max(0, count);
        } catch (Throwable ignored) {
            return 0;
        }
    }

    private static int p237CountBufferSubstring(String needle) {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null
                || context.logManager().getBuffer() == null) return 0;
            int count = 0;
            for (String message : context.logManager().getBuffer().getMostRecentMessages()) {
                if (message == null) continue;
                if (message.contains(needle)) {
                    count++;
                    if (count >= MAX_OBSERVATIONS) break;
                }
            }
            return count;
        } catch (Throwable ignored) {
            return 0;
        }
    }

    private static boolean p237IsDebugEnabledFor(String className) {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null) return false;
            Properties limits = context.logManager().getLimits();
            if (limits == null) return false;
            String level = limits.getProperty(className);
            return level != null && level.equalsIgnoreCase("DEBUG");
        } catch (Throwable ignored) {
            return false;
        }
    }

    private static boolean p237SchedulerObserverEnabled() {
        // The actual scheduler logger is SchedulerImpl (see above); the
        // plan-named SchedulerReceived entry is retained for intent.
        return p237IsDebugEnabledFor(P237_SCHEDULER_IMPL_CLASS);
    }

    private static int incrementBounded(AtomicInteger counter) {
        return counter.updateAndGet(value -> value < MAX_OBSERVATIONS ? value + 1 : value);
    }

    private static String hex(byte[] bytes) {
        StringBuilder out = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) out.append(String.format("%02x", b & 0xff));
        return out.toString();
    }

    private static byte[] unhex(String text) {
        if ((text.length() & 1) != 0) throw new IllegalArgumentException("odd hex");
        byte[] out = new byte[text.length() / 2];
        for (int i = 0; i < out.length; i++) out[i] = (byte) Integer.parseInt(text.substring(i * 2, i * 2 + 2), 16);
        return out;
    }

    private static String digest(byte[] bytes) throws Exception {
        return hex(MessageDigest.getInstance("SHA-256").digest(bytes));
    }

    private static Properties options(String host, int port) {
        Properties options = new Properties();
        options.setProperty("i2cp.tcp.host", host);
        options.setProperty("i2cp.tcp.port", Integer.toString(port));
        // Match the public raw destination helper and retain the existing
        // bounded zero-hop client profile after the RouterInfo bootstrap.
        options.setProperty("inbound.length", "0");
        options.setProperty("outbound.length", "0");
        options.setProperty("inbound.quantity", "1");
        options.setProperty("outbound.quantity", "1");
        options.setProperty("inbound.backupQuantity", "0");
        options.setProperty("outbound.backupQuantity", "0");
        options.setProperty("inbound.allowZeroHop", "true");
        options.setProperty("outbound.allowZeroHop", "true");
        options.setProperty("i2cp.leaseSetType", "3");
        options.setProperty("i2cp.leaseSetEncType", "4");
        options.setProperty("i2cp.dontPublishLeaseSet", "false");
        options.setProperty("i2cp.fastReceive", "true");
        options.setProperty("i2cp.messageReliability", "BestEffort");
        return options;
    }

    private static Destination createDestination(I2PClient client, File keyFile) throws Exception {
        try (FileOutputStream output = new FileOutputStream(keyFile)) {
            return client.createDestination(output, SigType.EdDSA_SHA512_Ed25519);
        }
    }

    private static I2PSocket socket(int id) {
        I2PSocket socket = SOCKETS.get(id);
        if (socket == null) throw new IllegalArgumentException("unknown socket");
        return socket;
    }

    private static int acceptOne(I2PServerSocket server) {
        incrementBounded(ACCEPT_ENTERED);
        boolean surfaceEntered = false;
        try {
            I2PSocket socket = server.accept();
            incrementBounded(ACCEPT_RETURNED);
            incrementBounded(SOCKET_SURFACE_ENTERED);
            surfaceEntered = true;
            socket.getInputStream();
            socket.getOutputStream();
            incrementBounded(SOCKET_SURFACE_READY);
            int id = NEXT_ID.getAndIncrement();
            SOCKETS.put(id, socket);
            ACCEPTED.add(id);
            incrementBounded(ACCEPT_STORED);
            return id;
        } catch (Throwable error) {
            incrementBounded(ACCEPT_ERRORS);
            if (surfaceEntered) {
                incrementBounded(SOCKET_SURFACE_ERRORS);
            }
            return -1;
        }
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 4) throw new IllegalArgumentException("usage: host i2cpPort controlPort keyFile");
        String host = args[0];
        int i2cpPort = Integer.parseInt(args[1]);
        int controlPort = Integer.parseInt(args[2]);
        File key = new File(args[3]);
        I2PClient client = I2PClientFactory.createClient();
        createDestination(client, key);
        I2PSocketManager manager;
        try (FileInputStream input = new FileInputStream(key)) {
            manager = I2PSocketManagerFactory.createDisconnectedManager(input, host, i2cpPort, options(host, i2cpPort));
        }
        I2PSession session = manager.getSession();
        session.connect();
        // Plan 237 §9 — enable only the three exact-pinned Streaming
        // classes at DEBUG, before any SYN can arrive. Uses only public
        // LogManager APIs; never patches router/client internals, never
        // uses reflection, never mutates private state.
        p237ConfigureStockObserver();
        I2PServerSocket server = manager.getServerSocket();
        Destination destination = session.getMyDestination();
        try (ServerSocket control = new ServerSocket(controlPort, 16, java.net.InetAddress.getByName("127.0.0.1"))) {
            // Plan 200 §A.1 — `READY` is intentionally the minimal helper
            // process + session + destination + control-socket fact set;
            // it MUST NOT include any publication claim. See
            // ReferenceRawDestination for the same rationale.
            System.out.println("READY PUBLIC_CLIENT_SESSION_CONNECTED=" + PUBLIC_CLIENT_SESSION_CONNECTED
                + " PUBLIC_CLIENT_DESTINATION_LEN=" + destination.toBase64().length()
                + " PUBLIC_CLIENT_CONTROL_READY=" + PUBLIC_CLIENT_CONTROL_READY
                + " " + destination.toBase64()
                + " " + PIN);
            System.out.flush();
            boolean stop = false;
            while (!stop) {
                try (Socket socket = control.accept();
                     BufferedReader input = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII));
                     PrintWriter output = new PrintWriter(socket.getOutputStream(), true)) {
                    String line = input.readLine();
                    if (line == null) continue;
                    String[] values = line.split(" ");
                    switch (values[0]) {
                        case "PING": output.println("PONG"); break;
                        case "REPORT_STATUS": {
                            // Plan 200 §A.2 — explicit, bounded status
                            // report. Asserts ONLY helper-local facts;
                            // never asserts publication or network
                            // visibility (those are external questions
                            // proved by the harness through ordinary I2NP
                            // traffic).
                            long uptimeMs = System.currentTimeMillis() - HELPER_START_MS.get();
                            output.println("STATUS public_client_session_connected=" + PUBLIC_CLIENT_SESSION_CONNECTED
                                + " public_client_destination_len=" + destination.toBase64().length()
                                + " public_client_control_ready=" + PUBLIC_CLIENT_CONTROL_READY
                                + " helper_uptime_ms=" + uptimeMs
                                + " publications_observed=no");
                            break;
                        }
                        case "REPORT_STREAM_STATE": {
                            // Plan 234 §8 / Plan 235 §B — the response is a bounded,
                            // read-only accept epoch.  `accept_returned`
                            // means only that the public Java Streaming API
                            // returned an I2PSocket; wire-stage attribution
                            // remains the Rust driver's responsibility.
                            output.println("STREAM_STATUS accept_requested=" + ACCEPT_REQUESTED.get()
                                + " accept_entered=" + ACCEPT_ENTERED.get()
                                + " accept_returned=" + ACCEPT_RETURNED.get()
                                + " socket_stored=" + ACCEPT_STORED.get()
                                + " accept_errors=" + ACCEPT_ERRORS.get()
                                + " socket_surface_entered=" + SOCKET_SURFACE_ENTERED.get()
                                + " socket_surface_ready=" + SOCKET_SURFACE_READY.get()
                                + " socket_surface_errors=" + SOCKET_SURFACE_ERRORS.get()
                                + " accepting=" + accepting
                                + " accepted_count=" + ACCEPTED.size()
                                + " connected_count=" + CONNECTED.size()
                                + " java_source_pin=" + JAVA_SOURCE_PIN
                                + " java_response_scheduler_class=" + JAVA_RESPONSE_SCHEDULER_CLASS
                                + " java_response_scheduler_method=" + JAVA_RESPONSE_SCHEDULER_METHOD
                                + " java_response_packet_kind=" + JAVA_RESPONSE_PACKET_KIND
                                + " java_response_send_method=" + JAVA_RESPONSE_SEND_METHOD
                                + " java_packetqueue_method=" + JAVA_PACKETQUEUE_METHOD
                                + " java_i2psession_send_method=" + JAVA_I2PSESSION_SEND_METHOD
                                // Plan 236 §7–§10 — no stage is inferred from
                                // accept(). These remain Unknown until stock
                                // log/status evidence proves each boundary.
                                + " java_response_observation_complete=false"
                                + " java_response_scheduler_observed=false"
                                + " java_response_packet_constructed=false"
                                + " java_sendpacket_observed=false"
                                + " java_packetqueue_observed=false"
                                + " java_packetqueue_send_failed=false"
                                + " java_i2psession_send_observed=false"
                                + " java_i2psession_send_failed=false"
                                + " java_router_i2cp_observed=false"
                                + " java_client_message_admitted=false"
                                + " java_target_leaseset_selected=false"
                                + " java_outbound_tunnel_selected=false"
                                + " java_dispatch_outbound_called=false"
                                + " java_outbound_gateway_enqueued=false"
                                + " java_transit_processed=false"
                                + " java_target_ibgw_present=false"
                                + " java_target_ibgw_dispatched=false");
                            break;
                        }
                        case "REPORT_RESPONSE_STATS": {
                            // Plan 237 §5 — bounded public stock facts for
                            // the isolated SYN epoch. The Rust driver
                            // snapshots this command before the SYN and at
                            // the end of the frozen response window, then
                            // classifies from deltas (never absolutes).
                            long sendEvents = p237SendMessageSizeLifetimeEvents();
                            int schedulerCount =
                                p237CountBufferSubstring(P237_SCHEDULER_SIGNAL);
                            int ackCount =
                                p237CountBufferSubstring(P237_SENDPACKET_SIGNAL);
                            int sendFail =
                                p237CountBufferSubstring(P237_SEND_FAILED_SIGNAL);
                            int sendException =
                                p237CountBufferSubstring(P237_SEND_EXCEPTION_SIGNAL);
                            boolean schedulerDebug =
                                p237SchedulerObserverEnabled();
                            boolean connectionDebug =
                                p237IsDebugEnabledFor(P237_CONNECTION_CLASS);
                            boolean packetqueueDebug =
                                p237IsDebugEnabledFor(P237_PACKETQUEUE_CLASS);
                            output.println("RESPONSE_STATS scheduler_log_count=" + schedulerCount
                                + " ack_constructed_log_count=" + ackCount
                                + " send_message_size_lifetime_events=" + sendEvents
                                + " send_failure_count=" + sendFail
                                + " send_exception_count=" + sendException
                                + " scheduler_debug_enabled=" + schedulerDebug
                                + " connection_debug_enabled=" + connectionDebug
                                + " packetqueue_debug_enabled=" + packetqueueDebug);
                            break;
                        }
                        case "START_ACCEPT":
                            if (!accepting) {
                                accepting = true;
                                incrementBounded(ACCEPT_REQUESTED);
                                Thread thread = new Thread(() -> {
                                    try {
                                        acceptOne(server);
                                    } finally {
                                        accepting = false;
                                    }
                                }, "plan234-accept");
                                thread.setDaemon(true);
                                thread.start();
                            }
                            output.println("STARTED");
                            break;
                        case "ACCEPT_STATUS": {
                            int index = Integer.parseInt(values[1]);
                            synchronized (ACCEPTED) {
                                output.println(ACCEPTED.size() > index ? "ACCEPTED " + ACCEPTED.get(index) : "WAITING");
                            }
                            break;
                        }
                        case "START_CONNECT": {
                            Destination peer = new Destination(values[1]);
                            Thread thread = new Thread(() -> {
                                try {
                                    I2PSocket connected = manager.connect(peer);
                                    int id = NEXT_ID.getAndIncrement();
                                    SOCKETS.put(id, connected);
                                    synchronized (CONNECTED) { CONNECTED.add(id); }
                                } catch (Throwable ignored) { }
                            }, "plan198-connect");
                            thread.setDaemon(true);
                            thread.start();
                            output.println("STARTED");
                            break;
                        }
                        case "CONNECT_STATUS": {
                            int index = Integer.parseInt(values[1]);
                            synchronized (CONNECTED) {
                                output.println(CONNECTED.size() > index ? "CONNECTED " + CONNECTED.get(index) : "WAITING");
                            }
                            break;
                        }
                        case "WRITE": {
                            I2PSocket target = socket(Integer.parseInt(values[1]));
                            byte[] body = unhex(values[2]);
                            OutputStream stream = target.getOutputStream();
                            stream.write(body);
                            stream.flush();
                            output.println("WROTE " + body.length + " " + digest(body));
                            break;
                        }
                        case "READ": {
                            I2PSocket target = socket(Integer.parseInt(values[1]));
                            int size = Integer.parseInt(values[2]);
                            byte[] body = target.getInputStream().readNBytes(size);
                            output.println(body.length == size ? "READ " + body.length + " " + hex(body) : "SHORT " + body.length);
                            break;
                        }
                        case "EOF": {
                            I2PSocket target = socket(Integer.parseInt(values[1]));
                            byte[] buffer = new byte[1024];
                            while (target.getInputStream().read(buffer) >= 0) { }
                            output.println("EOF");
                            break;
                        }
                        case "CLOSE": {
                            int id = Integer.parseInt(values[1]);
                            I2PSocket target = SOCKETS.remove(id);
                            if (target != null) target.close();
                            output.println("CLOSED");
                            break;
                        }
                        case "STOP": output.println("STOPPING"); stop = true; break;
                        default: output.println("ERROR unknown-command");
                    }
                }
            }
        } finally {
            for (I2PSocket socket : SOCKETS.values()) try { socket.close(); } catch (Throwable ignored) { }
            server.close();
            manager.destroySocketManager();
            key.delete();
        }
    }
}
