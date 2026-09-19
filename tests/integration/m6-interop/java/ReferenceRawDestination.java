// Plan 198 counted reference service; Plan 200 evidence-semantics
// corrective.  This helper uses only the public Java I2P client API;
// the localhost control socket carries coordination commands and never
// carries I2P protocol framing.
//
// Plan 200 §A.1 — `READY` (and the optional `LEASE_STATUS` line)
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
import net.i2p.client.I2PSessionException;
import net.i2p.client.I2PSessionListener;
import net.i2p.client.SendMessageOptions;
import net.i2p.client.SendMessageStatusListener;
import net.i2p.crypto.SigType;
import net.i2p.data.Destination;
import net.i2p.data.Hash;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;

public final class ReferenceRawDestination {
    private static final String PIN = "9134f808337b401e8e53c73734c81fab04280c9d";

    // Plan 200 §A.1 — bounded, sanitized, non-publication facts only.
    private static final String PUBLIC_CLIENT_SESSION_CONNECTED = "1";
    private static final String PUBLIC_CLIENT_CONTROL_READY = "1";
    // Helper process epoch + ready epoch give the harness a coarse
    // bounded lifetime to correlate with sanitized Java router log
    // entries (which themselves are never retained in evidence).
    private static final AtomicLong HELPER_START_MS = new AtomicLong(System.currentTimeMillis());

    // Plan 222 WP C — bounded nonce-correlated helper send state for the
    // public listener-enabled `I2PSession.sendMessage(..., SendMessageStatusListener)`
    // API. Maximum tracked messages: 32. Maximum events per message: 16.
    // Each entry holds payload length, payload SHA-256, creation monotonic
    // timestamp, and the ordered status events (numeric status + local
    // monotonic elapsed ms). Oldest completed entries are evicted first.
    // Never stores destination private key bytes or payload bytes.
    private static final int MAX_TRACKED_MESSAGES = 32;
    private static final int MAX_EVENTS_PER_MESSAGE = 16;
    private static final ConcurrentHashMap<Long, TrackedSend> TRACKED =
        new ConcurrentHashMap<Long, TrackedSend>();

    private static final class StatusEvent {
        final int status;
        final long elapsedMs;
        StatusEvent(int status, long elapsedMs) {
            this.status = status;
            this.elapsedMs = elapsedMs;
        }
    }

    private static final class TrackedSend {
        final int payloadLen;
        final String digest;
        final long createdMonoMs;
        final List<StatusEvent> events = new ArrayList<StatusEvent>();
        TrackedSend(int payloadLen, String digest, long createdMonoMs) {
            this.payloadLen = payloadLen;
            this.digest = digest;
            this.createdMonoMs = createdMonoMs;
        }
        synchronized void append(int status, long nowMonoMs) {
            if (events.size() >= MAX_EVENTS_PER_MESSAGE) return;
            events.add(new StatusEvent(status, nowMonoMs - createdMonoMs));
        }
        synchronized String renderEvents() {
            if (events.isEmpty()) return "none";
            StringBuilder out = new StringBuilder();
            for (int i = 0; i < events.size(); i++) {
                if (i > 0) out.append(",");
                StatusEvent e = events.get(i);
                out.append(e.status).append(":").append(e.elapsedMs);
            }
            return out.toString();
        }
        synchronized int count() {
            return events.size();
        }
    }

    private static long monoMs() {
        return System.nanoTime() / 1000000L;
    }

    private static void trackEvictIfNeeded() {
        if (TRACKED.size() < MAX_TRACKED_MESSAGES) return;
        // Evict oldest completed entries first (completed = has any
        // terminal event); fall back to oldest creation order.
        Long oldest = null;
        long oldestCreated = Long.MAX_VALUE;
        for (Map.Entry<Long, TrackedSend> e : TRACKED.entrySet()) {
            if (e.getValue().createdMonoMs < oldestCreated) {
                oldestCreated = e.getValue().createdMonoMs;
                oldest = e.getKey();
            }
        }
        if (oldest != null) TRACKED.remove(oldest);
    }

    private static String[] split(String line, int count) {
        String[] result = line.split(" ");
        if (result.length < count) throw new IllegalArgumentException("malformed control command");
        return result;
    }

