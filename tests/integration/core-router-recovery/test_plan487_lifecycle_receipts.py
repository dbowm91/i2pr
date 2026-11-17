#!/usr/bin/env python3
"""Offline controls for the Plan 487 §6 lifecycle lane's receipt readers.

The daemon emits terminal colour escapes even when its stdout is a
pipe, and the escapes land *inside* the tracing field pairs. A raw
substring search for `transition="…"` therefore never matches a real
daemon log line, and every receipt count would silently read zero —
which is a false "no publication happened" claim, not a cosmetic bug.

These controls pin the readers against faithfully reproduced daemon
output, so the lane's receipt evidence cannot silently decay to zeros
again.
"""

import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_plan487_lifecycle_recovery as lane  # noqa: E402


def daemon_line(message: str, fields: list[tuple[str, str]]) -> str:
    """One daemon log line with the daemon's real ANSI field layout.

    The tracing layer colours the field name, the `=` separator, and
    the value independently, so the coloured text never contains the
    contiguous substring `transition="…"`.
    """
    parts = [
        "2026-10-19T20:57:21.695961Z\x1b[0m",
        " \x1b[32m INFO\x1b[0m",
        " \x1b[2mi2pr_daemon::service_product\x1b[0m\x1b[2m:\x1b[0m",
        f" {message}",
    ]
    for name, value in fields:
        parts.append(f" \x1b[3m{name}\x1b[0m\x1b[2m=\x1b[0m{value}")
    return "".join(parts) + "\n"


PUBLICATION_EMITTED = daemon_line(
    "LS2 publication emitted; awaiting the correlated DeliveryStatus",
    [
        ("publication_reply_token", "2141790866"),
        ("published_seconds", "1792443440"),
        ("oldest_lease_expiry_ms", "1792443980000"),
        ("transition", '"ls2_publication_awaiting_acknowledgement"'),
    ],
)

PUBLICATION_ACKNOWLEDGED = daemon_line(
    "LS2 publication acknowledged by a correlated DeliveryStatus on an inbound tunnel (counts only)",
    [
        ("publication_reply_token", "2141790866"),
        ("inbound_tunnel_id", "928743703"),
        ("published_seconds", "1792443440"),
        ("elapsed_ms", "0"),
        ("transition", '"ls2_publication_acknowledged"'),
    ],
)

PUBLICATION_ACKNOWLEDGED_NO_TUNNEL = daemon_line(
    "LS2 publication acknowledged by a correlated DeliveryStatus on an inbound tunnel (counts only)",
    [
        ("publication_reply_token", "2141790866"),
        ("inbound_tunnel_id", "0"),
        ("published_seconds", "1792443440"),
        ("elapsed_ms", "0"),
        ("transition", '"ls2_publication_acknowledged"'),
    ],
)

BOOTSTRAP_RETRY = daemon_line(
    "destination outbound bootstrap attempt unanswered after emission",
    [
        ("build_attempt_id", "4"),
        ("originator_write_disposition", "AllOriginatorDeliveriesEmitted"),
        ("retry_attempt", "2"),
        ("transition", '"bootstrap_retry"'),
    ],
)

INBOUND_RETRY = daemon_line(
    "destination inbound build attempt unanswered after emission",
    [
        ("build_attempt_id", "5"),
        ("originator_write_disposition", "PartialEmission"),
        ("retry_attempt", "2"),
        ("transition", '"inbound_retry"'),
    ],
)

SERVICE_FAILED = daemon_line(
    "service failed",
    [
        ("event", '"runtime.service.failed"'),
        ("service", "ssu2-router"),
        ("classification", "Optional"),
        ("lifecycle", "Failed"),
        ("restart_attempts", "0"),
        ("failure_category", "Some(ServiceFailure)"),
    ],
)

SERVICE_FAILED_OTHER = daemon_line(
    "service failed",
    [
        ("event", '"runtime.service.failed"'),
        ("service", "netdb-bootstrap"),
        ("classification", "Essential"),
        ("lifecycle", "Failed"),
    ],
)


