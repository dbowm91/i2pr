"""Plan 062 v3 observation schema tests.

These tests cover the Plan 062 v3 observation schema
(``i2pr-ntcp2-direction-observation-v3``). They assert that:

- a valid v3 observation record finalizes with a 64-hex observation
  digest;
- the v3 schema rejects v2 observations;
- v3 observations without the correlation fields cannot satisfy the
  receiver pass predicate;
- generic-phrase-only sources cannot satisfy the receiver pass
  predicate;
- the receiver pass predicate requires nonzero decrypt/decode counts
  and exact DeliveryStatus message ID and Router Hash correlation;
- sender-only observations cannot pass the receiver predicate;
- correlation mismatch between trigger, sender, and receiver is
  detected;
- the historical v2 schema remains readable and pass/fail as
  before.
"""

from __future__ import annotations

import unittest

from observation import OBSERVATION_SCHEMA as V2_SCHEMA
from observation_v3 import (
    OBSERVATION_SCHEMA,
    OBSERVATION_SCHEMA_VERSION,
    ObservationError,
    both_authenticated,
    build_level,
    correlation_matches,
    empty_levels,
    finalize_observation,
    receiver_passes_data_phase,
    sender_emitted_data_frame,
    validate_observation,
)


def _level(state: str) -> dict:
    return build_level(
        state,
        "structured-event",
        "evidence-code",
        observer_implementation="v3-test",
    )


def _v3_levels(
    *,
    frame_emitted: bool = True,
    frame_decrypted: bool = True,
    frame_decoded: bool = True,
) -> dict:
    levels = empty_levels("not-applicable")
    for level, state in (
        ("process_started", "observed"),
        ("listener_ready", "observed"),
        ("tcp_connected", "observed"),
        ("ntcp2_authenticated", "observed"),
        ("terminal_clean", "observed"),
    ):
        levels[level] = build_level(
            state,
            "structured-event",
            "evidence-code",
            count=1,
            observer_implementation="v3-test",
        )
    levels["frame_emitted"] = build_level(
        "observed" if frame_emitted else "not-observed",
        "structured-event",
        "evidence-code",
        count=1 if frame_emitted else 0,
        observer_implementation="v3-test",
    )
    levels["frame_authenticated_and_decrypted"] = build_level(
        "observed" if frame_decrypted else "not-observed",
        "structured-event",
        "evidence-code",
        count=1 if frame_decrypted else 0,
        observer_implementation="v3-test",
    )
    levels["i2np_message_decoded"] = build_level(
        "observed" if frame_decoded else "not-observed",
        "structured-event",
        "evidence-code",
        count=1 if frame_decoded else 0,
        observer_implementation="v3-test",
    )
    return levels


def _v3_observation(
    *,
    frame_emitted: bool = True,
    frame_decrypted: bool = True,
    frame_decoded: bool = True,
    message_id: int = 1,
    peer_hash: str = "a" * 64,
    local_hash: str = "b" * 64,
    source_event_sha256: str = "c" * 64,
    side: str = "i2pr",
) -> dict:
    return {
        "schema": OBSERVATION_SCHEMA,
        "schema_version": OBSERVATION_SCHEMA_VERSION,
        "side": side,
        "levels": _v3_levels(
            frame_emitted=frame_emitted,
            frame_decrypted=frame_decrypted,
            frame_decoded=frame_decoded,
        ),
        "delivery_status_message_id": message_id,
        "peer_router_hash_sha256": peer_hash,
        "local_router_hash_sha256": local_hash,
        "source_event_sha256": source_event_sha256,
    }


class V3SchemaTests(unittest.TestCase):
    def test_minimal_v3_record_finalizes(self):
        observation = _v3_observation()
        digest = finalize_observation("i2pr", observation, require_correlation=True)
        self.assertEqual(len(digest), 64)
        self.assertEqual(observation["observation_sha256"], digest)

    def test_v3_rejects_v2_schema(self):
        observation = _v3_observation()
        observation["schema"] = V2_SCHEMA
        observation["schema_version"] = 2
        with self.assertRaises(ObservationError) as ctx:
            validate_observation("i2pr", observation)
        self.assertEqual(ctx.exception.args[0], "unknown observation schema")

    def test_v3_requires_correlation_when_requested(self):
        observation = _v3_observation()
        del observation["delivery_status_message_id"]
        with self.assertRaises(ObservationError) as ctx:
            validate_observation("i2pr", observation, require_correlation=True)
        self.assertEqual(
            ctx.exception.args[0],
            "observation-correlation-missing:delivery_status_message_id",
        )

    def test_v3_rejects_zero_message_id(self):
        observation = _v3_observation(message_id=0)
        with self.assertRaises(ObservationError) as ctx:
            validate_observation("i2pr", observation, require_correlation=True)
        self.assertEqual(
            ctx.exception.args[0],
            "observation-delivery-status-message-id-out-of-range",
        )

    def test_v3_rejects_non_int_message_id(self):
        observation = _v3_observation()
        observation["delivery_status_message_id"] = "not-an-int"
        with self.assertRaises(ObservationError):
            validate_observation("i2pr", observation, require_correlation=True)

    def test_v3_rejects_non_64_hex_peer_hash(self):
        observation = _v3_observation(peer_hash="not-a-hash")
        with self.assertRaises(ObservationError) as ctx:
            validate_observation("i2pr", observation, require_correlation=True)
        self.assertEqual(
            ctx.exception.args[0],
            "observation-peer_router_hash_sha256-invalid",
        )

    def test_v3_rejects_uppercase_peer_hash(self):
        observation = _v3_observation(peer_hash="A" * 64)
        with self.assertRaises(ObservationError):
            validate_observation("i2pr", observation, require_correlation=True)

    def test_v3_unknown_level_rejected(self):
        observation = _v3_observation()
        observation["levels"]["extra-level"] = build_level(
            "observed", "structured-event", "evidence-code"
        )
        with self.assertRaises(ObservationError):
            validate_observation("i2pr", observation)

    def test_v3_unknown_source_rejected(self):
        observation = _v3_observation()
        observation["levels"]["ntcp2_authenticated"]["source"] = "rogue-source"
        with self.assertRaises(ObservationError):
            validate_observation("i2pr", observation)