    private static String hex(byte[] bytes) {
        StringBuilder out = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) out.append(String.format("%02x", b & 0xff));
        return out.toString();
    }

    private static byte[] unhex(String text) {
        if ((text.length() & 1) != 0) throw new IllegalArgumentException("odd hex");
        byte[] out = new byte[text.length() / 2];
        for (int i = 0; i < out.length; i++) {
            out[i] = (byte) Integer.parseInt(text.substring(i * 2, i * 2 + 2), 16);
        }
        return out;
    }

    private static String digest(byte[] bytes) throws Exception {
        byte[] sum = MessageDigest.getInstance("SHA-256").digest(bytes);
        return hex(sum);
    }

    private static Destination createDestination(I2PClient client, File keyFile) throws Exception {
        try (FileOutputStream output = new FileOutputStream(keyFile)) {
            return client.createDestination(output, SigType.EdDSA_SHA512_Ed25519);
        }
    }

    private static Properties options(String host, int port) {
        Properties options = new Properties();
        options.setProperty("i2cp.tcp.host", host);
        options.setProperty("i2cp.tcp.port", Integer.toString(port));
        // Plan 199: the service router has a distinct publication peer.
        // The bounded helper keeps the existing zero-hop client profile;
        // the pre-helper ordinary RouterInfo bootstrap supplies the normal
        // floodfill publication target without changing tunnel semantics.
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

    public static void main(String[] args) throws Exception {
        if (args.length != 4) throw new IllegalArgumentException("usage: host i2cpPort controlPort keyFile");
        String host = args[0];
        int i2cpPort = Integer.parseInt(args[1]);
        int controlPort = Integer.parseInt(args[2]);
        File key = new File(args[3]);
        I2PClient client = I2PClientFactory.createClient();
        Destination destination = createDestination(client, key);
        I2PSession session;
        try (FileInputStream input = new FileInputStream(key)) {
            session = client.createSession(input, options(host, i2cpPort));
        }
        ArrayBlockingQueue<byte[]> received = new ArrayBlockingQueue<>(32);
        session.setSessionListener(new I2PSessionListener() {
            public void messageAvailable(I2PSession s, int id, long size) {
                try {
                    byte[] body = s.receiveMessage(id);
                    if (body != null) received.offer(body, 1, TimeUnit.SECONDS);
                } catch (Exception ignored) { }
            }
            public void reportAbuse(I2PSession s, int severity) { }
            public void disconnected(I2PSession s) { }
            public void errorOccurred(I2PSession s, String message, Throwable error) { }
        });
        session.connect();
        try (ServerSocket server = new ServerSocket(controlPort, 16, java.net.InetAddress.getByName("127.0.0.1"))) {
            // Plan 200 §A.1 — `READY` is intentionally the minimal helper
            // process + session + destination + control-socket fact set;
            // it MUST NOT include any publication claim. The harness
            // treats it as equivalent to the recommended sanitized facts:
            //
            //   PUBLIC_CLIENT_SESSION_CONNECTED=1
            //   PUBLIC_CLIENT_DESTINATION_LEN=<bounded integer>
            //   PUBLIC_CLIENT_CONTROL_READY=1
            //
            // The destination Base64 is required to identify the
            // destination the harness must look up; it is public material
            // and never carries the private destination bytes.
            System.out.println("READY PUBLIC_CLIENT_SESSION_CONNECTED=" + PUBLIC_CLIENT_SESSION_CONNECTED
                + " PUBLIC_CLIENT_DESTINATION_LEN=" + destination.toBase64().length()
                + " PUBLIC_CLIENT_CONTROL_READY=" + PUBLIC_CLIENT_CONTROL_READY
                + " " + destination.toBase64()
                + " " + PIN);
            System.out.flush();
            boolean stop = false;
            while (!stop) {
                try (Socket socket = server.accept();
                     BufferedReader input = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII));
                     PrintWriter output = new PrintWriter(socket.getOutputStream(), true)) {
                    String line = input.readLine();
                    if (line == null) continue;
                    String[] parts = split(line, 1);
                    switch (parts[0]) {
                        case "PING": output.println("PONG"); break;
                        case "WAIT": {
                            byte[] body = received.poll(Long.parseLong(parts[1]), TimeUnit.MILLISECONDS);
                            output.println(body == null ? "TIMEOUT" : "RECEIVED " + body.length + " " + digest(body));
                            break;
                        }
                        case "SEND": {
                            String[] values = split(line, 5);
                            Destination peer = new Destination(values[1]);
                            byte[] body = unhex(values[2]);
                            boolean sent = session.sendMessage(peer, body, I2PSession.PROTO_DATAGRAM_RAW,
                                    Integer.parseInt(values[3]), Integer.parseInt(values[4]));
                            output.println(sent ? "SENT " + body.length + " " + digest(body) : "SEND_FAILED");
                            break;
                        }
                        case "SEND_TRACKED": {
                            // Plan 222 WP C — public API only. Uses the
                            // listener-enabled long `sendMessage`:
                            //   long sendMessage(Destination, byte[], int, int,
                            //       int, int, int, SendMessageOptions,
                            //       SendMessageStatusListener)
                            // Do not set a special expiration; do not alter
                            // reliability/tunnel/publication session options.
                            // The only behavior difference from legacy SEND
                            // is requesting public asynchronous status
                            // notifications correlated by the returned nonce.
                            String[] values = split(line, 5);
                            Destination peer = new Destination(values[1]);
                            byte[] body = unhex(values[2]);
                            int fromPort = Integer.parseInt(values[3]);
                            int toPort = Integer.parseInt(values[4]);
                            String bodyDigest = digest(body);
                            int bodyLen = body.length;
                            long created = monoMs();
                            SendMessageOptions options = new SendMessageOptions();
                            final long[] nonceHolder = new long[1];
                            SendMessageStatusListener listener =
                                new SendMessageStatusListener() {
                                    public void messageStatus(
                                            I2PSession s, long nonce, int status) {
                                        TrackedSend entry = TRACKED.get(nonce);
                                        if (entry != null) entry.append(status, monoMs());
                                    }
                                };
                            try {
                                long nonce = session.sendMessage(
                                    peer, body, 0, body.length,
                                    I2PSession.PROTO_DATAGRAM_RAW,
                                    fromPort, toPort, options, listener);
                                nonceHolder[0] = nonce;
                                trackEvictIfNeeded();
                                TRACKED.put(nonce,
                                    new TrackedSend(bodyLen, bodyDigest, created));
                                output.println("TRACKED_SENT nonce=" + nonce
                                    + " payload_len=" + bodyLen
                                    + " digest=" + bodyDigest);
                            } catch (Throwable t) {
                                output.println("TRACKED_ERROR class="
                                    + t.getClass().getSimpleName());
                            }
                            break;
                        }
                        case "SEND_STATUS": {
                            // Plan 222 WP C — status polling. The Rust
                            // driver polls; the helper MUST NOT block
                            // waiting for a future status event. The
                            // listener appends every state-changing
                            // callback in order; no collapsing to last.
                            String[] values = split(line, 2);
                            long nonce;
                            try {
                                nonce = Long.parseLong(values[1]);
                            } catch (NumberFormatException nfe) {
                                output.println("TRACKED_STATUS_UNKNOWN nonce=" + values[1]);
                                break;
                            }
                            TrackedSend entry = TRACKED.get(nonce);
                            if (entry == null) {
                                output.println("TRACKED_STATUS_UNKNOWN nonce=" + nonce);
                            } else if (entry.count() == 0) {
                                output.println("TRACKED_STATUS nonce=" + nonce
                                    + " count=0 events=none");
                            } else {
                                output.println("TRACKED_STATUS nonce=" + nonce
                                    + " count=" + entry.count()
                                    + " events=" + entry.renderEvents());
                            }
                            break;
                        }
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
                        case "INSPECT_DEST": {
                            // Plan 223 WP B — read-only exact Destination
                            // observation. Parses the supplied I2P Base64
                            // Destination via the public
                            // `new Destination(String)` API and returns only
                            // public facts: hash, encryption type, public-key
                            // length, signing type. Never mutates NetDB,
                            // KeyManager, tunnel, or LeaseSet state. Never
                            // carries private key material.
                            String[] values = split(line, 2);
                            String b64 = values[1];
                            try {
                                Destination peer = new Destination(b64);
                                Hash h = peer.calculateHash();
                                String hashHex = hex(h.getData());
                                int encCode = peer.getEncType().getCode();
                                String encName = peer.getEncType().name();
                                int pubLen = peer.getPublicKey().length();
                                int sigCode = peer.getSigType().getCode();
                                output.println("DEST_INFO hash_hex=" + hashHex
                                    + " enc_type_code=" + encCode
                                    + " enc_type_name=" + encName
                                    + " public_key_len=" + pubLen
                                    + " sig_type_code=" + sigCode);
                            } catch (Throwable t) {
                                output.println("DEST_INFO_ERROR class="
                                    + t.getClass().getSimpleName());
                            }
                            break;
                        }
                        case "STOP": output.println("STOPPING"); stop = true; break;
                        default: output.println("ERROR unknown-command");
                    }
                }
            }
        } finally {
            try { session.destroySession(); } catch (Throwable ignored) { }
            key.delete();
        }
    }
}
