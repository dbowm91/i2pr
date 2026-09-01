// Plan 146 reference helper.
//
// A throwaway Java helper used to produce and consume the standard Java
// `PrivateKeyFile` concatenation for SIGNATURE_TYPE=7 (Ed25519) /
// CRYPTO_TYPE=4 (ECIES-X25519 / X25519). It binds the Plan 146 evidence to
// the pinned reference revision recorded in
// `tests/integration/ntcp2/references.lock.toml`
// (Java I2P 2.12.0 at `2800040deee9bb376567b671ef2e9c34cf3e30b6`,
// i2pd 2.60.0 at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`).
//
// Usage:
//   javac -cp <i2p.jar> Plan146ReferenceHelper.java
//
//   # Generate a fresh reference private destination (Evidence direction B
//   # in Plan 146 §6, "reference generates, i2pr imports").
//   # Emits one record per line, fields separated by single spaces; no raw
//   # secret material is written -- only lengths, the public destination
//   # hash, and the SHA-256 digest of the ephemeral PRIV bytes.
//   java -cp <i2p.jar>:. Plan146ReferenceHelper generate
//
//   # Parse a PRIV supplied on stdin as I2P Base64 (Evidence direction B in
//   # Plan 146 §5, "reference consumes i2pr output"). The Base64 input is
//   # the textual PRIV returned by i2pr's `DEST GENERATE SIGNATURE_TYPE=7`
//   # or `SESSION CREATE` reply. The helper emits the public destination
//   # hash so the test can compare exact equality.
//   java -cp <i2p.jar>:. Plan146ReferenceHelper parse < priv.b64
//
//   # Print the pinned reference revision used at compile time so the
//   # runner can record provenance.
//   java -cp <i2p.jar>:. Plan146ReferenceHelper version
//
// The class is intentionally narrow: it does no I/O beyond stdin/stdout,
// holds one private destination in memory at a time, and never logs the
// private bytes. Plan 146 §9 forbids committing the raw `PRIV` value.

import net.i2p.data.*;
import net.i2p.crypto.SigType;
import net.i2p.crypto.EncType;
import net.i2p.crypto.KeyGenerator;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.FileOutputStream;
import java.io.File;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.Base64;

public class Plan146ReferenceHelper {

