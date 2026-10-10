import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;

/**
 * A small Java SAM wire peer for exercising child styles omitted by the
 * pinned SAMStreamSink utility.  The normative command spellings and child
 * matrix are source-locked to Java I2P 2.13.0 by run-java-368.sh.
 */
public final class Sam368PrimaryProbe {
    private Sam368PrimaryProbe() {}

    private static void expect(BufferedReader input, String prefix) throws Exception {
        String line = input.readLine();
        boolean reorderedPrimaryStatus = line != null
            && prefix.startsWith("SESSION STATUS RESULT=OK ID=")
            && line.startsWith("SESSION STATUS RESULT=OK ")
            && line.contains(prefix.substring("SESSION STATUS RESULT=OK ".length()));
        if (line == null || (!line.startsWith(prefix) && !reorderedPrimaryStatus)) {
            String safe = line == null ? "<eof>" : line;
            StringBuilder redacted = new StringBuilder();
            for (String token : safe.split(" ")) {
                if (redacted.length() > 0) redacted.append(' ');
                if (token.startsWith("DESTINATION=") || token.startsWith("VALUE=")) {
                    redacted.append(token.substring(0, token.indexOf('=') + 1)).append("<redacted>");
                } else {
                    redacted.append(token);
                }
            }
            safe = redacted.toString();
            throw new IllegalStateException("unexpected SAM reply " + safe);
        }
    }

    private static void send(OutputStream output, String command) throws Exception {
        output.write((command + "\n").getBytes(StandardCharsets.UTF_8));
        output.flush();
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("host and port required");
        String phase = "connect";
        try (DatagramSocket datagram1 = new DatagramSocket(
                 new InetSocketAddress(InetAddress.getByName("127.0.0.1"), 0));
             DatagramSocket datagram2 = new DatagramSocket(
                 new InetSocketAddress(InetAddress.getByName("127.0.0.1"), 0));
             DatagramSocket datagram3 = new DatagramSocket(
                 new InetSocketAddress(InetAddress.getByName("127.0.0.1"), 0));
             Socket socket = new Socket(args[0], Integer.parseInt(args[1]));
             BufferedReader input = new BufferedReader(new InputStreamReader(
                 socket.getInputStream(), StandardCharsets.UTF_8))) {
            OutputStream output = socket.getOutputStream();
            socket.setSoTimeout(10000);

            phase = "HELLO";
            send(output, "HELLO VERSION MIN=1.0 MAX=3.3");
            expect(input, "HELLO REPLY RESULT=OK VERSION=3.3");
            phase = "PRIMARY";
            send(output, "SESSION CREATE DESTINATION=TRANSIENT STYLE=PRIMARY ID=javaProbe");
            expect(input, "SESSION STATUS RESULT=OK ");

            String[] commands = {
                "SESSION ADD STYLE=STREAM ID=stream FROM_PORT=18110 TO_PORT=18110",
                "SESSION ADD STYLE=DATAGRAM ID=datagram PORT=" + datagram1.getLocalPort() + " LISTEN_PORT=18117",
                "SESSION ADD STYLE=DATAGRAM2 ID=datagram2 PORT=" + datagram2.getLocalPort() + " LISTEN_PORT=18119",
                "SESSION ADD STYLE=DATAGRAM3 ID=datagram3 PORT=" + datagram3.getLocalPort() + " LISTEN_PORT=18120",
                "SESSION ADD STYLE=RAW ID=raw PORT=18118 LISTEN_PORT=18118 PROTOCOL=18 LISTEN_PROTOCOL=18"
            };
            String[] ids = {"stream", "datagram", "datagram2", "datagram3", "raw"};
            for (int i = 0; i < commands.length; i++) {
                phase = ids[i] + " child";
                send(output, commands[i]);
                expect(input, "SESSION STATUS RESULT=OK ID=\"" + ids[i] + "\"");
            }

            phase = "NAME=ME";
            send(output, "NAMING LOOKUP NAME=ME");
            expect(input, "NAMING REPLY RESULT=OK NAME=ME VALUE=");
            phase = "RAW child removal";
            send(output, "SESSION REMOVE ID=raw");
            expect(input, "SESSION STATUS RESULT=OK ID=\"raw\"");
            System.out.println("java_sam33_primary_child_matrix=passed");
            System.out.flush();

            phase = "DATAGRAM1/2/3 receive";
            receivePayload(datagram1, "java-protocol-17");
            receivePayload(datagram2, "java-protocol-19");
            receivePayload(datagram3, "java-protocol-20");
            System.out.println("java_sam33_datagram_17_19_20_receive=passed");
            System.out.flush();

            BufferedReader control = new BufferedReader(new InputStreamReader(
                System.in, StandardCharsets.UTF_8));
            if (!"close".equals(control.readLine())) {
                throw new IllegalStateException("expected close control");
            }
        } catch (Exception failure) {
            System.err.println("Java SAM probe failed during " + phase + ": "
                + failure.getClass().getSimpleName() + ": " + failure.getMessage());
            System.exit(2);
        }
    }

    private static void receivePayload(DatagramSocket socket, String expected) throws Exception {
        socket.setSoTimeout(10000);
        byte[] bytes = new byte[65507];
        DatagramPacket packet = new DatagramPacket(bytes, bytes.length);
        socket.receive(packet);
        String received = new String(packet.getData(), packet.getOffset(), packet.getLength(),
            StandardCharsets.UTF_8);
        if (!received.endsWith(expected)) {
            throw new IllegalStateException("unexpected protocol datagram payload");
        }
    }
}