class ReceiptReaderTests(unittest.TestCase):
    def _write(self, text: str) -> Path:
        handle = tempfile.NamedTemporaryFile(
            "w", suffix=".log", delete=False, encoding="utf-8"
        )
        handle.write(text)
        handle.close()
        self.addCleanup(lambda: Path(handle.name).unlink(missing_ok=True))
        return Path(handle.name)

    def test_publication_receipts_are_read_from_coloured_output(self) -> None:
        path = self._write(
            PUBLICATION_EMITTED + PUBLICATION_ACKNOWLEDGED + PUBLICATION_EMITTED
        )
        receipt = lane.read_publication_receipts(path)
        self.assertEqual(receipt["publications_awaiting_acknowledgement"], 2)
        self.assertEqual(receipt["publications_acknowledged"], 1)
        self.assertEqual(receipt["acknowledged_on_inbound_tunnel"], 1)
        self.assertEqual(receipt["published_deltas_seconds"], [])
        self.assertEqual(receipt["oldest_lease_expiry_deltas_ms"], [])

    def test_publication_deltas_follow_the_published_order(self) -> None:
        second = daemon_line(
            "LS2 publication emitted; awaiting the correlated DeliveryStatus",
            [
                ("published_seconds", "1792443464"),
                ("oldest_lease_expiry_ms", "1792444004000"),
                ("transition", '"ls2_publication_awaiting_acknowledgement"'),
            ],
        )
        path = self._write(PUBLICATION_EMITTED + second)
        receipt = lane.read_publication_receipts(path)
        self.assertEqual(receipt["published_deltas_seconds"], [24])
        self.assertEqual(receipt["oldest_lease_expiry_deltas_ms"], [24000])

    def test_an_acknowledgement_without_an_inbound_tunnel_is_not_counted(self) -> None:
        path = self._write(PUBLICATION_ACKNOWLEDGED_NO_TUNNEL)
        receipt = lane.read_publication_receipts(path)
        self.assertEqual(receipt["publications_acknowledged"], 1)
        self.assertEqual(receipt["acknowledged_on_inbound_tunnel"], 0)

    def test_build_retry_receipts_are_read_from_coloured_output(self) -> None:
        path = self._write(BOOTSTRAP_RETRY + INBOUND_RETRY)
        self.assertEqual(lane.read_build_retry_receipts(path), 2)

    def test_service_failures_are_read_from_coloured_output(self) -> None:
        # Plan 487 WP4: the lease-rotation path used to fail the whole
        # `ssu2-router` service. The lane asserts the service never
        # failed, so the reader must see the failure receipt at all.
        path = self._write(SERVICE_FAILED)
        failures = lane.read_service_failures(path)
        self.assertEqual(failures, {"ssu2-router": 1})

    def test_service_failures_are_counted_per_service(self) -> None:
        path = self._write(SERVICE_FAILED + SERVICE_FAILED_OTHER + SERVICE_FAILED)
        failures = lane.read_service_failures(path)
        self.assertEqual(failures, {"ssu2-router": 2, "netdb-bootstrap": 1})

    def test_an_unrelated_receipt_is_not_a_service_failure(self) -> None:
        path = self._write(PUBLICATION_EMITTED + BOOTSTRAP_RETRY)
        self.assertEqual(lane.read_service_failures(path), {})

    def test_an_unrelated_receipt_is_not_a_build_retry(self) -> None:
        path = self._write(PUBLICATION_EMITTED)
        self.assertEqual(lane.read_build_retry_receipts(path), 0)

    def test_acknowledgement_count_is_read_from_coloured_output(self) -> None:
        path = self._write(PUBLICATION_ACKNOWLEDGED + PUBLICATION_ACKNOWLEDGED)
        self.assertEqual(lane.read_publication_acknowledgement_count(path), 2)

    def test_the_offset_separates_the_pre_and_post_restart_receipts(self) -> None:
        before = PUBLICATION_ACKNOWLEDGED + BOOTSTRAP_RETRY
        path = self._write(before)
        offset = path.stat().st_size
        with path.open("a", encoding="utf-8") as handle:
            handle.write(PUBLICATION_ACKNOWLEDGED + INBOUND_RETRY)
        self.assertEqual(lane.read_build_retry_receipts(path), 2)
        self.assertEqual(lane.read_build_retry_receipts(path, offset), 1)
        self.assertEqual(lane.read_publication_acknowledgement_count(path), 2)
        self.assertEqual(lane.read_publication_acknowledgement_count(path, offset), 1)

    def test_a_missing_log_reads_as_empty_rather_than_raising(self) -> None:
        missing = Path("/nonexistent/plan487-i2pr.stdout")
        self.assertEqual(lane.read_build_retry_receipts(missing), 0)
        self.assertEqual(lane.read_publication_acknowledgement_count(missing), 0)
        self.assertEqual(
            lane.read_publication_receipts(missing)["publications_acknowledged"], 0
        )


