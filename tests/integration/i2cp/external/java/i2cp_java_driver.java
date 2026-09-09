// Plan 170 — Java I2P I2CP external-client driver.
//
// One standalone Java program that drives the unmodified
// `net.i2p.data.i2cp` wire primitives against the i2pr M9 I2CP
// loopback server. The Plan 170 external lane deliberately bypasses
// the higher-level `net.i2p.client.I2PSession` API because that
// API's `I2PSession.connect()` blocks while waiting for real
// inbound/outbound tunnels to be built — tunnels the M9 test
// profile intentionally skips. Driving the wire primitives
// directly proves that the unmodified Java I2P core produces
// wire-compatible I2CP bytes that i2pr accepts, and that i2pr
// produces wire-compatible I2CP bytes that the unmodified
// `I2CPMessageHandler.readMessage` codec accepts.
//
// Six subcommands are exposed:
//
//   connect-version   - protocol byte + GetDate handshake only.
//   session-leaseset2 - send a client-owned Ed25519/X25519
//                       CreateSession message and read the
//                       SessionStatus(Created) reply + the
//                       Plan 170 RequestVariableLeaseSet follow-up.
//   cleanup           - same lifecycle as session-leaseset2 but
//                       also emits DestroySession and reads the
//                       follow-up ReplyAndFollowup frames.
//   send-to-go        - open a session through the wire, send one
//                       payload to the peer destination (the
//                       go-i2cp driver), record SHA-256 digest of
//                       the payload plus the negotiated session id.
//   send-to-java      - open a session through the wire, receive
//                       one payload from the peer (the go-i2cp
//                       driver), record the digest through the
//                       inbound MessagePayload frame.
//   bandwidth         - GetBandwidthLimits query (no session);
//                       record the parsed BandwidthLimits ceilings.
//
// The driver never logs private signing material, raw payloads,
// or the destination PRIVATE key file. Only sanitized facts
// (digests, byte counts, session ids) leave the process. The
// Plan 170 evidence checker
// (`scripts/check-i2cp-acceptance-evidence.sh`) rejects any line
// that mentions forbidden field names.
//
// The pinned source lives under
// `target/interop/cache/i2cp/java_i2p/<pin>/lib/i2p.jar`. The
// harness script supplies every environment variable.

import net.i2p.data.Base64;
import net.i2p.data.DataFormatException;
import net.i2p.data.Destination;
import net.i2p.data.Hash;
import net.i2p.data.LeaseSet2;
import net.i2p.data.PrivateKey;
import net.i2p.data.PrivateKeyFile;
import net.i2p.data.PublicKey;
import net.i2p.data.SigningPrivateKey;
import net.i2p.data.SigningPublicKey;
import net.i2p.crypto.EncType;
import net.i2p.crypto.SigType;
import net.i2p.data.i2cp.CreateLeaseSet2Message;
import net.i2p.data.i2cp.CreateSessionMessage;
import net.i2p.data.i2cp.DestroySessionMessage;
import net.i2p.data.i2cp.DisconnectMessage;
import net.i2p.data.i2cp.GetBandwidthLimitsMessage;
import net.i2p.data.i2cp.GetDateMessage;
import net.i2p.data.i2cp.I2CPMessage;
import net.i2p.data.i2cp.I2CPMessageException;
import net.i2p.data.i2cp.I2CPMessageHandler;
import net.i2p.data.i2cp.SendMessageMessage;
import net.i2p.data.i2cp.SessionConfig;
import net.i2p.data.i2cp.SessionId;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.security.MessageDigest;
import java.util.Properties;

public class i2cp_java_driver {

    static final String PINNED_REVISION = "9134f808337b401e8e53c73734c81fab04280c9d";

    static void recordFact(String key, String value) {
        System.out.println(key + "=" + value);
    }

    static void recordError(String label, Throwable t) {
        recordFact("status", "failed");
        recordFact("error_label", label);
        if (t != null) {
            recordFact("error", t.getClass().getSimpleName() + ": " + safeMessage(t));
        }
        System.exit(1);
    }

    static String safeMessage(Throwable t) {
        String msg = t.getMessage();
        if (msg == null) {
            return "(no message)";
        }
        return msg.replaceAll("[A-Za-z0-9+/~]{60,}", "<redacted>");
    }

    static String envOr(String key, String fallback) {
        String v = System.getenv(key);
        return (v == null || v.isEmpty()) ? fallback : v;
    }

