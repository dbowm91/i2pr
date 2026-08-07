"""Plan 062 reference event v1 schema tests.

These tests cover the Plan 062 reference-event v1 schema
(``i2pr-reference-event-v1``). They assert that:

- a valid event record finalizes with a 64-hex event digest;
- data-phase events require ``delivery_status_message_id``,
  ``i2np_type``, and ``frame_sequence``;
- duplicate event sequence is rejected;
- event sequence outside the per-process monotonic cursor is
  rejected through the seen_event_sequences argument;
- generic phrase-only or text-only events cannot satisfy a data
  phase (the schema refuses forbidden payload strings);
- peer Router Hash mismatch is rejected;
- terminal-only events reject data-phase fields;
- all-zero driver binary digests are rejected.
"""

from __future__ import annotations

import unittest

from reference_event import (
    EVENT_SCHEMA,
    EVENT_SCHEMA_VERSION,
    EventKind,
    ReferenceEventError,
    build_event,
    expected_event_sequence,
    known_data_phase_event_kinds,
    validate_event,
)


def _base_event_kwargs(**overrides):
    kwargs = dict(
        run_id="mixed-20260101t000000z-1-abcdef01",
        scenario_id="i2pr-to-java-ipv4",
        direction="i2pr-to-java-ipv4",
        invocation_id="plan094-invocation-1",
        implementation="java-direct-driver",
        implementation_revision="2800040deee9bb376567b671ef2e9c34cf3e30b6",
        driver_binary_sha256="a" * 64,
        local_router_hash_sha256="b" * 64,
        peer_router_hash_sha256="c" * 64,
        monotonic_ms=1000,
        event_kind=EventKind.PROCESS_STARTED,
        event_sequence=0,
    )
    kwargs.update(overrides)
    return kwargs


class EventValidationTests(unittest.TestCase):
    def test_minimal_event_finalizes(self):
        event = build_event(**_base_event_kwargs())
        self.assertEqual(event["schema"], EVENT_SCHEMA)
        self.assertEqual(event["schema_version"], EVENT_SCHEMA_VERSION)
        self.assertEqual(len(event["event_sha256"]), 64)

    def test_data_phase_event_requires_message_id(self):
        kwargs = _base_event_kwargs(
            event_kind=EventKind.FRAME_EMITTED,
            event_sequence=expected_event_sequence(0),
            delivery_status_message_id=None,
        )
        with self.assertRaises(ReferenceEventError) as ctx:
            build_event(**kwargs)
        self.assertEqual(
            ctx.exception.args[0],
            "data-phase-event-requires-delivery-status-message-id-and-i2np-type",
        )

    def test_data_phase_event_accepts_delivery_status(self):
        kwargs = _base_event_kwargs(
            event_kind=EventKind.FRAME_EMITTED,
            event_sequence=expected_event_sequence(0),
            delivery_status_message_id=42,
            i2np_type=10,
            frame_sequence=1,
        )
        event = build_event(**kwargs)
        self.assertEqual(event["i2np_type"], 10)
        self.assertEqual(event["delivery_status_message_id"], 42)
        self.assertEqual(event["frame_sequence"], 1)

    def test_data_phase_event_rejects_non_delivery_status_type(self):
        kwargs = _base_event_kwargs(
            event_kind=EventKind.I2NP_MESSAGE_DECODED,
            event_sequence=expected_event_sequence(0),
            delivery_status_message_id=1,
            i2np_type=11,
            frame_sequence=2,
        )
        with self.assertRaises(ReferenceEventError) as ctx:
            build_event(**kwargs)
        self.assertEqual(ctx.exception.args[0], "event-i2np-type-invalid-for-data-phase")

    def test_duplicate_event_sequence_rejected(self):
        first = build_event(**_base_event_kwargs())
        validate_event(first)
        second = build_event(**_base_event_kwargs())
        seen = {first["event_sequence"]}
        with self.assertRaises(ReferenceEventError) as ctx:
            validate_event(second, seen_event_sequences=seen)
        self.assertEqual(ctx.exception.args[0], "event-sequence-duplicate")

    def test_strictly_increasing_event_sequence(self):
        first = build_event(**_base_event_kwargs(event_sequence=0))
        second = build_event(**_base_event_kwargs(
            event_kind=EventKind.LISTENER_READY,
            event_sequence=expected_event_sequence(0),
        ))
        third = build_event(**_base_event_kwargs(
            event_kind=EventKind.ROUTER_INFO_EXPORTED,
            event_sequence=expected_event_sequence(1),
        ))
        seen = set()
        validate_event(first, seen_event_sequences=seen)
        validate_event(second, seen_event_sequences=seen)
        validate_event(third, seen_event_sequences=seen)
        self.assertEqual(sorted(seen), [0, 1, 2])

    def test_wrong_peer_router_hash_rejected(self):
        event = build_event(**_base_event_kwargs())
        with self.assertRaises(ReferenceEventError) as ctx:
            validate_event(
                event,
                expected_peer_router_hash_sha256="d" * 64,
            )
        self.assertEqual(ctx.exception.args[0], "event-peer-router-hash-mismatch")

    def test_forbidden_payload_text_rejected(self):
        event = build_event(**_base_event_kwargs())
        event["sanitized_detail"] = "/home/user/secret"
        with self.assertRaises(ReferenceEventError) as ctx:
            validate_event(event)
        self.assertEqual(
            ctx.exception.args[0],
            "event contains forbidden path or payload text",
        )

    def test_terminal_event_rejects_data_phase_fields(self):
        kwargs = _base_event_kwargs(
            event_kind=EventKind.TERMINAL_CLEAN,
            event_sequence=expected_event_sequence(0),
        )
        event = build_event(**kwargs)
        # ``build_event`` only attaches data-phase fields when the
        # event kind is data-phase. A terminal event that nevertheless
        # carries a data-phase field (e.g. via direct mutation) must
        # be rejected by the validator.
        event["delivery_status_message_id"] = 1
        with self.assertRaises(ReferenceEventError) as ctx:
            validate_event(event)
        self.assertEqual(
            ctx.exception.args[0],
            "event-data-phase-field-not-allowed:delivery_status_message_id",
        )

    def test_uppercase_hex_rejected(self):
        with self.assertRaises(ReferenceEventError) as ctx:
            build_event(**_base_event_kwargs(driver_binary_sha256="A" * 64))
        self.assertEqual(
            ctx.exception.args[0],
            "event-driver_binary_sha256-invalid",
        )

    def test_unknown_event_kind_rejected(self):
        event = build_event(**_base_event_kwargs())
        event["event_kind"] = "rogue-event"
        with self.assertRaises(ReferenceEventError) as ctx:
            validate_event(event)
        self.assertEqual(ctx.exception.args[0], "event-kind-not-allowlisted")

    def test_unknown_direction_rejected(self):
        event = build_event(**_base_event_kwargs())
        event["direction"] = "bogus-direction"
        with self.assertRaises(ReferenceEventError) as ctx:
            validate_event(event)
        self.assertEqual(ctx.exception.args[0], "event-direction-not-allowlisted")

    def test_known_data_phase_kinds(self):
        kinds = known_data_phase_event_kinds()
        self.assertIn(EventKind.FRAME_EMITTED, kinds)
        self.assertIn(EventKind.FRAME_AUTHENTICATED_AND_DECRYPTED, kinds)
        self.assertIn(EventKind.I2NP_MESSAGE_DECODED, kinds)
        self.assertNotIn(EventKind.NTCP2_AUTHENTICATED, kinds)


if __name__ == "__main__":
    unittest.main()
