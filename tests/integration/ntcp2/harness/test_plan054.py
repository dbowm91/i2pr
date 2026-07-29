"""Plan 054 tests for the Java startup matrix, observation catalog, and
per-side observation-v2 records.

These tests cover the new Java matrix helpers, the machine-readable
reference observation catalog, the catalog-driven adapter observation
helpers, and the source-locked control-experiment predicates required
for the Plan 052 directional predicate to pass for an i2pr-initiated
direction.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]


class ObservationCatalogTests(unittest.TestCase):
    def test_catalog_is_revision_locked(self):
        from observation_catalog import (
            SCHEMA,
            REVISION,
            load_catalog,
            observation_catalog_digest,
        )
        catalog = load_catalog()
        self.assertEqual(catalog["schema"], SCHEMA)
        self.assertEqual(catalog["revision"], REVISION)
        self.assertEqual(len(observation_catalog_digest()), 64)

    def test_catalog_entries_have_all_levels(self):
        from observation_catalog import entries_for, load_catalog
        catalog = load_catalog()
        for reference in ("java_i2p", "i2pd"):
            entries = entries_for(catalog, reference)
            self.assertEqual(
                set(entries),
                {"ntcp2_authenticated", "frame_authenticated_and_decrypted", "i2np_message_decoded"},
            )
            for entry in entries.values():
                self.assertGreaterEqual(entry["minimum_count"], 1)
                self.assertTrue(entry["symbol"])

    def test_handshake_marker_cannot_claim_data_level(self):
        from observation_catalog import entries_for, load_catalog
        catalog = load_catalog()
        for reference in ("java_i2p", "i2pd"):
            entries = entries_for(catalog, reference)
            for level, entry in entries.items():
                if level != "ntcp2_authenticated":
                    self.assertNotIn(
                        entry["marker"],
                        {"SessionConfirmed sent", "SessionConfirmed from", "NTCP2 connection established"},
                        f"{reference}.{level} cannot be a handshake marker",
                    )

    def test_sanitize_marker_line_strips_endpoints(self):
        from observation_catalog import sanitize_marker_line
        text = "NTCP2 data frame authenticated and decrypted 192.0.2.1:45678"
        sanitized = sanitize_marker_line(text)
        self.assertNotIn("192.0.2.1", sanitized)
        self.assertIn("synthetic-endpoint", sanitized)

    def test_unrecognized_sanitizer_rejected(self):
        from observation_catalog import CatalogError, validate_catalog
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "catalog.toml"
            path.write_text(
                'schema = "i2pr-reference-observation-catalog-v1"\nrevision = 1\n'
                '[java_i2p]\nversion = "2.12.0"\nrevision = "2800040deee9bb376567b671ef2e9c34cf3e30b6"\n'
                '[[java_i2p.observations]]\nsemantic_level = "ntcp2_authenticated"\n'
                'source_path = "x"\nsymbol = "y"\nmarker = "m"\nsanitization_rule = "nope"\nminimum_count = 1\n',
                encoding="utf-8",
            )
            with self.assertRaises(CatalogError):
                validate_catalog(load_catalog_for_path(path))

    def test_duplicate_semantic_level_rejected(self):
        from observation_catalog import CatalogError, validate_catalog
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "catalog.toml"
            text = (
                'schema = "i2pr-reference-observation-catalog-v1"\nrevision = 1\n'
                '[java_i2p]\nversion = "2.12.0"\nrevision = "2800040deee9bb376567b671ef2e9c34cf3e30b6"\n'
                '[[java_i2p.observations]]\nsemantic_level = "ntcp2_authenticated"\nsource_path = "x"\n'
                'symbol = "y"\nmarker = "SessionConfirmed sent"\nsanitization_rule = "strip-ipv4-endpoint-prefix"\nminimum_count = 1\n'
                '[[java_i2p.observations]]\nsemantic_level = "ntcp2_authenticated"\nsource_path = "x2"\n'
                'symbol = "y2"\nmarker = "NTCP2 connection established"\nsanitization_rule = "strip-ipv4-endpoint-prefix"\nminimum_count = 1\n'
            )
            path.write_text(text, encoding="utf-8")
            with self.assertRaises(CatalogError):
                validate_catalog(load_catalog_for_path(path))

    def test_markdown_drift_detected(self):
        from observation_catalog import observation_catalog_digest, drift_against_markdown
        digest = observation_catalog_digest()
        with tempfile.TemporaryDirectory() as directory:
            markdown = Path(directory) / "doc.md"
            markdown.write_text(
                "# drift\nThe catalog carries " + digest + " or other markers.\n",
                encoding="utf-8",
            )
            self.assertTrue(drift_against_markdown(markdown, (digest, digest)))
            self.assertFalse(drift_against_markdown(markdown, (digest, "0" * 64)))


def load_catalog_for_path(path: Path) -> dict:
    import tomllib

    return tomllib.loads(path.read_text(encoding="utf-8"))


class ObservationHelperTests(unittest.TestCase):
    def _write_log(self, path: Path, lines: list[str]) -> None:
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    def test_handshake_marker_does_not_satisfy_data_phase(self):
        from observation_helpers import LogCursor, build_observation
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "eventlog.txt"
            self._write_log(log, [
                "INFO  NTCP2: SessionConfirmed sent",
                "INFO  NTCP2 connection established",
            ])
            observation = build_observation(
                side="i2pd",
                role="initiator",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                catalog=None,
            )
            from observation import receiver_passes_data_phase
            self.assertFalse(receiver_passes_data_phase(observation))
            self.assertEqual(
                observation["levels"]["ntcp2_authenticated"]["state"], "observed"
            )
            self.assertEqual(
                observation["levels"]["frame_authenticated_and_decrypted"]["state"], "not-observed"
            )

    def test_decrypt_and_decode_markers_satisfy_data_phase(self):
        from observation_helpers import LogCursor, build_observation
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "eventlog.txt"
            self._write_log(log, [
                "INFO  NTCP2: SessionConfirmed sent 192.0.2.1:45678",
                "INFO  NTCP2: data frame authenticated and decrypted 192.0.2.1",
                "INFO  NTCP2: I2NP message decoded messageId=42",
            ])
            observation = build_observation(
                side="i2pd",
                role="responder",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                correlation={"delivery_status_message_id": "42"},
                catalog=None,
            )
            from observation import receiver_passes_data_phase
            self.assertTrue(receiver_passes_data_phase(observation))
            self.assertIn(
                "synthetic-endpoint",
                observation["levels"]["ntcp2_authenticated"]["sanitized_detail"],
            )

    def test_wrong_correlation_does_not_count(self):
        from observation_helpers import LogCursor, build_observation
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "eventlog.txt"
            self._write_log(log, [
                "INFO  NTCP2: data frame authenticated and decrypted",
                "INFO  NTCP2: I2NP message decoded messageId=99",
            ])
            observation = build_observation(
                side="i2pd",
                role="responder",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                correlation={"delivery_status_message_id": "42"},
                catalog=None,
            )
            self.assertEqual(
                observation["levels"]["i2np_message_decoded"]["state"], "not-observed"
            )

    def test_stale_log_marker_not_counted(self):
        from observation_helpers import LogCursor, build_observation
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "eventlog.txt"
            self._write_log(log, [
                "INFO  NTCP2: data frame authenticated and decrypted",
                "INFO  NTCP2: I2NP message decoded",
            ])
            stale = build_observation(
                side="i2pd",
                role="responder",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                catalog=None,
            )
            self.assertEqual(stale["levels"]["i2np_message_decoded"]["state"], "observed")
            truncated = build_observation(
                side="i2pd",
                role="responder",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=log.stat().st_size),
                catalog=None,
            )
            self.assertEqual(truncated["levels"]["i2np_message_decoded"]["state"], "not-observed")

    def test_observation_digest_changes_when_level_changes(self):
        from observation_helpers import LogCursor, build_observation
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "eventlog.txt"
            self._write_log(log, ["INFO  NTCP2: I2NP message decoded"])
            a = build_observation(
                side="i2pd",
                role="responder",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                catalog=None,
            )
            self._write_log(log, ["INFO  NTCP2: I2NP message decoded", "INFO  NTCP2: data frame authenticated and decrypted"])
            b = build_observation(
                side="i2pd",
                role="responder",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                catalog=None,
            )
            self.assertNotEqual(a["observation_sha256"], b["observation_sha256"])


class JavaMatrixTests(unittest.TestCase):
    def test_matrix_cells_cover_16(self):
        from java_matrix import matrix_cells
        cells = matrix_cells()
        self.assertEqual(len(cells), 16)
        self.assertEqual(
            {cell["data_state"] for cell in cells}, {"empty", "seeded-clone"}
        )

    def test_seeded_clone_rejects_template_directory(self):
        from java_startup_probe import ProbeError, _ensure_data_state
        with tempfile.TemporaryDirectory() as directory:
            template = Path(directory) / "template"
            template.mkdir()
            with self.assertRaises(ProbeError) as ctx:
                _ensure_data_state(
                    template=template,
                    data_dir=template,
                    data_state="seeded-clone",
                )
            self.assertEqual(ctx.exception.code, "template-launch-forbidden")

    def test_seeded_clone_copies_template(self):
        from java_startup_probe import _ensure_data_state
        with tempfile.TemporaryDirectory() as directory:
            template = Path(directory) / "template"
            template.mkdir()
            (template / "router.config").write_text("body", encoding="ascii")
            clone = Path(directory) / "clone"
            _ensure_data_state(template=template, data_dir=clone, data_state="seeded-clone")
            self.assertTrue((clone / "router.config").is_file())

    def test_template_manifest_digest_stable(self):
        from java_matrix import build_template_manifest, verify_template_unchanged
        with tempfile.TemporaryDirectory() as directory:
            template = Path(directory) / "template"
            template.mkdir()
            (template / "router.config").write_text("body", encoding="ascii")
            manifest = build_template_manifest(
                template, reference_version="2.12.0", reference_revision="2800040deee9bb376567b671ef2e9c34cf3e30b6"
            )
            self.assertEqual(manifest["schema"], "i2pr-java-template-manifest-v1")
            self.assertTrue(verify_template_unchanged(template, manifest["tree_sha256"]))
            (template / "router.config").write_text("tampered", encoding="ascii")
            self.assertFalse(verify_template_unchanged(template, manifest["tree_sha256"]))

    def test_failure_classifier_maps_known_codes(self):
        from java_startup_probe import FAILURE_STAGES, _classify_failure
        for code in FAILURE_STAGES:
            self.assertEqual(_classify_failure(code), code)
        self.assertEqual(_classify_failure("process-start-failed"), "java-process-spawn-failed")
        self.assertEqual(
            _classify_failure("java-eventlog-started-timeout", launcher="wrapper"),
            "java-wrapper-bootstrap-failed",
        )
        self.assertEqual(
            _classify_failure("java-eventlog-started-timeout", launcher="runplain"),
            "java-listener-readiness-timeout",
        )
        self.assertEqual(
            _classify_failure("java-state-file-mode-not-private"),
            "java-state-permission-invalid",
        )

    def test_entropy_probe_returns_buckets(self):
        from java_startup_probe import _entropy_probe
        result = _entropy_probe()
        self.assertIn("latency_bucket_ms", result)
        self.assertIn(result["latency_bucket_ms"], {"0-10", "10-100", "100-1000", "1000+"})
        self.assertIn(result["class"], {"ok", "degraded", "unavailable"})

    def test_cli_rejects_unkown_namespace_for_seeded_clone(self):
        with tempfile.TemporaryDirectory() as directory:
            install = Path(directory) / "install"
            install.mkdir()
            (install / "runplain.sh").write_text("#!/bin/sh", encoding="ascii")
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "java_startup_probe.py"),
                    "--reference-install", str(install),
                    "--data-dir", str(Path(directory) / "data"),
                    "--data-state", "seeded-clone",
                    "--launcher", "runplain",
                    "--namespace", "rogue",
                    "--output", str(Path(directory) / "out.json"),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)


class AdapterObservationTests(unittest.TestCase):
    def test_java_adapter_collect_observation_returns_v2(self):
        from observation_helpers import build_observation, LogCursor
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "eventlog.txt"
            log.write_text(
                "INFO NTCP2 connection established 192.0.2.1:45678\n"
                "INFO NTCP2 data frame authenticated and decrypted 192.0.2.1\n"
                "INFO NTCP2 I2NP message decoded 192.0.2.1\n",
                encoding="ascii",
            )
            observation = build_observation(
                side="java_i2p",
                role="responder",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                correlation={"delivery_status_message_id": "192.0.2.1"},
            )
            self.assertEqual(observation["schema"], "i2pr-ntcp2-direction-observation-v2")
            self.assertEqual(observation["side"], "java_i2p")
            self.assertEqual(observation["levels"]["ntcp2_authenticated"]["state"], "observed")
            self.assertEqual(observation["levels"]["frame_authenticated_and_decrypted"]["state"], "observed")
            self.assertEqual(observation["levels"]["i2np_message_decoded"]["state"], "observed")

    def test_i2pd_adapter_collect_observation_returns_v2(self):
        from observation_helpers import build_observation, LogCursor
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "i2pd.log"
            log.write_text(
                "INFO NTCP2: SessionConfirmed sent 192.0.2.1:45678\n"
                "INFO NTCP2: data frame authenticated and decrypted\n"
                "INFO NTCP2: I2NP message decoded messageId=42\n",
                encoding="ascii",
            )
            observation = build_observation(
                side="i2pd",
                role="initiator",
                run_id="plan054",
                cursor=LogCursor(run_id="plan054", log_path=log, start_offset=0),
                correlation={"delivery_status_message_id": "42"},
            )
            self.assertEqual(observation["schema"], "i2pr-ntcp2-direction-observation-v2")
            self.assertEqual(observation["side"], "i2pd")
            self.assertEqual(observation["levels"]["ntcp2_authenticated"]["state"], "observed")
            self.assertEqual(observation["levels"]["frame_authenticated_and_decrypted"]["state"], "observed")
            self.assertEqual(observation["levels"]["i2np_message_decoded"]["state"], "observed")


class Plan052PredicateTests(unittest.TestCase):
    def _observation(self, *, handshake: bool, decrypt: bool, decode: bool) -> dict:
        def level(state: str) -> dict:
            return {
                "state": state,
                "source": "source-derived-log-marker",
                "evidence_code": "x",
                "sanitized_detail": "",
                "observer_implementation": "java_i2p-observation-catalog-v1",
            }
        return {
            "schema": "i2pr-ntcp2-direction-observation-v2",
            "schema_version": 2,
            "side": "java_i2p",
            "levels": {
                "process_started": level("observed"),
                "listener_ready": level("observed"),
                "tcp_connected": level("observed"),
                "ntcp2_authenticated": level("observed" if handshake else "not-observed"),
                "frame_emitted": level("not-observed"),
                "frame_authenticated_and_decrypted": level("observed" if decrypt else "not-observed"),
                "i2np_message_decoded": level("observed" if decode else "not-observed"),
                "terminal_clean": level("observed"),
            },
            "observation_sha256": "0" * 64,
        }

    def _terminal(self, result: str = "passed") -> dict:
        return {"result": result, "reason_code": "handshake_authenticated", "counters": {"frames_sent": 1, "frames_received": 1, "i2np_sent": 1, "i2np_received": 1}}

    def test_pass_predicate_requires_decrypt_and_decode(self):
        from mixed_runner import _evaluate_plan052_predicate
        terminal = self._terminal()
        result, reason = _evaluate_plan052_predicate(
            terminal,
            self._observation(handshake=True, decrypt=True, decode=True),
            {"sender_observed": "observed", "receiver_observed": "observed"},
        )
        self.assertEqual((result, reason), ("passed", "mixed-router-direction-authenticated"))

    def test_pass_predicate_rejects_missing_decode(self):
        from mixed_runner import _evaluate_plan052_predicate
        terminal = self._terminal()
        result, reason = _evaluate_plan052_predicate(
            terminal,
            self._observation(handshake=True, decrypt=True, decode=False),
            {"sender_observed": "observed", "receiver_observed": "not-observed"},
        )
        self.assertEqual(result, "rejected")
        self.assertEqual(reason, "reference-i2np-marker-not-source-locked")

    def test_pass_predicate_rejects_handshake_only(self):
        from mixed_runner import _evaluate_plan052_predicate
        terminal = self._terminal()
        result, reason = _evaluate_plan052_predicate(
            terminal,
            self._observation(handshake=True, decrypt=False, decode=False),
            {"sender_observed": "observed", "receiver_observed": "not-observed"},
        )
        self.assertEqual(result, "rejected")
        self.assertEqual(reason, "reference-receiver-marker-not-source-locked")

    def test_pass_predicate_rejects_non_v2_string(self):
        from mixed_runner import _evaluate_plan052_predicate
        terminal = self._terminal()
        result, reason = _evaluate_plan052_predicate(terminal, "authenticated", {"sender_observed": "observed", "receiver_observed": "not-observed"})
        self.assertEqual((result, reason), ("rejected", "reference-receiver-marker-not-source-locked"))


if __name__ == "__main__":
    unittest.main()