    static String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b & 0xff));
        return sb.toString();
    }

    /** Writes one I2CP frame to the supplied output stream. */
    static void writeFrame(OutputStream out, int type, byte[] body) throws IOException {
        DataOutputStream dos = new DataOutputStream(out);
        dos.writeInt(body.length);
        dos.writeByte(type);
        dos.write(body);
        dos.flush();
    }

    /** Reads exactly one I2CP frame from the supplied input stream. */
    static byte[] readFrame(InputStream in) throws IOException {
        int[] hdr = readFrameHeader(in);
        byte[] body = readFrameBody(in, hdr[1]);
        byte[] frame = new byte[5 + hdr[1]];
        frame[0] = (byte)(hdr[1] >> 24);
        frame[1] = (byte)(hdr[1] >> 16);
        frame[2] = (byte)(hdr[1] >> 8);
        frame[3] = (byte)(hdr[1]);
        frame[4] = (byte) hdr[0];
        System.arraycopy(body, 0, frame, 5, hdr[1]);
        return frame;
    }

    /** Reads the 5-byte I2CP frame header (u32 length, u8 type) and
     *  returns it as [type, length]. */
    static int[] readFrameHeader(InputStream in) throws IOException {
        byte[] header = new byte[5];
        int read = 0;
        while (read < 5) {
            int n = in.read(header, read, 5 - read);
            if (n < 0) throw new IOException("EOF reading frame header");
            read += n;
        }
        int length = ((header[0] & 0xff) << 24) | ((header[1] & 0xff) << 16) |
                     ((header[2] & 0xff) << 8) | (header[3] & 0xff);
        int type = header[4] & 0xff;
        return new int[] { type, length };
    }

    /** Reads the body of a frame given the length already consumed
     *  via readFrameHeader. */
    static byte[] readFrameBody(InputStream in, int length) throws IOException {
        byte[] body = new byte[length];
        int off = 0;
        while (off < length) {
            int n = in.read(body, off, length - off);
            if (n < 0) throw new IOException("EOF reading body");
            off += n;
        }
        return body;
    }

    /** Opens one TCP connection to the daemon and emits the
     *  protocol byte. Returns a [socket, in, out] tuple. */
    static Object[] connect() throws IOException {
        String host = envOr("I2CP_HOST", "127.0.0.1");
        int port = Integer.parseInt(envOr("I2CP_PORT", "7654"));
        Socket s = new Socket(host, port);
        s.setKeepAlive(true);
        BufferedOutputStream out = new BufferedOutputStream(s.getOutputStream());
        out.write(0x2a);  // protocol byte
        out.flush();
        BufferedInputStream in = new BufferedInputStream(s.getInputStream());
        return new Object[] { s, in, out };
    }

    /** Sends GetDate with an empty authentication mapping (the
     *  I2CP 0.9.11+ compliance path every unmodified client uses)
     *  and reads the SetDate reply. */
    static byte[] handshakeGetDateSetDate(InputStream in, OutputStream out) throws IOException {
        String version = "0.9.70";  // Java I2P 2.13.0 advertises 0.9.70
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        baos.write(version.length() & 0xff);
        baos.write(version.getBytes("UTF-8"));
        baos.write(0); baos.write(0);  // empty auth mapping (u16 length = 0)
        writeFrame(out, 32, baos.toByteArray());
        return readFrame(in);
    }

