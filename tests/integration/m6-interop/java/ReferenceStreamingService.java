// Plan 198 counted reference service.  The I2P side uses only the public
// Streaming API.  The localhost control socket is test coordination only.

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

public final class ReferenceStreamingService {
    private static final String PIN = "9134f808337b401e8e53c73734c81fab04280c9d";
    private static final Map<Integer, I2PSocket> SOCKETS = new ConcurrentHashMap<>();
    private static final List<Integer> ACCEPTED = Collections.synchronizedList(new ArrayList<>());
    private static final List<Integer> CONNECTED = Collections.synchronizedList(new ArrayList<>());
    private static final AtomicInteger NEXT_ID = new AtomicInteger(1);
    private static volatile boolean accepting;

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
        try {
            I2PSocket socket = server.accept();
            int id = NEXT_ID.getAndIncrement();
            SOCKETS.put(id, socket);
            ACCEPTED.add(id);
            return id;
        } catch (Throwable error) {
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
            System.out.println("READY " + destination.toBase64() + " " + PIN);
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
                        case "START_ACCEPT":
                            if (!accepting) {
                                accepting = true;
                                Thread thread = new Thread(() -> { acceptOne(server); accepting = false; }, "plan198-accept");
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