class FakeSocket:
    def __init__(self, payload: bytes = b""):
        self.payload = payload
        self.sent = b""

    def sendall(self, data: bytes) -> None:
        self.sent += data


class FakeSamSocket:
    def __init__(self, inbound: bytes):
        self.sock = FakeSocket()
        self.inbound = inbound

    def exact(self, count: int) -> bytes:
        if not self.inbound:
            raise ConnectionError(f"short stream read (0/{count})")
        chunk, self.inbound = self.inbound[:count], self.inbound[count:]
        return chunk


class ExchangeAttributionTests(unittest.TestCase):
    def test_the_accept_side_loss_names_the_accept_side(self) -> None:
        connect = FakeSamSocket(b"")
        accept = FakeSamSocket(b"")
        with self.assertRaises(lane.StreamExchangeError) as caught:
            lane.exchange(connect, accept, b"payload")
        self.assertIn("accept side", str(caught.exception))

    def test_the_connect_side_loss_names_the_connect_side(self) -> None:
        connect = FakeSamSocket(b"")
        accept = FakeSamSocket(b"payload")
        with self.assertRaises(lane.StreamExchangeError) as caught:
            lane.exchange(connect, accept, b"payload")
        self.assertIn("connect side", str(caught.exception))

    def test_a_matching_round_trip_returns_the_payload_digest(self) -> None:
        payload = b"payload"
        connect = FakeSamSocket(payload)
        accept = FakeSamSocket(payload)
        digest = lane.exchange(connect, accept, payload)
        self.assertEqual(digest, lane._digest(payload))

    def test_a_differing_payload_is_not_a_silent_pass(self) -> None:
        connect = FakeSamSocket(b"other")
        accept = FakeSamSocket(b"payload")
        with self.assertRaises(lane.StreamExchangeError):
            lane.exchange(connect, accept, b"payload")


STALL_RECEIPT = daemon_line(
    "STREAM ACCEPT data phase stalled with no DATA at the owning manager; live four-stage attribution (counts only)",
    [
        ("app_bytes_drained", "0"),
        ("mirror_observed", "0"),
        ("mirror_accepted", "0"),
        ("mirror_rejected", "0"),
        ("mirror_payload_bytes", "0"),
        ("mirror_arrived", "0"),
        ("canonical_observed", "4"),
        ("canonical_accepted", "4"),
        ("canonical_rejected", "0"),
        ("canonical_payload_bytes", "2328"),
        ("canonical_arrived", "4"),
        ("mirror_ack_generated", "0"),
        ("mirror_ack_transmitted", "0"),
        ("mirror_ack_no_ack_emitted", "0"),
        ("canonical_ack_generated", "2"),
        ("canonical_ack_transmitted", "2"),
        ("ack_egress_sent", "0"),
        ("ack_egress_dropped", "0"),
        ("transition", '"stream_accept_data_phase_stalled"'),
    ],
)


