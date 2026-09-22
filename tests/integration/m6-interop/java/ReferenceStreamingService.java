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

import net.i2p.client.I2PClient;
import net.i2p.client.I2PClientFactory;
import net.i2p.client.I2PSession;
import net.i2p.client.streaming.I2PServerSocket;
import net.i2p.client.streaming.I2PSocket;
import net.i2p.client.streaming.I2PSocketManager;
import net.i2p.client.streaming.I2PSocketManagerFactory;
import net.i2p.crypto.SigType;
import net.i2p.data.Destination;

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
    private static volatile boolean accepting;

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
                                + " connected_count=" + CONNECTED.size());
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