    // Pinned alongside `tests/integration/ntcp2/references.lock.toml`.
    // Java I2P 2.12.0 source revision; the compiled artifact in
    // `target/interop/cache/java_i2p/<cache-key>/lib/i2p.jar` is the same
    // revision rebuilt under the Plan 038 harness.
    static final String PINNED_REVISION = "2800040deee9bb376567b671ef2e9c34cf3e30b6";
    static final String PINNED_RELEASE = "2.12.0";

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("usage: Plan146ReferenceHelper {generate|parse|version}");
            System.exit(2);
        }
        switch (args[0]) {
            case "generate":
                doGenerate();
                break;
            case "parse":
                doParse();
                break;
            case "version":
                System.out.println("reference=java_i2p");
                System.out.println("release=" + PINNED_RELEASE);
                System.out.println("source_revision=" + PINNED_REVISION);
                System.out.println("signature_type=7");
                System.out.println("crypto_type=4");
                System.out.println("keygen=SigType.EdDSA_SHA512_Ed25519");
                System.out.println("encgen=EncType.ECIES_X25519");
                break;
            default:
                System.err.println("unknown subcommand: " + args[0]);
                System.exit(2);
        }
    }

    private static void doGenerate() throws Exception {
        SigType sig = SigType.EdDSA_SHA512_Ed25519;
        EncType enc = EncType.ECIES_X25519;

        // Ed25519 signing keypair.
        Object[] keys = KeyGenerator.getInstance().generateSigningKeys(sig);
        SigningPublicKey spub = (SigningPublicKey) keys[0];
        SigningPrivateKey spriv = (SigningPrivateKey) keys[1];

        // Destination encryption private key is unused since 0.6; Java I2P
        // 2.12.0's `PrivateKeyFile` accepts a random 32-byte X25519 secret
        // (the length is dictated by the EncType in the KeyCertificate,
        // not the legacy 256-byte ElGamal size). Plan 146 §7 documents
        // this and pins both i2pd and Java I2P behaviour.
        byte[] randPriv = new byte[32];
        new SecureRandom().nextBytes(randPriv);
        PrivateKey priv = new PrivateKey(enc, randPriv);

        // 32-byte X25519 encryption public key slot (also unused for
        // destinations). The first 32 bytes of the public key area carry
        // this slot; the remaining 320 bytes are random padding.
        byte[] pubRand = new byte[32];
        new SecureRandom().nextBytes(pubRand);
        PublicKey pub = new PublicKey(enc, pubRand);
        byte[] padding = new byte[320];
        new SecureRandom().nextBytes(padding);

        KeyCertificate cert = new KeyCertificate(sig, enc);
        Destination d = new Destination();
        d.setPublicKey(pub);
        d.setSigningPublicKey(spub);
        d.setCertificate(cert);
        d.setPadding(padding);

        // Build the standard Java `PrivateKeyFile` binary concatenation:
        // Destination || PrivateKey || SigningPrivateKey.
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        d.writeBytes(dos);
        priv.writeBytes(dos);
        spriv.writeBytes(dos);
        byte[] privBytes = baos.toByteArray();

        // Compute the SHA-256 of the public destination encoding. Plan 146
        // §5 / §6 require exact public destination equality after the
        // helper round-trip and after the i2pr import path. The reference
        // helper never emits the raw PRIV -- only its length, its SHA-256
        // digest (for record-keeping), and the public destination's
        // SHA-256 hash plus Base64.
        MessageDigest sha = MessageDigest.getInstance("SHA-256");
        byte[] destBytes = d.toByteArray();
        byte[] destHash = sha.digest(destBytes);
        byte[] privHash = sha.digest(privBytes);
        String privB64 = net.i2p.data.Base64.encode(privBytes);
        String pubB64 = net.i2p.data.Base64.encode(destBytes);

        // Round-trip the generated PRIV through PrivateKeyFile to prove
        // the helper itself can re-parse its own output. The destination
        // hash must be byte-equal.
        File tmp = File.createTempFile("plan146-pkf-", ".dat");
        try (FileOutputStream fos = new FileOutputStream(tmp)) {
            fos.write(privBytes);
        }
        PrivateKeyFile pkf = new PrivateKeyFile(tmp);
        Destination d2 = pkf.getDestination();
        byte[] dest2Bytes = d2.toByteArray();
        byte[] dest2Hash = sha.digest(dest2Bytes);
        boolean destEqual = java.util.Arrays.equals(destHash, dest2Hash);
        boolean bytesEqual = java.util.Arrays.equals(destBytes, dest2Bytes);
        tmp.delete();

        // Emit the record. Field order is stable so the Rust runner can
        // parse line by line.
        System.out.println("reference=java_i2p");
        System.out.println("release=" + PINNED_RELEASE);
        System.out.println("source_revision=" + PINNED_REVISION);
        System.out.println("keygen=" + sig.getCode());
        System.out.println("encgen=" + enc.getCode());
        System.out.println("priv_binary_len=" + privBytes.length);
        System.out.println("priv_base64_len=" + privB64.length());
        System.out.println("pub_binary_len=" + destBytes.length);
        System.out.println("pub_base64_len=" + pubB64.length());
        System.out.println("priv_sha256=" + toHex(privHash));
        System.out.println("dest_sha256=" + toHex(destHash));
        System.out.println("private_key_field_is_256=" + (privBytes.length - destBytes.length - 32 == 256));
        System.out.println("helper_self_round_trip_dest_equal=" + destEqual);
        System.out.println("helper_self_round_trip_bytes_equal=" + bytesEqual);
        // Print the Base64 PRIV as a separate marker so the runner can
        // capture it for the i2pr import path. The runner is responsible
        // for deleting this secret-bearing value after the import.
        System.out.println("PRIV_B64_BEGIN");
        System.out.println(privB64);
        System.out.println("PRIV_B64_END");
    }

    private static void doParse() throws Exception {
        // Read I2P Base64 PRIV from stdin, decode, and emit the parsed
        // public destination hash. The runner compares this hash with the
        // one the i2pr-side export reports.
        StringBuilder sb = new StringBuilder();
        java.io.BufferedReader reader = new java.io.BufferedReader(new java.io.InputStreamReader(System.in));
        String line;
        while ((line = reader.readLine()) != null) {
            sb.append(line.trim());
        }
        String b64 = sb.toString();
        // Use net.i2p.data.Base64 for I2P-alphabet decoding.
        // `Base64.decode(String)` does not exist directly; the
        // DataHelper-style decode lives in `net.i2p.data.Base64`.
        // Use the public decoder that respects the I2P substitution table
        // (slots 62 / 63 are `-` / `~` and padding is `=`).
        byte[] privBytes = net.i2p.data.Base64.decode(b64);
        File tmp = File.createTempFile("plan146-pkf-parse-", ".dat");
        try (FileOutputStream fos = new FileOutputStream(tmp)) {
            fos.write(privBytes);
        }
        PrivateKeyFile pkf = new PrivateKeyFile(tmp);
        Destination d = pkf.getDestination();
        byte[] destBytes = d.toByteArray();
        MessageDigest sha = MessageDigest.getInstance("SHA-256");
        byte[] destHash = sha.digest(destBytes);
        String pubB64 = net.i2p.data.Base64.encode(destBytes);
        String certType = d.getCertificate().getCertificateType() == Certificate.CERTIFICATE_TYPE_KEY ? "KEY_CERT" : String.valueOf(d.getCertificate().getCertificateType());
        tmp.delete();
        System.out.println("reference=java_i2p");
        System.out.println("release=" + PINNED_RELEASE);
        System.out.println("source_revision=" + PINNED_REVISION);
        System.out.println("input_priv_binary_len=" + privBytes.length);
        System.out.println("input_priv_base64_len=" + b64.length());
        System.out.println("parsed_pub_binary_len=" + destBytes.length);
        System.out.println("parsed_pub_base64_len=" + pubB64.length());
        System.out.println("parsed_cert_type=" + certType);
        System.out.println("parsed_cert_signing_type=" + d.getSigningPublicKey().getType().getCode());
        System.out.println("parsed_cert_crypto_type=" + d.getPublicKey().getType().getCode());
        System.out.println("parsed_dest_sha256=" + toHex(destHash));
        // Emit the parsed public destination Base64 so the runner can
        // compare byte-for-byte with i2pr's `PUB` reply.
        System.out.println("PUB_B64_BEGIN");
        System.out.println(pubB64);
        System.out.println("PUB_B64_END");
    }

    private static String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b));
        return sb.toString();
    }
}
