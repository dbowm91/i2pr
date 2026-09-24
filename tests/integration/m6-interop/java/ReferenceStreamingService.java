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
    // Plan 246 §6 — exact-socket correlation. After `accept()` returns
    // the helper derives the remote peer b32 from the public I2PSocket
    // surface (`socket.getPeerDestination().toBase32()`) and stores it
    // here. All `REPORT_TIMER_STATS` snapshots filter the bounded public
    // LogManager console buffer down to lines that match
    // `event on [Connection ... from <EXPECTED_PEER_B32> up ...`. The
    // peer string is never written to durable evidence; only sanitized
    // counts and bounded numeric delays are exposed.
    private static volatile String EXPECTED_PEER_B32 = "";
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
    // Plan 241 §7 — active SessionConfig tunnel profile facts. Bounded
    // small integers and booleans only; never the explicit-peer value,
    // keys, tags, or payloads. Lets the lane prove the corrected
    // one-hop settings are active before the client-pair gate.
    private static volatile String PROFILE_INBOUND_LENGTH = "unknown";
    private static volatile String PROFILE_OUTBOUND_LENGTH = "unknown";
    private static volatile String PROFILE_INBOUND_ALLOW_ZERO_HOP = "unknown";
    private static volatile String PROFILE_OUTBOUND_ALLOW_ZERO_HOP = "unknown";
    private static volatile boolean PROFILE_EXPLICIT_PEERS_SET = false;

    // Plan 237 §4–§5 — stock helper-JVM response observation. All facts
    // are bounded public counts from the exact-pinned implementation:
    // `stream.con.sendMessageSize` lifetime events via the public
    // `StatManager.getRate(...).getLifetimeEventCount()`, plus exact
    // source-locked DEBUG/WARN substrings scanned from the public
    // `LogManager.getBuffer().getMostRecentMessages()` console buffer.
    // No packet contents, keys, tags, plaintext payloads, or private
    // state. Counts are bounded by the console buffer size (512) and
    // MAX_OBSERVATIONS.
    // Plan 245 §4 — buffer growth: the bounded console buffer must be
    // proven sufficient for the Plan-237 retained signals plus the
    // seven new bounded Plan-245 epoch signals
    // (scheduler_send_branch, scheduler_reschedule_branch,
    // scheduler_no_unacked_warning, message_output_flush_nonempty,
    // receiver_do_send_false, receiver_packet_built,
    // connection_resend_timer). The Plan-237 512-line buffer is
    // already shared across an unrelated consumer
    // (`accept_one`-style accept observability) and every observation
    // runs through the bounded `p237CountBufferSubstring` /
    // `p245CountBufferSubstring` counters that statically cap each
    // needle at MAX_OBSERVATIONS (1024). The helper increases the
    // buffer to 1024 so the seven added Plan-245 needles share the
    // same bounded window without evicting the Plan-237 needles. No
    // packet content, key, tag, payload, or private state crosses
    // the bound; only the bounded public LogManager console buffer.
    private static final int P237_CONSOLE_BUFFER_SIZE = 1024;
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
    // Plan 245 §4 — additional bounded observation classes the
    // stock-helper needs to surface the direct construction signal.
    // The Plan-237 `Connection` and `PacketQueue` DEBUG levels stay
    // set (they cover the retransmit-timer `Resend in` log and the
    // `sendMessage` lifetime stat); `ConnectionDataReceiver` exposes
    // the authoritative `New OB pkt (acks not yet filled in)` DEBUG
    // log emitted at the end of `buildPacket()` for every packet the
    // JVM constructs, and `MessageOutputStream` exposes the
    // `flushAvailable()` INFO log emitted when `_valid > 0` so the
    // observer can correlate the upstream flush call with the
    // downstream writeData decision.
    private static final String P245_RECEIVER_CLASS =
        "net.i2p.client.streaming.impl.ConnectionDataReceiver";
    private static final String P245_MESSAGE_OUTPUT_CLASS =
        "net.i2p.client.streaming.impl.MessageOutputStream";
    // Plan 246 §3.7 — exact-pinned SimpleTimer2 lifecycle marker classes
    // and substrings. The transition `addEvent(SimpleTimer.TimedEvent, ...)`
    // path emits DEBUG-level `Scheduling: ...`, `Running: ...`,
    // `Early execution, Rescheduling ...`, and `Execution finished in ...`
    // lines via the `net.i2p.util.SimpleTimer2` logger; the wrapper
    // `toString()` delegates to the inner Connection.ConEvent.toString(),
    // which contains the peer b32. The helper never persists the peer
    // string or raw line; it only counts lines that match both the
    // lifecycle needle and the exact-socket correlation marker.
    private static final String P246_SIMPLE_TIMER_CLASS =
        "net.i2p.util.SimpleTimer2";
    private static final String P246_TIMER_SCHEDULING_NEEDLE =
        "Scheduling: ";
    private static final String P246_TIMER_RUNNING_NEEDLE =
        "Running: ";
    private static final String P246_TIMER_EARLY_RESCHED_NEEDLE =
        "Early execution, Rescheduling for ";
    private static final String P246_TIMER_FINISHED_NEEDLE =
        "Execution finished in ";
    private static final String P246_CON_EVENT_PREFIX =
        "event on [Connection ";
    // Marker sub-string inside Connection.toString() that the helper
    // derives from the accepted I2PSocket peer destination
    // (`socket.getPeerDestination().toBase32()`). Connection.toString()
    // renders `[Connection <x>/<y> from <b32> up ...]`, so
    // `" from " + PEER_B32 + " up "` uniquely identifies the active
    // socket without leaking the b32 into durable evidence.
    private static final String P246_PEER_MARKER_PREFIX = " from ";
    private static final String P246_PEER_MARKER_SUFFIX = " up ";
    // Plan 246 §3.7 — the timeout delimiter appears only on the
    // SimpleTimer2 `Scheduling:` log lines; the helper extracts the
    // bounded numeric delay for the per-poll rolling maxima without
    // ever reading any other token from the line.
    private static final String P246_TIMEOUT_TOKEN = " timeout = ";
    private static final String P246_RESCHEDULE_DELTA_TOKEN =
        "Rescheduling for ";
    private static final String P246_FINISHED_DELTA_TOKEN =
        "Execution finished in ";
    // Plan 246 §3.8 — I2PAppContext.clock().now() is the helper-clock
    // view; the SimpleTimer2 timer is the system-clock executor. The
    // helper exposes the difference as a diagnostic-only numeric fact.
    // The drift itself is informational only and never authorizes a
    // Java patch or production i2pr change.
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
    // Plan 245 §3.1 / §3.4 — the exact-pinned source-locked
    // observation needles that supersede only Plan 244's
    // `Resend in`-based construction proxy. Each needle is a
    // bounded, public, sanitized substring from the exact-pinned
    // implementation; never a packet dump, key, tag, plaintext
    // payload, or private field. The Plan-237 `received con... `
    // and `Resend in ` needles stay live for the retained
    // Plan-237/244 chain.
    private static final String P245_SCHEDULER_SEND_BRANCH =
        "received con... send a packet";
    private static final String P245_SCHEDULER_RESCHEDULE_BRANCH =
        "received con... time till next send: ";
    private static final String P245_SCHEDULER_NO_UNACKED =
        "hmm, state is received, but no unacked packets received?";
    private static final String P245_MESSAGE_OUTPUT_FLUSH =
        "flushAvailable() valid = ";
    private static final String P245_RECEIVER_DOSEND_FALSE =
        "writeData called: size=";
    private static final String P245_RECEIVER_PACKET_BUILT =
        "New OB pkt (acks not yet filled in): ";

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
            // Plan 245 §4 — DEBUG for ConnectionDataReceiver so the
            // direct construction signal `New OB pkt (acks not yet
            // filled in): ...` is captured at the source-locked
            // site. INFO for MessageOutputStream so the
            // `flushAvailable() valid = <n>` log fires only when
            // the upstream flush carries data; both stay bounded by
            // the console buffer and MAX_OBSERVATIONS, never touch
            // private state.
            limits.setProperty(P245_RECEIVER_CLASS, "DEBUG");
            limits.setProperty(P245_MESSAGE_OUTPUT_CLASS, "INFO");
            // Plan 246 §3.7 / §5.2 — DEBUG for `net.i2p.util.SimpleTimer2`
            // surfaces the four bounded lifecycle markers
            // (`Scheduling:`, `Running:`, `Early execution, Rescheduling`,
            // `Execution finished in`) emitted by the inner
            // `SimpleTimer2.TimedEvent.schedule/run/finish` paths. The
            // `addEvent(SimpleTimer.TimedEvent, timeoutMs)` transition
            // path schedules a fresh wrapper, so the toString-derived
            // `event on [Connection ... from <peer> up ...]` line is the
            // only stable correlation identifier. The exact-socket
            // filter (Plan 246 §6) lives in `REPORT_TIMER_STATS`. The
            // bounded console buffer 1024 covers both the retained
            // Plan 237/245 needles and the new Plan 246 lifecycle
            // markers within the two-second attribution horizon; every
            // counter caps at MAX_OBSERVATIONS (1024).
            limits.setProperty(P246_SIMPLE_TIMER_CLASS, "DEBUG");
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

    private static boolean p245ReceiverDebugEnabled() {
        return p237IsDebugEnabledFor(P245_RECEIVER_CLASS);
    }

    private static boolean p245MessageOutputEnabled() {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null) return false;
            Properties limits = context.logManager().getLimits();
            if (limits == null) return false;
            String level = limits.getProperty(P245_MESSAGE_OUTPUT_CLASS);
            if (level == null) return false;
            // INFO or higher (DEBUG also counts). Strict parse: any
            // other value is treated as not enabled so the observation
            // cannot accidentally observe a downgraded level.
            return level.equalsIgnoreCase("INFO")
                || level.equalsIgnoreCase("DEBUG");
        } catch (Throwable ignored) {
            return false;
        }
    }

    private static int incrementBounded(AtomicInteger counter) {
        return counter.updateAndGet(value -> value < MAX_OBSERVATIONS ? value + 1 : value);
    }

    // Plan 246 §6 — exact-socket correlation for SimpleTimer2 lines.
    // Returns true iff the line contains both the lifecycle marker
    // (`event on [Connection `) and the active socket's
    // `from <peerB32> up ` marker, with peerB32 derived from the
    // public `I2PSocket.getPeerDestination().toBase32()` surface. The
    // helper never writes the peer string to durable evidence; it only
    // uses the marker as an in-memory filter. A missing peer (no accept
    // yet) makes the filter strict — the marker must be present.
    private static boolean p246LineMatchesActiveSocket(String line) {
        if (line == null) return false;
        String peer = EXPECTED_PEER_B32;
        if (peer == null || peer.isEmpty()) return false;
        if (!line.contains(P246_CON_EVENT_PREFIX)) return false;
        String marker = P246_PEER_MARKER_PREFIX + peer + P246_PEER_MARKER_SUFFIX;
        return line.contains(marker);
    }

    private static int p246CountBufferSubstringExactSocket(String needle) {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null
                || context.logManager().getBuffer() == null) return 0;
            int count = 0;
            for (String message : context.logManager().getBuffer().getMostRecentMessages()) {
                if (message == null) continue;
                if (!p246LineMatchesActiveSocket(message)) continue;
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

    // Plan 246 §3.7 — extract the bounded numeric timeout from a
    // `Scheduling: ... timeout = <n> state: ...` line. Returns -1
    // for any line that does not match the exact format; the helper
    // never throws and never persists raw line content.
    private static long p246ExtractTimeoutMs(String line) {
        if (line == null) return -1L;
        int idx = line.indexOf(P246_TIMEOUT_TOKEN);
        if (idx < 0) return -1L;
        int start = idx + P246_TIMEOUT_TOKEN.length();
        int end = start;
        while (end < line.length()) {
            char c = line.charAt(end);
            if (c < '0' || c > '9') break;
            end++;
        }
        if (end == start) return -1L;
        try {
            return Long.parseLong(line.substring(start, end));
        } catch (NumberFormatException nfe) {
            return -1L;
        }
    }

    private static long p246ExtractTrailingLong(String line, String token) {
        if (line == null) return -1L;
        int idx = line.indexOf(token);
        if (idx < 0) return -1L;
        int start = idx + token.length();
        int end = start;
        while (end < line.length()) {
            char c = line.charAt(end);
            if (c < '0' || c > '9') break;
            end++;
        }
        if (end == start) return -1L;
        try {
            return Long.parseLong(line.substring(start, end));
        } catch (NumberFormatException nfe) {
            return -1L;
        }
    }

    // Plan 246 §3.7 — return the bounded per-poll rolling maxima /
    // first-seen timestamps for the four SimpleTimer2 lifecycle markers.
    // The driver tracks rolling maxima across polls; the helper exposes
    // the bounded numeric facts only.
    private static long[] p246CollectTimeouts(boolean matches) {
        // Return shape: [count, first, latest, min, max]
        long count = 0;
        long first = -1L;
        long latest = -1L;
        long min = -1L;
        long max = -1L;
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null
                || context.logManager().getBuffer() == null) {
                return new long[]{0, -1, -1, -1, -1};
            }
            for (String message : context.logManager().getBuffer().getMostRecentMessages()) {
                if (message == null) continue;
                if (!matches && !message.contains(P246_TIMER_SCHEDULING_NEEDLE)) continue;
                if (matches && !p246LineMatchesActiveSocket(message)) continue;
                if (!message.contains(P246_TIMER_SCHEDULING_NEEDLE)) continue;
                long t = p246ExtractTimeoutMs(message);
                if (t < 0) continue;
                count++;
                if (first < 0) first = t;
                latest = t;
                if (min < 0 || t < min) min = t;
                if (t > max) max = t;
                if (count >= MAX_OBSERVATIONS) break;
            }
        } catch (Throwable ignored) { }
        return new long[]{count, first, latest, min, max};
    }

    private static long[] p246CollectRescheduleDeltas() {
        // Return shape: [count, first, latest, min, max]
        long count = 0;
        long first = -1L;
        long latest = -1L;
        long min = -1L;
        long max = -1L;
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null
                || context.logManager().getBuffer() == null) {
                return new long[]{0, -1, -1, -1, -1};
            }
            for (String message : context.logManager().getBuffer().getMostRecentMessages()) {
                if (message == null) continue;
                if (!p246LineMatchesActiveSocket(message)) continue;
                if (!message.contains(P246_TIMER_EARLY_RESCHED_NEEDLE)) continue;
                long d = p246ExtractTrailingLong(message, P246_RESCHEDULE_DELTA_TOKEN);
                if (d < 0) continue;
                count++;
                if (first < 0) first = d;
                latest = d;
                if (min < 0 || d < min) min = d;
                if (d > max) max = d;
                if (count >= MAX_OBSERVATIONS) break;
            }
        } catch (Throwable ignored) { }
        return new long[]{count, first, latest, min, max};
    }

    private static long[] p246CollectFinishedElapsed() {
        // Return shape: [count, first, latest, min, max]
        long count = 0;
        long first = -1L;
        long latest = -1L;
        long min = -1L;
        long max = -1L;
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null
                || context.logManager().getBuffer() == null) {
                return new long[]{0, -1, -1, -1, -1};
            }
            for (String message : context.logManager().getBuffer().getMostRecentMessages()) {
                if (message == null) continue;
                if (!p246LineMatchesActiveSocket(message)) continue;
                if (!message.contains(P246_TIMER_FINISHED_NEEDLE)) continue;
                long d = p246ExtractTrailingLong(message, P246_FINISHED_DELTA_TOKEN);
                if (d < 0) continue;
                count++;
                if (first < 0) first = d;
                latest = d;
                if (min < 0 || d < min) min = d;
                if (d > max) max = d;
                if (count >= MAX_OBSERVATIONS) break;
            }
        } catch (Throwable ignored) { }
        return new long[]{count, first, latest, min, max};
    }

    // Plan 246 §14 — clock-skew observation. The helper-clock view
    // (`I2PAppContext.clock().now()`) and the SimpleTimer2 executor
    // clock (`System.currentTimeMillis()`) may diverge. The helper
    // exposes the bounded numeric difference for the polling horizon;
    // it never adjusts clocks and never authorizes a Java patch.
    private static long p246ClockSkewMs() {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null) return 0L;
            long helper = context.clock().now();
            long system = System.currentTimeMillis();
            return helper - system;
        } catch (Throwable ignored) {
            return 0L;
        }
    }

    private static boolean p246SimpleTimerDebugEnabled() {
        return p237IsDebugEnabledFor(P246_SIMPLE_TIMER_CLASS);
    }

    // Plan 247 §11 — bounded console-buffer entry count. The helper
    // returns the size of the bounded public LogManager console
    // buffer (or 0 when the buffer is unavailable). Plan 247 §11
    // forbids increasing the buffer preemptively; the field stays
    // bounded by `P237_CONSOLE_BUFFER_SIZE` (1024).
    private static long p246BufferEntryCount() {
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null
                || context.logManager().getBuffer() == null) {
                return 0L;
            }
            return context.logManager().getBuffer().getMostRecentMessages().size();
        } catch (Throwable ignored) {
            return 0L;
        }
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

    private static String explicitPeerB64OrNull(String explicitArg) {        // Plan 241 WP — Router-C explicit peer for the genuine one-hop
        // Streaming client tunnel. Mirrors the already-proven optional
        // explicit-peer contract in ReferenceRawDestination: scoped
        // strictly to this Streaming client's inbound/outbound
        // SessionConfig; never a router-global property. Validated with
        // Java's own I2P Base64 decoder via `new Hash(decoded)` shape
        // (exact 32 bytes); any mismatch is a hard argument error.
        String candidate = null;
        if (explicitArg != null && !explicitArg.trim().isEmpty()) {
            candidate = explicitArg.trim();
        } else {
            String env = System.getenv("I2PR_M6_JAVA_EXPLICIT_PEER_B64");
            if (env != null && !env.trim().isEmpty()) {
                candidate = env.trim();
            }
        }
        if (candidate == null) {
            return null;
        }
        if (candidate.length() != 44 && candidate.length() != 43) {
            throw new IllegalArgumentException("explicit peer must be 43-44 char I2P Base64");
        }
        for (int i = 0; i < candidate.length(); i++) {
            char c = candidate.charAt(i);
            boolean ok = (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z')
                || (c >= '0' && c <= '9') || c == '-' || c == '~' || c == '=';
            if (!ok) {
                throw new IllegalArgumentException("explicit peer has non-I2P-Base64 character");
            }
        }
        try {
            byte[] raw = net.i2p.data.Base64.decode(candidate);
            if (raw == null || raw.length != 32) {
                throw new IllegalArgumentException("explicit peer must decode to 32 bytes");
            }
            new net.i2p.data.Hash(raw);
        } catch (RuntimeException re) {
            throw new IllegalArgumentException("explicit peer is not a valid RouterHash");
        }
        return candidate;
    }

    private static Properties options(String host, int port) {
        return options(host, port, null);
    }

    private static Properties options(String host, int port, String explicitPeerB64) {
        Properties options = new Properties();
        options.setProperty("i2cp.tcp.host", host);
        options.setProperty("i2cp.tcp.port", Integer.toString(port));
        if (explicitPeerB64 != null) {
            // Plan 241 WP — genuine stock-Java one-hop Streaming client
            // tunnel through controlled Router C. Ordinary public I2CP
            // SessionConfig options only; no router-global explicitPeers,
            // no profile/tier mutation, no NetDB injection, no VMComm.
            // Lease-set type, encryption type, publication flags, message
            // reliability, Destination generation, Streaming behavior, and
            // response scheduling are unchanged from the zero-hop profile.
            options.setProperty("inbound.length", "1");
            options.setProperty("outbound.length", "1");
            options.setProperty("inbound.quantity", "1");
            options.setProperty("outbound.quantity", "1");
            options.setProperty("inbound.backupQuantity", "0");
            options.setProperty("outbound.backupQuantity", "0");
            options.setProperty("inbound.allowZeroHop", "false");
            options.setProperty("outbound.allowZeroHop", "false");
            options.setProperty("inbound.explicitPeers", explicitPeerB64);
            options.setProperty("outbound.explicitPeers", explicitPeerB64);
        } else {
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
        }
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
            // Plan 246 §6 — derive the accepted socket's remote peer b32
            // from the public I2PSocket surface for exact-socket
            // correlation of SimpleTimer2 lifecycle markers. The peer
            // string is held only in memory and is never written to
            // durable evidence. Stock Java's I2PSocket.getPeerDestination()
            // may return null at the moment accept() returns because the
            // SYN packet has not yet been fully parsed; the helper
            // therefore tries the eager fetch first and keeps the lazy
            // fetch on REPORT_TIMER_STATS as a fallback. A missing peer
            // (no remote Destination known yet) leaves the previous marker
            // in place, so any subsequent report continues to use the
            // last successful accept's peer.
            try {
                net.i2p.data.Destination peer = socket.getPeerDestination();
                if (peer != null) {
                    EXPECTED_PEER_B32 = peer.toBase32();
                }
            } catch (Throwable ignored) { }
            return id;
        } catch (Throwable error) {
            incrementBounded(ACCEPT_ERRORS);
            if (surfaceEntered) {
                incrementBounded(SOCKET_SURFACE_ERRORS);
            }
            return -1;
        }
    }

    // Plan 246 §6 — lazy peer-b32 fetch on REPORT_TIMER_STATS. The
    // eager fetch inside acceptOne() may race with stock Java's
    // accept() surface state; if it returns null at accept time the
    // marker is empty and exact-socket correlation is impossible.
    // The lazy fetch re-derives the marker on every report call so
    // the FIRST report after the peer becomes available captures it.
    // The `LogWriter` background thread periodically calls
    // `rereadConfig()` which CLEARS the existing limits (including
    // the SimpleTimer2 DEBUG level we set in
    // `p237ConfigureStockObserver`); the helper therefore re-applies
    // the SimpleTimer2 limit on every REPORT_TIMER_STATS call so the
    // exact-socket-filtered lifecycle markers stay observable across
    // the 2 s attribution horizon. The re-apply is unconditional — it
    // runs even when ACCEPTED is empty (the very first polls happen
    // before the SYN) — because the LogWriter may have cleared the
    // limits between startup and the first report.
    private static void p246RefreshPeerMarker() {
        try {
            if (!ACCEPTED.isEmpty()) {
                int id;
                synchronized (ACCEPTED) {
                    id = ACCEPTED.get(0);
                }
                I2PSocket active = SOCKETS.get(id);
                if (active != null) {
                    net.i2p.data.Destination peer = active.getPeerDestination();
                    if (peer != null) {
                        EXPECTED_PEER_B32 = peer.toBase32();
                    }
                }
            }
        } catch (Throwable ignored) { }
        // Plan 246 §17 — re-apply the SimpleTimer2 DEBUG level on
        // every REPORT_TIMER_STATS call. The `LogWriter` background
        // thread periodically calls `rereadConfig()` which CLEARS
        // the existing limits (including the SimpleTimer2 DEBUG
        // level we set in `p237ConfigureStockObserver`); the bounded
        // `setLimits` is idempotent and keeps the exact-socket-
        // filtered lifecycle markers observable across the 2 s
        // attribution horizon. We pass the full limits object so
        // the other previously-set DEBUG levels are also re-applied.
        try {
            I2PAppContext context = I2PAppContext.getGlobalContext();
            if (context == null || context.logManager() == null) return;
            Properties limits = new Properties();
            limits.setProperty(P237_SCHEDULER_IMPL_CLASS, "DEBUG");
            limits.setProperty(P237_SCHEDULER_CLASS, "DEBUG");
            limits.setProperty(P237_CONNECTION_CLASS, "DEBUG");
            limits.setProperty(P237_PACKETQUEUE_CLASS, "DEBUG");
            limits.setProperty(P245_RECEIVER_CLASS, "DEBUG");
            limits.setProperty(P245_MESSAGE_OUTPUT_CLASS, "INFO");
            limits.setProperty(P246_SIMPLE_TIMER_CLASS, "DEBUG");
            context.logManager().setLimits(limits);
        } catch (Throwable ignored) { }
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 4 && args.length != 5) throw new IllegalArgumentException("usage: host i2cpPort controlPort keyFile [explicitPeerB64]");
        String host = args[0];
        int i2cpPort = Integer.parseInt(args[1]);
        int controlPort = Integer.parseInt(args[2]);
        File key = new File(args[3]);
        String explicitPeerB64 = explicitPeerB64OrNull(args.length >= 5 ? args[4] : null);
        I2PClient client = I2PClientFactory.createClient();
        createDestination(client, key);
        Properties sessionOptions = options(host, i2cpPort, explicitPeerB64);
        PROFILE_INBOUND_LENGTH = sessionOptions.getProperty("inbound.length", "unknown");
        PROFILE_OUTBOUND_LENGTH = sessionOptions.getProperty("outbound.length", "unknown");
        PROFILE_INBOUND_ALLOW_ZERO_HOP = sessionOptions.getProperty("inbound.allowZeroHop", "unknown");
        PROFILE_OUTBOUND_ALLOW_ZERO_HOP = sessionOptions.getProperty("outbound.allowZeroHop", "unknown");
        PROFILE_EXPLICIT_PEERS_SET = explicitPeerB64 != null;
        I2PSocketManager manager;
        try (FileInputStream input = new FileInputStream(key)) {
            manager = I2PSocketManagerFactory.createDisconnectedManager(input, host, i2cpPort, sessionOptions);
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
                        case "REPORT_TUNNEL_PROFILE": {
                            // Plan 241 §7 — active SessionConfig tunnel
                            // profile facts. Bounded lengths/booleans only;
                            // never the explicit-peer value, keys, tags, or
                            // payloads. The lane requires the one-hop
                            // profile (length 1, allowZeroHop false,
                            // explicit peer set) before the client-pair
                            // gate; a zero-hop report on the counted lane
                            // is a deterministic fixture defect, never a
                            // counted build terminal.
                            output.println("TUNNEL_PROFILE inbound_length=" + PROFILE_INBOUND_LENGTH
                                + " outbound_length=" + PROFILE_OUTBOUND_LENGTH
                                + " inbound_allow_zero_hop=" + PROFILE_INBOUND_ALLOW_ZERO_HOP
                                + " outbound_allow_zero_hop=" + PROFILE_OUTBOUND_ALLOW_ZERO_HOP
                                + " explicit_peers_set=" + PROFILE_EXPLICIT_PEERS_SET);
                            break;
                        }
                        case "REPORT_STATUS": {                            // Plan 200 §A.2 — explicit, bounded status
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
                        case "REPORT_TIMER_STATS": {
                            // Plan 246 §7 — bounded SimpleTimer2 lifecycle
                            // + exact-socket correlation snapshot. Every
                            // counter is bounded by MAX_OBSERVATIONS (1024)
                            // and only numeric/count/bool fields are
                            // emitted; the peer string is never written.
                            // The driver polls this command ~50 ms × 40
                            // times across a two-second attribution
                            // horizon and keeps rolling maxima in
                            // memory; the final 45 s outer lane keeps the
                            // retained Plan-237/245 snapshot for
                            // compatibility.
                            //
                            // Plan 247 §11 — the response now also carries
                            // three console-buffer-pressure fields
                            // (`console_buffer_entries`,
                            // `console_buffer_capacity`, and
                            // `console_buffer_at_capacity`). The capacity
                            // mirrors the configured
                            // `P237_CONSOLE_BUFFER_SIZE` constant (1024)
                            // and `at_capacity` flips to `true` when the
                            // bounded entry count meets or exceeds it.
                            // Plan 247 §11 forbids increasing the buffer
                            // preemptively; the pressure fields gate the
                            // classifier's absence inference only.
                            p246RefreshPeerMarker();
                            long[] schedTimeouts = p246CollectTimeouts(true);
                            long[] early = p246CollectRescheduleDeltas();
                            long[] finished = p246CollectFinishedElapsed();
                            int runningCount =
                                p246CountBufferSubstringExactSocket(P246_TIMER_RUNNING_NEEDLE);
                            int schedulingCount = (int) schedTimeouts[0];
                            int earlyRescheduleCount = (int) early[0];
                            int finishedCount = (int) finished[0];
                            long firstScheduleTimeout = schedTimeouts[1];
                            long latestScheduleTimeout = schedTimeouts[2];
                            long minScheduleTimeout = schedTimeouts[3];
                            long maxScheduleTimeout = schedTimeouts[4];
                            long firstRescheduleDelta = early[1];
                            long latestRescheduleDelta = early[2];
                            long minRescheduleDelta = early[3];
                            long maxRescheduleDelta = early[4];
                            long firstFinishedElapsed = finished[1];
                            long clockSkew = p246ClockSkewMs();
                            boolean simpleTimerDebug = p246SimpleTimerDebugEnabled();
                            long bufferEntries = p246BufferEntryCount();
                            long bufferCapacity = P237_CONSOLE_BUFFER_SIZE;
                            boolean bufferAtCapacity = bufferEntries >= bufferCapacity;
                            output.println("TIMER_STATS peer_correlation_present="
                                + (EXPECTED_PEER_B32 != null && !EXPECTED_PEER_B32.isEmpty())
                                + " simple_timer_debug_enabled=" + simpleTimerDebug
                                + " timer_scheduler_count=" + schedulingCount
                                + " timer_running_count=" + runningCount
                                + " timer_early_reschedule_count=" + earlyRescheduleCount
                                + " timer_finished_count=" + finishedCount
                                + " connection_timer_first_schedule_timeout_ms=" + firstScheduleTimeout
                                + " connection_timer_latest_schedule_timeout_ms=" + latestScheduleTimeout
                                + " connection_timer_min_schedule_timeout_ms=" + minScheduleTimeout
                                + " connection_timer_max_schedule_timeout_ms=" + maxScheduleTimeout
                                + " first_reschedule_delta_ms=" + firstRescheduleDelta
                                + " latest_reschedule_delta_ms=" + latestRescheduleDelta
                                + " min_reschedule_delta_ms=" + minRescheduleDelta
                                + " max_reschedule_delta_ms=" + maxRescheduleDelta
                                + " connection_timer_first_run_elapsed_ms=" + firstFinishedElapsed
                                + " context_clock_minus_system_ms=" + clockSkew
                                + " java_source_pin=" + JAVA_SOURCE_PIN
                                + " console_buffer_entries=" + bufferEntries
                                + " console_buffer_capacity=" + bufferCapacity
                                + " console_buffer_at_capacity=" + bufferAtCapacity);
                            break;
                        }
                        case "REPORT_RESPONSE_STATS": {
                            // Plan 237 §5 — bounded public stock facts for
                            // the isolated SYN epoch. The Rust driver
                            // snapshots this command before the SYN and at
                            // the end of the frozen response window, then
                            // classifies from deltas (never absolutes).
                            // Plan 245 §5 — extends the bounded response
                            // snapshot with the seven direct-attribution
                            // needles the Plan-244 retransmit-timer proxy
                            // could not discriminate. Counts are still
                            // bounded public substrings; never packet
                            // dumps, keys, tags, or payload bytes.
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
                            int schedulerSendBranch =
                                p237CountBufferSubstring(P245_SCHEDULER_SEND_BRANCH);
                            int schedulerRescheduleBranch =
                                p237CountBufferSubstring(P245_SCHEDULER_RESCHEDULE_BRANCH);
                            int schedulerNoUnacked =
                                p237CountBufferSubstring(P245_SCHEDULER_NO_UNACKED);
                            int messageOutputFlush =
                                p237CountBufferSubstring(P245_MESSAGE_OUTPUT_FLUSH);
                            int receiverDoSendFalse =
                                p237CountBufferSubstring(P245_RECEIVER_DOSEND_FALSE);
                            int receiverPacketBuilt =
                                p237CountBufferSubstring(P245_RECEIVER_PACKET_BUILT);
                            int connectionResendTimer =
                                p237CountBufferSubstring(P237_SENDPACKET_SIGNAL);
                            boolean receiverDebug =
                                p245ReceiverDebugEnabled();
                            boolean messageOutputEnabled =
                                p245MessageOutputEnabled();
                            output.println("RESPONSE_STATS scheduler_log_count=" + schedulerCount
                                + " ack_constructed_log_count=" + ackCount
                                + " send_message_size_lifetime_events=" + sendEvents
                                + " send_failure_count=" + sendFail
                                + " send_exception_count=" + sendException
                                + " scheduler_debug_enabled=" + schedulerDebug
                                + " connection_debug_enabled=" + connectionDebug
                                + " packetqueue_debug_enabled=" + packetqueueDebug
                                // Plan 245 §5 — bounded direct-attribution
                                // needles. `connection_resend_timer_delta`
                                // is the retained Plan-244 construction
                                // proxy; `receiver_packet_built_delta` is
                                // the new direct construction signal that
                                // proves construction even for ACK-only
                                // sequence-0 non-SYN packets.
                                + " scheduler_send_branch_log_count=" + schedulerSendBranch
                                + " scheduler_reschedule_branch_log_count=" + schedulerRescheduleBranch
                                + " scheduler_no_unacked_warning_log_count=" + schedulerNoUnacked
                                + " message_output_flush_nonempty_log_count=" + messageOutputFlush
                                + " receiver_do_send_false_log_count=" + receiverDoSendFalse
                                + " receiver_packet_built_log_count=" + receiverPacketBuilt
                                + " connection_resend_timer_log_count=" + connectionResendTimer
                                + " receiver_debug_enabled=" + receiverDebug
                                + " message_output_enabled=" + messageOutputEnabled);
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
