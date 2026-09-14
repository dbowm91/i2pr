// Plan 198 counted reference service.  This helper uses only the public
// Java I2P client API; the localhost control socket carries coordination
// commands and never carries I2P protocol framing.

import net.i2p.client.I2PClient;
import net.i2p.client.I2PClientFactory;
import net.i2p.client.I2PSession;
import net.i2p.client.I2PSessionException;
import net.i2p.client.I2PSessionListener;
import net.i2p.crypto.SigType;
import net.i2p.data.Destination;

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
import java.util.Properties;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.TimeUnit;

public final class ReferenceRawDestination {
    private static final String PIN = "9134f808337b401e8e53c73734c81fab04280c9d";

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
            System.out.println("READY " + destination.toBase64() + " " + PIN);
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