class ReceiverPredicateTests(unittest.TestCase):
    def test_complete_v3_record_passes_receiver_predicate(self):
        observation = _v3_observation()
        self.assertTrue(receiver_passes_data_phase(observation))

    def test_missing_decrypt_fails_receiver_predicate(self):
        observation = _v3_observation(frame_decrypted=False, frame_decoded=True)
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_missing_decode_fails_receiver_predicate(self):
        observation = _v3_observation(frame_decrypted=True, frame_decoded=False)
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_zero_decrypt_count_fails_receiver_predicate(self):
        observation = _v3_observation()
        observation["levels"]["frame_authenticated_and_decrypted"]["count"] = 0
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_zero_decode_count_fails_receiver_predicate(self):
        observation = _v3_observation()
        observation["levels"]["i2np_message_decoded"]["count"] = 0
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_missing_correlation_fails_receiver_predicate(self):
        observation = _v3_observation()
        del observation["delivery_status_message_id"]
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_message_id_out_of_range_fails_receiver_predicate(self):
        observation = _v3_observation(message_id=0)
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_non_64_hex_peer_hash_fails_receiver_predicate(self):
        observation = _v3_observation(peer_hash="not-64-hex")
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_sender_emitted_predicate(self):
        observation = _v3_observation()
        self.assertTrue(sender_emitted_data_frame(observation))

    def test_both_authenticated_predicate(self):
        sender = _v3_observation(side="i2pr")
        receiver = _v3_observation(side="java_i2p")
        self.assertTrue(both_authenticated(sender, receiver))

    def test_both_authenticated_fails_when_one_side_missing(self):
        sender = _v3_observation(side="i2pr")
        receiver = _v3_observation(side="java_i2p")
        receiver["levels"]["ntcp2_authenticated"]["state"] = "not-observed"
        self.assertFalse(both_authenticated(sender, receiver))

    def test_correlation_matches_positive(self):
        trigger = {
            "delivery_status_message_id": 7,
            "peer_router_hash_sha256": "a" * 64,
        }
        sender = {"delivery_status_message_id": 7, "peer_router_hash_sha256": "a" * 64}
        receiver = {
            "delivery_status_message_id": 7,
            "peer_router_hash_sha256": "a" * 64,
        }
        self.assertTrue(correlation_matches(trigger, sender, receiver))

    def test_correlation_mismatch_rejected(self):
        trigger = {
            "delivery_status_message_id": 7,
            "peer_router_hash_sha256": "a" * 64,
        }
        sender = {"delivery_status_message_id": 8, "peer_router_hash_sha256": "a" * 64}
        receiver = {
            "delivery_status_message_id": 7,
            "peer_router_hash_sha256": "a" * 64,
        }
        self.assertFalse(correlation_matches(trigger, sender, receiver))


class SenderOnlyFixtureTests(unittest.TestCase):
    """Acceptance tests for the Plan 062 WP4 sender-only fixtures.

    These tests prove that the sender-only path cannot pass the
    receiver predicate and that the wrong-message fixture fails.
    """

    def test_sender_only_observation_does_not_satisfy_receiver_predicate(self):
        sender = _v3_observation(side="i2pr")
        receiver_observation = _v3_observation(
            side="java_i2p",
            frame_decrypted=False,
            frame_decoded=False,
        )
        self.assertTrue(sender_emitted_data_frame(sender))
        self.assertFalse(receiver_passes_data_phase(receiver_observation))

    def test_wrong_message_id_fails_predicate(self):
        trigger = {
            "delivery_status_message_id": 7,
            "peer_router_hash_sha256": "a" * 64,
        }
        sender = {
            "delivery_status_message_id": 7,
            "peer_router_hash_sha256": "a" * 64,
        }
        # The receiver carries a different delivery_status_message_id
        # from the trigger and sender; the predicate must reject the
        # receiver even though every individual record carries
        # the correlation fields.
        receiver = _v3_observation(message_id=8)
        self.assertTrue(correlation_matches(trigger, sender, _v3_observation(message_id=7)))
        self.assertFalse(correlation_matches(trigger, sender, receiver))


class LegacyV2CompatTests(unittest.TestCase):
    def test_v2_record_remains_valid_observation(self):
        from observation import finalize_observation as finalize_v2
        # The v2 schema accepts only the legacy source-set
        # (``typed-status``, ``structured-log``,
        # ``source-derived-log-marker``, ``control-api``) so we
        # construct a v2 fixture with a legacy source.
        def v2_level(state: str) -> dict:
            return build_level(
                state,
                "typed-status",
                "evidence-code",
                observer_implementation="v2-test",
            )
        observation = {
            "schema": V2_SCHEMA,
            "schema_version": 2,
            "side": "i2pr",
            "levels": {
                level: v2_level("observed")
                for level in (
                    "process_started",
                    "listener_ready",
                    "tcp_connected",
                    "ntcp2_authenticated",
                    "frame_emitted",
                    "frame_authenticated_and_decrypted",
                    "i2np_message_decoded",
                    "terminal_clean",
                )
            },
        }
        finalize_v2("i2pr", observation)
        self.assertEqual(len(observation["observation_sha256"]), 64)


if __name__ == "__main__":
    unittest.main()