class DataPhaseStallAttributionTests(unittest.TestCase):
    def _write(self, text: str) -> Path:
        handle = tempfile.NamedTemporaryFile(
            "w", suffix=".log", delete=False, encoding="utf-8"
        )
        handle.write(text)
        handle.close()
        self.addCleanup(lambda: Path(handle.name).unlink(missing_ok=True))
        return Path(handle.name)

    def test_the_stall_receipt_is_read_with_its_stage_counts(self) -> None:
        path = self._write(STALL_RECEIPT)
        receipts = lane.read_data_phase_stall_receipts(path)
        self.assertEqual(len(receipts), 1)
        receipt = receipts[0]
        self.assertEqual(receipt["app_bytes_drained"], 0)
        self.assertEqual(receipt["mirror_arrived"], 0)
        self.assertEqual(receipt["canonical_arrived"], 4)
        self.assertEqual(receipt["canonical_payload_bytes"], 2328)
        self.assertEqual(receipt["ack_egress_sent"], 0)

    def test_data_in_the_wrong_manager_is_named_as_such(self) -> None:
        path = self._write(STALL_RECEIPT)
        receipts = lane.read_data_phase_stall_receipts(path)
        self.assertEqual(
            lane.classify_data_phase_stall(receipts),
            "data-arrived-in-the-wrong-manager",
        )

    def test_data_that_never_arrived_is_named_as_such(self) -> None:
        line = STALL_RECEIPT.replace(
            'canonical_arrived\x1b[0m\x1b[2m=\x1b[0m4',
            'canonical_arrived\x1b[0m\x1b[2m=\x1b[0m0',
        )
        path = self._write(line)
        receipts = lane.read_data_phase_stall_receipts(path)
        self.assertEqual(
            lane.classify_data_phase_stall(receipts), "data-never-arrived"
        )

    def test_no_receipt_is_reported_as_no_receipt(self) -> None:
        path = self._write(PUBLICATION_EMITTED)
        self.assertEqual(
            lane.classify_data_phase_stall(lane.read_data_phase_stall_receipts(path)),
            "no-four-stage-receipt",
        )

    def test_a_data_phase_failure_without_a_receipt_says_so(self) -> None:
        path = self._write(PUBLICATION_EMITTED)
        connect = FakeSamSocket(b"")
        accept = FakeSamSocket(b"")
        with self.assertRaises(lane.StreamExchangeError) as caught:
            lane.exchange(connect, accept, b"payload", stdout_path=path)
        self.assertIn("no four-stage DATA-phase attribution", str(caught.exception))

    def test_a_data_phase_failure_carries_the_routers_own_classification(self) -> None:
        path = self._write(STALL_RECEIPT)
        connect = FakeSamSocket(b"")
        accept = FakeSamSocket(b"")
        with self.assertRaises(lane.StreamExchangeError) as caught:
            lane.exchange(connect, accept, b"payload", stdout_path=path)
        self.assertIn(
            "data-arrived-in-the-wrong-manager", str(caught.exception)
        )