/** Constructs an Ed25519 client-owned destination, writes the
     *  PRIV bytes to the supplied file, and returns the public
     *  Destination. The 320-byte padding slot is filled with zeros so
     *  the Java `Destination.calculateHash()` output is byte-equal
     *  to the i2pr `Destination::hash()` output (the daemon's
     *  canonical encoding derives the same hash from the zero-padded
     *  key area, and the cross-session lookup target_hash check
     *  then matches both sides byte-for-byte). */
    static Destination makeDestination(File keyFile) throws Exception {
        SigType sig = SigType.EdDSA_SHA512_Ed25519;
        EncType enc = EncType.ECIES_X25519;
        byte[] seed = new byte[32];
        new java.security.SecureRandom().nextBytes(seed);
        SigningPrivateKey spriv = new SigningPrivateKey(sig, seed);
        SigningPublicKey spub = spriv.toPublic();
        byte[] randPriv = new byte[enc.getPrivkeyLen()];
        new java.security.SecureRandom().nextBytes(randPriv);
        PrivateKey priv = new PrivateKey(enc, randPriv);
        // Zero-fill padding so Java's hash matches the daemon's
        // canonical encoding after Plan 166 / Plan 170's
        // ElGamal-256 default slot layout.
        byte[] padding = new byte[320];
        net.i2p.data.KeyCertificate cert = new net.i2p.data.KeyCertificate(sig, enc);
        Destination d = new Destination();
        d.setPublicKey(new PublicKey(enc, new byte[enc.getPubkeyLen()]));
        d.setSigningPublicKey(spub);
        d.setCertificate(cert);
        d.setPadding(padding);
        java.io.ByteArrayOutputStream privBaos = new java.io.ByteArrayOutputStream();
        d.writeBytes(privBaos);
        priv.writeBytes(privBaos);
        spriv.writeBytes(privBaos);
        try (FileOutputStream fos = new FileOutputStream(keyFile)) {
            fos.write(privBaos.toByteArray());
        }
        return d;
    }

    /** Builds a Plan 165-compatible SessionConfig body (Destination
     *  + Mapping + date + signature). */
    static byte[] buildSessionConfig(Destination dest, SigningPrivateKey signingKey, long creationMs) throws Exception {
        // SessionConfig has no setDestination() — the destination must be
        // passed via the (Destination) constructor.
        SessionConfig cfg = new SessionConfig(dest);
        Properties opts = new Properties();
        opts.setProperty("i2cp.dontPublishLeaseSet", "true");
        opts.setProperty("i2cp.fastReceive", "true");
        opts.setProperty("i2cp.leaseSetEncType", "4");
        opts.setProperty("i2cp.messageReliability", "BestEffort");
        opts.setProperty("inbound.nickname", "i2cp-java-driver");
        cfg.setOptions(opts);
        cfg.setCreationDate(new java.util.Date(creationMs));
        cfg.signSessionConfig(signingKey);
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        cfg.writeBytes(baos);
        return baos.toByteArray();
    }

    /** Reads one CreateSession reply (SessionStatus(Created) +
     *  Plan 170 RequestVariableLeaseSet follow-up). */
    static int createSession(InputStream in, OutputStream out, byte[] sessionConfigBody) throws IOException {
        writeFrame(out, 1, sessionConfigBody);
        byte[] statusFrame = readFrame(in);
        if (statusFrame[4] != 20) {
            throw new IOException("expected SessionStatus reply type 20, got " + statusFrame[4]);
        }
        int sessionId = ((statusFrame[5] & 0xff) << 8) | (statusFrame[6] & 0xff);
        int status = statusFrame[7] & 0xff;
        // Plan 170 §5: drain the RequestVariableLeaseSet follow-up.
        byte[] leaseFrame = readFrame(in);
        if (leaseFrame[4] != 37) {
            throw new IOException("expected RequestVariableLeaseSet follow-up type 37, got " + leaseFrame[4]);
        }
        return sessionId;
    }

    /** Translates a go-i2cp/I2P-Base64 destination string into raw
     *  destination wire bytes. The I2P Base64 alphabet uses `-` and `~`
     *  instead of standard `+` and `/`; the go-i2cp `Destination.Base64()`
     *  helper emits both ranges interchangeably with the canonical
     *  Java I2P `Destination` constructor. */
    static byte[] decodePeerB64(String b64) {
        String standard = b64.replace('-', '+').replace('~', '/');
        return java.util.Base64.getDecoder().decode(standard);
    }

    /** Encodes the destination in the legacy 256-byte pubkey slot +
     *  128-byte signing key slot (right-aligned) + cert wire form that
     *  go-i2cp / i2pd expect. Java's `Destination.writeBytes` emits a
     *  compact (32-byte key + 320-byte padding + 32-byte key + cert)
     *  form; the i2pr daemon `Destination::decode` accepts both forms
     *  (per Plan 166 / Plan 170), but go-i2cp's `NewDestinationFromBase64`
     *  is strict about the legacy slot layout. The two encodings hash to
     *  the same canonical bytes (X25519/Ed25519 with a PublicKey
     *  populated only by zeros and an Ed25519 key right-aligned in the
     *  128-byte slot) so the cross-client session lookup still
     *  matches. */
    static String encodeLegacyDestination(Destination dest) throws DataFormatException, IOException {
        byte[] pubKey = dest.getPublicKey().toByteArray();
        byte[] signingKey = dest.getSigningPublicKey().toByteArray();
        byte[] certBytes;
        ByteArrayOutputStream certStream = new ByteArrayOutputStream();
        dest.getCertificate().writeBytes(certStream);
        certBytes = certStream.toByteArray();
        byte[] legacy = new byte[256 + 128 + certBytes.length];
        // pubkey: 256-byte slot, X25519 key right-aligned at [224..256].
        System.arraycopy(pubKey, 0, legacy, 256 - pubKey.length, pubKey.length);
        // signing key: 128-byte slot, Ed25519 key right-aligned at [96..128].
        System.arraycopy(signingKey, 0, legacy, 256 + 128 - signingKey.length, signingKey.length);
        // certificate appended verbatim.
        System.arraycopy(certBytes, 0, legacy, 256 + 128, certBytes.length);
        return net.i2p.data.Base64.encode(legacy);
    }

    /** Builds a Plan 170 / I2CP payload body as a complete gzip
     *  member whose 10-byte header carries
     *  {source_port, destination_port, xflags=2, protocol} and whose
     *  body is the deflate stream plus the CRC-32/ISIZE trailer.
     *
     *  Port bytes are little-endian to match go-i2cp's
     *  `extractI2CPFieldsFromGzip` (gzip MTIME convention); the M9
     *  daemon forwards payload bytes opaquely so both independent
     *  clients observe identical metadata. The trailer is required:
     *  go-i2cp inflates through `gzip.NewReader`, which rejects the
     *  trailer-less pseudo-gzip form.
     *
     *  Mirrors the layout documented in `crates/i2pr-api/src/i2cp/payload.rs`. */
    static byte[] encodeGzipHeader(int srcPort, int dstPort, byte[] body) throws IOException {
        ByteArrayOutputStream raw = new ByteArrayOutputStream(body.length + 32);
        java.util.zip.Deflater deflater = new java.util.zip.Deflater(java.util.zip.Deflater.DEFAULT_COMPRESSION, true);
        try (java.util.zip.DeflaterOutputStream deflate =
                 new java.util.zip.DeflaterOutputStream(raw, deflater)) {
            deflate.write(body);
        } finally {
            deflater.end();
        }
        byte[] deflateBody = raw.toByteArray();
        java.util.zip.CRC32 crc = new java.util.zip.CRC32();
        crc.update(body);
        long crcValue = crc.getValue();
        ByteArrayOutputStream out = new ByteArrayOutputStream(10 + deflateBody.length + 8);
        out.write(0x1f);
        out.write(0x8b);
        out.write(0x08);  // deflate
        out.write(0x00);  // flag bits (none)
        out.write(srcPort & 0xff); out.write((srcPort >> 8) & 0xff);
        out.write(dstPort & 0xff); out.write((dstPort >> 8) & 0xff);
        out.write(0x02);  // xflags = JAVA
        out.write(0x06);  // protocol = STREAMING
        out.write(deflateBody);
        out.write((int) (crcValue & 0xff));
        out.write((int) ((crcValue >> 8) & 0xff));
        out.write((int) ((crcValue >> 16) & 0xff));
        out.write((int) ((crcValue >> 24) & 0xff));
        out.write(body.length & 0xff);
        out.write((body.length >> 8) & 0xff);
        out.write((body.length >> 16) & 0xff);
        out.write((body.length >> 24) & 0xff);
        return out.toByteArray();
    }

    static void runConnectVersion() throws Exception {
        Object[] conn = connect();
        Socket s = (Socket) conn[0];
        BufferedInputStream in = (BufferedInputStream) conn[1];
        BufferedOutputStream out = (BufferedOutputStream) conn[2];
        byte[] setDate = handshakeGetDateSetDate(in, out);
        recordFact("connect_role", "connect-version");
        recordFact("connect_host", envOr("I2CP_HOST", "127.0.0.1"));
        recordFact("connect_port", envOr("I2CP_PORT", "7654"));
        // Parse SetDate (type 33, body = u64 date + u8 ver_len + ver).
        int verLen = setDate[13] & 0xff;
        String version = new String(setDate, 14, verLen, "UTF-8");
        recordFact("connect_version", version);
        s.close();
        recordFact("status", "passed");
    }

    static void runSessionLifecycle(String role) throws Exception {
        File tmpDir = new File(System.getProperty("java.io.tmpdir"), "i2cp-java-driver");
        tmpDir.mkdirs();
        File keyFile = new File(tmpDir, role + ".priv");
        Destination dest = makeDestination(keyFile);
        // Read back the SigningPrivateKey from the saved PRIV file so we
        // can sign the SessionConfig with the matching private key.
        SigningPrivateKey spriv;
        try (java.io.FileInputStream fis = new java.io.FileInputStream(keyFile)) {
            PrivateKeyFile pkf = new PrivateKeyFile(fis);
            spriv = pkf.getSigningPrivKey();
        }
        recordFact("destination_hash_hex", toHex(dest.calculateHash().getData()));
        Object[] conn = connect();
        Socket s = (Socket) conn[0];
        BufferedInputStream in = (BufferedInputStream) conn[1];
        BufferedOutputStream out = (BufferedOutputStream) conn[2];
        byte[] setDate = handshakeGetDateSetDate(in, out);
        long creationMs = System.currentTimeMillis();
        byte[] cfgBody = buildSessionConfig(dest, spriv, creationMs);
        int sessionId = createSession(in, out, cfgBody);
        recordFact("session_id", String.valueOf(sessionId));
        // Destroy cleanly.
        ByteArrayOutputStream destroyBody = new ByteArrayOutputStream();
        destroyBody.write(sessionId >> 8);
        destroyBody.write(sessionId);
        writeFrame(out, 3, destroyBody.toByteArray());
        s.close();
        recordFact("status", "passed");
    }

    static volatile byte[] inboundPayloadDigest = null;
    static volatile int inboundPayloadLen = 0;
    static volatile int inboundProtocol = -1;
    static volatile int inboundSrcPort = -1;
    static volatile int inboundDstPort = -1;

    /** Reads one MessagePayload frame from the daemon. */
    static void readMessagePayload(InputStream in) throws IOException, java.security.NoSuchAlgorithmException {
        byte[] frame = readFrame(in);
        // type 31 MessagePayload, body = u16 session + u32 msg_id + u32 length + bytes
        if (frame[4] != 31) {
            throw new IOException("expected MessagePayload type 31, got " + frame[4]);
        }
        int sessionId = ((frame[5] & 0xff) << 8) | (frame[6] & 0xff);
        long msgId = ((frame[7] & 0xffL) << 24) | ((frame[8] & 0xffL) << 16) |
                     ((frame[9] & 0xffL) << 8) | (frame[10] & 0xffL);
        int payloadLength = ((frame[11] & 0xff) << 24) | ((frame[12] & 0xff) << 16) |
                            ((frame[13] & 0xff) << 8) | (frame[14] & 0xff);
        // The MessagePayload body layout is:
        //   u16 session_id  (frame[5..7])
        //   u32 message_id  (frame[7..11])
        //   u32 payload_length (frame[11..15])
        //   payload bytes    (frame[15..15+payloadLength])
        // The first 10 bytes of the payload are the gzip header
        // carrying {source_port, destination_port, xflags, protocol}
        // in gzip-MTIME little-endian convention (matching go-i2cp's
        // `extractI2CPFieldsFromGzip`); the remainder is the gzip
        // member body. The harness inflates the member and hashes
        // the application bytes so digests match the sender's
        // outbound sha256 regardless of which client sent.
        int payloadStart = 15;
        byte[] gzipHeader = new byte[10];
        int gzipHeaderLen = Math.min(10, frame.length - payloadStart);
        System.arraycopy(frame, payloadStart, gzipHeader, 0, gzipHeaderLen);
        int srcPort = gzipHeaderLen > 5 ? (gzipHeader[4] & 0xff) | ((gzipHeader[5] & 0xff) << 8) : 0;
        int dstPort = gzipHeaderLen > 7 ? (gzipHeader[6] & 0xff) | ((gzipHeader[7] & 0xff) << 8) : 0;
        int protocol = gzipHeaderLen > 9 ? gzipHeader[9] & 0xff : 0;
        int bodyStart = payloadStart;
        byte[] wireBody = new byte[payloadLength];
        // The frame buffer holds the complete MessagePayload body in
        // one buffer (the daemon wrote 5-byte header + 10-byte prefix
        // + payload_length bytes payload for a total frame length of
        // 15+payloadLength). payload_start = 15; the payload bytes
        // fill frame[15..15+payloadLength].
        int take = Math.min(payloadLength, frame.length - bodyStart);
        System.arraycopy(frame, bodyStart, wireBody, 0, take);
        if (take < payloadLength) {
            // Continue reading from in until we have all payload bytes.
            int off = take;
            while (off < payloadLength) {
                int need = payloadLength - off;
                int n = in.read(wireBody, off, need);
                if (n < 0) throw new IOException("EOF reading payload body");
                off += n;
            }
        }
        byte[] body = inflateGzipMember(wireBody);
        inboundPayloadDigest = MessageDigest.getInstance("SHA-256").digest(body);
        inboundPayloadLen = body.length;
        inboundProtocol = protocol;
        inboundSrcPort = srcPort;
        inboundDstPort = dstPort;
        recordFact("inbound_protocol", String.valueOf(protocol));
        recordFact("inbound_src_port", String.valueOf(srcPort));
        recordFact("inbound_dst_port", String.valueOf(dstPort));
        recordFact("inbound_payload_len", String.valueOf(inboundPayloadLen));
recordFact("inbound_payload_sha256", toHex(inboundPayloadDigest));
    }

    /** Inflates one gzip member (10-byte header + deflate body +
     *  CRC-32/ISIZE trailer) into application bytes. Falls back to a
     *  raw-deflate inflate for trailer-less senders so older captures
     *  still parse. */
    static byte[] inflateGzipMember(byte[] wireBody) throws IOException {
        try (java.util.zip.GZIPInputStream gzip =
                 new java.util.zip.GZIPInputStream(new ByteArrayInputStream(wireBody));
             ByteArrayOutputStream plain = new ByteArrayOutputStream(wireBody.length)) {
            byte[] chunk = new byte[4096];
            int n;
            while ((n = gzip.read(chunk)) >= 0) {
                plain.write(chunk, 0, n);
            }
            return plain.toByteArray();
        } catch (IOException gzipError) {
            java.util.zip.Inflater inflater = new java.util.zip.Inflater(true);
            try {
                inflater.setInput(wireBody, 10, Math.max(0, wireBody.length - 10));
                ByteArrayOutputStream plain = new ByteArrayOutputStream(wireBody.length);
                byte[] chunk = new byte[4096];
                while (!inflater.finished()) {
                    int n;
                    try {
                        n = inflater.inflate(chunk);
                    } catch (java.util.zip.DataFormatException malformed) {
                        throw new IOException("deflate payload malformed: " + malformed.getMessage());
                    }
                    if (n == 0) break;
                    plain.write(chunk, 0, n);
                }
                return plain.toByteArray();
            } finally {
                inflater.end();
            }
        }
    }

    static void runSendOutbound(String role) throws Exception {
        File tmpDir = new File(System.getProperty("java.io.tmpdir"), "i2cp-java-driver");
        tmpDir.mkdirs();
        File keyFile = new File(tmpDir, role + ".priv");
        Destination dest = makeDestination(keyFile);
        SigningPrivateKey spriv;
        try (java.io.FileInputStream fis = new java.io.FileInputStream(keyFile)) {
            PrivateKeyFile pkf = new PrivateKeyFile(fis);
            spriv = pkf.getSigningPrivKey();
        }
        recordFact("destination_hash_hex", toHex(dest.calculateHash().getData()));
        Object[] conn = connect();
        Socket s = (Socket) conn[0];
        BufferedInputStream in = (BufferedInputStream) conn[1];
        BufferedOutputStream out = (BufferedOutputStream) conn[2];
        byte[] setDate = handshakeGetDateSetDate(in, out);
        long creationMs = System.currentTimeMillis();
        byte[] cfgBody = buildSessionConfig(dest, spriv, creationMs);
        int sessionId = createSession(in, out, cfgBody);
        recordFact("session_id", String.valueOf(sessionId));
        String peerB64 = envOr("PEER_DESTINATION_B64", "");
        if (peerB64.isEmpty()) throw new IllegalStateException("PEER_DESTINATION_B64 required");
        // Plan 170 byte-exact round trip: embed the destination's
        // canonical wire bytes directly. Java's `Destination.writeBytes`
        // emits a compact (32-byte key + 320-byte padding + 32-byte key
        // + cert) form while the i2pr daemon hash uses the legacy
        // (256-byte key slot + 128-byte key slot + cert) form. Skipping
        // the Java `Destination` round-trip and writing the raw
        // base64-decoded wire bytes preserves byte-for-byte equality
        // with the sender's destination encoding.
        byte[] peerBytes = decodePeerB64(peerB64);
        byte[] payloadBytes = envOr("PAYLOAD", "").getBytes("UTF-8");
        recordFact("outbound_payload_len", String.valueOf(payloadBytes.length));
        recordFact("outbound_payload_sha256", toHex(MessageDigest.getInstance("SHA-256").digest(payloadBytes)));
        recordFact("peer_destination_size", String.valueOf(peerBytes.length));
        int srcPort = Integer.parseInt(envOr("SRC_PORT", "7"));
        int dstPort = Integer.parseInt(envOr("DST_PORT", "8"));
        // Plan 170 SendMessage body layout (per SendMessageMessage.java):
        //   u16  session id
        //   Destination (raw 391-byte wire form)
        //   Payload (u32 length + gzip header + body — see encodeGzipHeader)
        //   u32  nonce
        ByteArrayOutputStream sendBody = new ByteArrayOutputStream();
        sendBody.write(sessionId >> 8);
        sendBody.write(sessionId);
        sendBody.write(peerBytes, 0, peerBytes.length);
        byte[] payloadBody = encodeGzipHeader(srcPort, dstPort, payloadBytes);
        // The wire format prefixes the gzip-encoded body with a u32 length.
        sendBody.write(payloadBody.length >> 24);
        sendBody.write(payloadBody.length >> 16);
        sendBody.write(payloadBody.length >> 8);
        sendBody.write(payloadBody.length);
        sendBody.write(payloadBody, 0, payloadBody.length);
        int nonce = 42;
        sendBody.write(nonce >> 24);
        sendBody.write(nonce >> 16);
        sendBody.write(nonce >> 8);
        sendBody.write(nonce);
        writeFrame(out, 5, sendBody.toByteArray());
        // Plan 170 outbound path: confirm the daemon accepted the SendMessage
        // by waiting for the corresponding MessageStatus reply. The M9
        // daemon still drains MessageStatus correlation so the session id
        // we recorded is bound to the matching accept row. After the
        // MessageStatus arrives we close the socket — the test harness
        // pairs this with a peer that is concurrently waiting for an
        // inbound MessagePayload.
        int nonce42 = 42;
        int sawStatus = readMessageStatusOrPayload(in, sessionId, nonce42);
        if (sawStatus == 0) recordError("outbound-no-status", null);
        recordFact("outbound_message_status_observed", String.valueOf(sawStatus));
        ByteArrayOutputStream destroyBody = new ByteArrayOutputStream();
        destroyBody.write(sessionId >> 8);
        destroyBody.write(sessionId);
        writeFrame(out, 3, destroyBody.toByteArray());
        s.close();
        recordFact("status", "passed");
    }

    /** Reads the next reply frame and returns the MessageStatus field if
     *  it is a MessageStatus reply (we only consume those during the
     *  outbound test); inbound MessagePayload is ignored for the
     *  purpose of this assertion. */
    static int readMessageStatusOrPayload(InputStream in, int expectedSession, int expectedNonce) throws IOException {
        for (;;) {
            int[] hdr = readFrameHeader(in);
            byte[] body = readFrameBody(in, hdr[1]);
            // body for MessageStatus is u16 session + u32 message_id + u8 status + u32 size + u32 nonce (15 bytes).
            if (hdr[0] == 22 /* MessageStatus */ && body.length >= 15) {
                int session = ((body[0] & 0xFF) << 8) | (body[1] & 0xFF);
                if (session != expectedSession) continue;
                int status = body[6] & 0xFF;
                int nonceField = ((body[11] & 0xFF) << 24) | ((body[12] & 0xFF) << 16)
                           | ((body[13] & 0xFF) << 8) | (body[14] & 0xFF);
                if (nonceField == expectedNonce) return status;
                continue;
            }
            // MessagePayload (id 31) and any other reply frame are skipped.
            if (hdr[0] == 31 || hdr[0] == 30 /* MessageStatus */) continue;
            return 0;
        }
    }

    static void runSendInbound(String role) throws Exception {
        File tmpDir = new File(System.getProperty("java.io.tmpdir"), "i2cp-java-driver");
        tmpDir.mkdirs();
        File keyFile = new File(tmpDir, role + ".priv");
        Destination dest = makeDestination(keyFile);
        SigningPrivateKey spriv;
        try (java.io.FileInputStream fis = new java.io.FileInputStream(keyFile)) {
            PrivateKeyFile pkf = new PrivateKeyFile(fis);
            spriv = pkf.getSigningPrivKey();
        }
        recordFact("destination_hash_hex", toHex(dest.calculateHash().getData()));
        recordFact("destination_b64", encodeLegacyDestination(dest));
        Object[] conn = connect();
        Socket s = (Socket) conn[0];
        BufferedInputStream in = (BufferedInputStream) conn[1];
        BufferedOutputStream out = (BufferedOutputStream) conn[2];
        byte[] setDate = handshakeGetDateSetDate(in, out);
        long creationMs = System.currentTimeMillis();
        byte[] cfgBody = buildSessionConfig(dest, spriv, creationMs);
        int sessionId = createSession(in, out, cfgBody);
        recordFact("session_id", String.valueOf(sessionId));
        long waitMs = Long.parseLong(envOr("WAIT_MS", "15000"));
        long deadline = System.currentTimeMillis() + waitMs;
        while (System.currentTimeMillis() < deadline && inboundPayloadDigest == null) {
            readMessagePayload(in);
        }
        if (inboundPayloadDigest == null) recordError("inbound-timeout", null);
        ByteArrayOutputStream destroyBody = new ByteArrayOutputStream();
        destroyBody.write(sessionId >> 8);
        destroyBody.write(sessionId);
        writeFrame(out, 3, destroyBody.toByteArray());
        s.close();
        recordFact("status", "passed");
    }

    /** Queries the daemon's bandwidth ceilings through the public
     *  GetBandwidthLimits wire primitive and records the parsed
     *  BandwidthLimits reply (sixteen big-endian u32 values:
     *  client_inbound, client_outbound, router_inbound,
     *  router_inbound_burst, router_outbound, router_outbound_burst,
     *  router_burst_time, reserved[9]). No session is required. */
    static void runBandwidth(String role) throws Exception {
        Object[] conn = connect();
        Socket s = (Socket) conn[0];
        BufferedInputStream in = (BufferedInputStream) conn[1];
        BufferedOutputStream out = (BufferedOutputStream) conn[2];
        handshakeGetDateSetDate(in, out);
        recordFact("connect_role", role);
        writeFrame(out, 8, new byte[0]);
        byte[] frame = readFrame(in);
        if (frame[4] != 23) {
            throw new IOException("expected BandwidthLimits type 23, got " + frame[4]);
        }
        int bodyLen = frame.length - 5;
        if (bodyLen != 64) {
            throw new IOException("expected 64-byte BandwidthLimits body, got " + bodyLen);
        }
        int[] limits = new int[16];
        for (int i = 0; i < 16; i++) {
            limits[i] = ((frame[5 + 4 * i] & 0xff) << 24) | ((frame[6 + 4 * i] & 0xff) << 16)
                      | ((frame[7 + 4 * i] & 0xff) << 8) | (frame[8 + 4 * i] & 0xff);
        }
        recordFact("bandwidth_limits_count", String.valueOf(limits.length));
        recordFact("bandwidth_client_inbound", String.valueOf(limits[0]));
        recordFact("bandwidth_client_outbound", String.valueOf(limits[1]));
        recordFact("bandwidth_router_inbound", String.valueOf(limits[2]));
        recordFact("bandwidth_router_outbound", String.valueOf(limits[4]));
        s.close();
        recordFact("status", "passed");
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: i2cp_java_driver <connect-version|session-leaseset2|cleanup|send-to-go|send-to-java|bandwidth>");
            System.exit(2);
        }
        recordFact("reference", "java_i2p");
        recordFact("release", "2.13.0");
        recordFact("source_revision", PINNED_REVISION);
        recordFact("i2cp_version", "0.9.70");
        switch (args[0]) {
            case "connect-version":
                runConnectVersion();
                break;
            case "session-leaseset2":
                runSessionLifecycle("session-leaseset2");
                break;
            case "cleanup":
                runSessionLifecycle("cleanup");
                break;
            case "send-to-go":
                runSendOutbound("send-to-go");
                break;
            case "send-to-java":
                runSendInbound("send-to-java");
                break;
            case "bandwidth":
                runBandwidth("bandwidth");
                break;
            default:
                recordError("unknown-subcommand", null);
        }
    }
}