class RotatedPublicationAcknowledgementTests(unittest.TestCase):
    """The rotation's own publication must be the acknowledged one.

    A count of acknowledgements cannot tell the caller's publication
    from an unrelated one, and the caller's publication is often
    already counted when the caller starts waiting. The gate names the
    publication instead.
    """

    def _write(self, text: str) -> Path:
        handle = tempfile.NamedTemporaryFile(
            "w", suffix=".log", delete=False, encoding="utf-8"
        )
        handle.write(text)
        handle.close()
        self.addCleanup(lambda: Path(handle.name).unlink(missing_ok=True))
        return Path(handle.name)

    def _acknowledged(self, published_seconds: int) -> str:
        return daemon_line(
            "LS2 publication acknowledged by a correlated DeliveryStatus on an "
            "inbound tunnel (counts only)",
            [
                ("publication_reply_token", "2141790866"),
                ("inbound_tunnel_id", "928743703"),
                ("published_seconds", str(published_seconds)),
                ("elapsed_ms", "0"),
                ("transition", '"ls2_publication_acknowledged"'),
            ],
        )

    def test_the_acknowledged_publication_is_read_from_the_receipt(self) -> None:
        path = self._write(PUBLICATION_ACKNOWLEDGED)
        self.assertEqual(
            lane.read_acknowledged_published_seconds(path), [1792443440]
        )

    def test_an_earlier_publication_does_not_satisfy_the_gate(self) -> None:
        path = self._write(self._acknowledged(1792443440))
        count = lane.wait_for_publication_acknowledgement(
            path,
            baseline=1,
            published_seconds=1792443999,
            budget_s=0.0,
        )
        self.assertEqual(count, 1)

    def test_the_named_publication_satisfies_the_gate(self) -> None:
        path = self._write(self._acknowledged(1792443999))
        count = lane.wait_for_publication_acknowledgement(
            path,
            baseline=0,
            published_seconds=1792443999,
            budget_s=0.0,
        )
        self.assertEqual(count, 1)

    def test_a_count_alone_still_gates_on_a_new_acknowledgement(self) -> None:
        path = self._write(self._acknowledged(1792443440))
        count = lane.wait_for_publication_acknowledgement(
            path, baseline=1, budget_s=0.0
        )
        self.assertEqual(count, 1)


class ArmedAcceptReadTests(unittest.TestCase):
    """The accept-side read must be armed before the peer can deliver.

    The accept socket is one bidirectional channel that the router's
    raw driver also reads. An application that posts its read late can
    find the channel already drained by the driver, and the payload
    never reaches the application.
    """

    def test_the_accept_read_is_armed_before_the_connect_send(self) -> None:
        order = []

        class RecordingSocket:
            def __init__(self, name, inbound=b""):
                self.name = name
                self.inbound = inbound

            def sendall(self, data):
                order.append(f"send:{self.name}")

            def settimeout(self, _value):
                return None

        class RecordingSam:
            def __init__(self, name, inbound=b""):
                self.name = name
                self.sock = RecordingSocket(name)
                self.inbound = inbound

            def exact(self, count):
                order.append(f"read:{self.name}")
                if not self.inbound:
                    raise ConnectionError(f"short stream read (0/{count})")
                chunk, self.inbound = self.inbound[:count], self.inbound[count:]
                return chunk

        connect = RecordingSam("connect", b"payload")
        accept = RecordingSam("accept", b"payload")
        lane.exchange(connect, accept, b"payload")
        self.assertLess(order.index("read:accept"), order.index("send:connect"))

    def test_a_late_read_is_not_what_the_lane_does(self) -> None:
        # The armed read is a separate thread, so the read is posted
        # before the connect side sends even though the send is what
        # makes the payload arrive.
        armed = lane.AcceptSideRead(FakeSamSocket(b"payload"), 7)
        received, error = armed.wait(5.0)
        self.assertIsNone(error)
        self.assertEqual(received, b"payload")

    def test_a_failed_accept_read_is_reported_not_swallowed(self) -> None:
        armed = lane.AcceptSideRead(FakeSamSocket(b""), 7)
        received, error = armed.wait(5.0)
        self.assertIsNone(received)
        self.assertIsInstance(error, ConnectionError)


class LeaseRotationTests(unittest.TestCase):
    def _write(self, text: str) -> Path:
        handle = tempfile.NamedTemporaryFile(
            "w", suffix=".log", delete=False, encoding="utf-8"
        )
        handle.write(text)
        handle.close()
        self.addCleanup(lambda: Path(handle.name).unlink(missing_ok=True))
        return Path(handle.name)

    @staticmethod
    def _published(published_seconds: int, expiry_ms: int) -> str:
        return daemon_line(
            "LS2 publication emitted; awaiting the correlated DeliveryStatus",
            [
                ("published_seconds", str(published_seconds)),
                ("oldest_lease_expiry_ms", str(expiry_ms)),
                ("transition", '"ls2_publication_awaiting_acknowledgement"'),
            ],
        )

    def test_a_second_publication_of_the_same_destination_is_a_rotation(self) -> None:
        path = self._write(
            self._published(1792448840, 1792449380000)
            + self._published(1792449160, 1792449700000)
        )
        receipt = lane.wait_for_lease_rotation(path, baseline_count=0, budget_s=0.0)
        self.assertTrue(receipt["rotated"])
        self.assertEqual(receipt["published_delta_seconds"], 320)
        self.assertEqual(receipt["oldest_lease_expiry_delta_ms"], 320000)
        self.assertEqual(receipt["publications_for_the_destination"], 2)

    def test_two_destinations_are_not_a_rotation(self) -> None:
        # The baseline scopes the comparison to the destination under
        # test: a publication for a *different* destination must not be
        # read as this destination's rotation.
        path = self._write(
            self._published(1792448840, 1792449380000)
            + self._published(1792449160, 1792449700000)
        )
        receipt = lane.wait_for_lease_rotation(path, baseline_count=1, budget_s=0.0)
        self.assertFalse(receipt["rotated"])
        self.assertEqual(receipt["publications_for_the_destination"], 1)

    def test_a_republication_without_the_expiry_advance_is_not_a_rotation(self) -> None:
        path = self._write(
            self._published(1792448840, 1792449380000)
            + self._published(1792449160, 1792449380000)
        )
        receipt = lane.wait_for_lease_rotation(path, baseline_count=0, budget_s=0.0)
        self.assertFalse(receipt["rotated"])

    def test_the_rotation_wait_is_bounded(self) -> None:
        path = self._write(self._published(1792448840, 1792449380000))
        receipt = lane.wait_for_lease_rotation(path, baseline_count=0, budget_s=0.0)
        self.assertFalse(receipt["rotated"])
        self.assertEqual(receipt["publications_for_the_destination"], 1)

    def test_the_publication_series_keeps_the_publication_order(self) -> None:
        path = self._write(
            self._published(1792448840, 1792449380000)
            + self._published(1792449160, 1792449700000)
        )
        published, expiry = lane.read_publication_series(path)
        self.assertEqual(published, [1792448840, 1792449160])
        self.assertEqual(expiry, [1792449380000, 1792449700000])


class FakeHeartbeatSocket:
    def __init__(self) -> None:
        self.sent: list[str] = []
        self.replies = ["NAMING REPLY RESULT=OK VALUE=x"]

    def send_line(self, line: str) -> None:
        self.sent.append(line)

    def line(self) -> str:
        return self.replies.pop(0) if self.replies else ""


class ControlSocketHeartbeatTests(unittest.TestCase):
    def test_the_heartbeat_keeps_the_control_socket_busy(self) -> None:
        sam = FakeHeartbeatSocket()
        heartbeat = lane.ControlSocketHeartbeat(sam, interval_s=0.05)
        heartbeat.start()
        deadline = time.monotonic() + 5.0
        while not sam.sent and time.monotonic() < deadline:
            time.sleep(0.01)
        receipt = heartbeat.stop()
        self.assertTrue(sam.sent)
        self.assertEqual(sam.sent[0], "NAMING LOOKUP NAME=ME")
        self.assertEqual(receipt["heartbeat_failures"], 0)
        self.assertGreaterEqual(receipt["heartbeats_sent"], 1)

    def test_a_failed_heartbeat_is_counted_not_hidden(self) -> None:
        class BrokenSocket(FakeHeartbeatSocket):
            def send_line(self, line: str) -> None:
                raise OSError("closed")

        heartbeat = lane.ControlSocketHeartbeat(BrokenSocket(), interval_s=0.05)
        heartbeat.start()
        deadline = time.monotonic() + 5.0
        while heartbeat.failures == 0 and time.monotonic() < deadline:
            time.sleep(0.01)
        receipt = heartbeat.stop()
        self.assertGreaterEqual(receipt["heartbeat_failures"], 1)


if __name__ == "__main__":
    unittest.main()
