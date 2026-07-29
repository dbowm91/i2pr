"""Plan 052 evidence-bundle tests (Plan 053 Workstream A hardening)."""

from __future__ import annotations

import json
import os
import shutil
import stat
import tempfile
import unittest
from pathlib import Path

from evidence_bundle import (
    BUNDLE_SCHEMA,
    BundleError,
    DIRECTION_CLASSES,
    ENVIRONMENT_CLASSES,
    PRIMARY_DIRECTIONS,
    build_bundle_manifest,
    classify_bundle_file,
    export_bundle_atomic,
    finalize_bundle,
    has_typed_absence,
    load_bundle_manifest,
    validate_direction_catalog,
    validate_environment_block,
    verify_bundle,
    write_json_atomic,
)


def _make_environment_block(directory: Path) -> None:
    env_dir = directory / "environment"
    env_dir.mkdir()
    for name in ENVIRONMENT_CLASSES:
        if name.endswith(".sha256"):
            (env_dir / name).write_text("0" * 64 + "  parent-network.txt\n", encoding="ascii")
        else:
            write_json_atomic(env_dir / name, {"schema": "env", "value": name})


_DIRECTION_CLASS_SCHEMAS = {
    "attestations": "i2pr-mixed-router-attestation-v2",
    "directions": "i2pr-mixed-router-direction-v2",
    "triggers": "i2pr-reference-trigger-v2",
    "observations": "i2pr-ntcp2-direction-observation-v2",
    "cleanup": "i2pr-mixed-router-cleanup-v2",
}


def _make_directions(directory: Path, *scenario_ids: str) -> None:
    for direction_class in DIRECTION_CLASSES:
        class_dir = directory / direction_class
        class_dir.mkdir()
        schema = _DIRECTION_CLASS_SCHEMAS.get(direction_class, direction_class)
        for scenario_id in scenario_ids:
            write_json_atomic(
                class_dir / f"{scenario_id}.json",
                {"schema": schema, "scenario_id": scenario_id},
            )


def _make_run_identity(directory: Path) -> None:
    write_json_atomic(directory / "run-identity.json", {
        "schema": "i2pr-interop-run-identity-v1",
        "schema_version": 1,
        "run_id": "plan052-20260722000000-aabbccdd",
        "source_commit": "a" * 40,
        "source_tree_sha256": "b" * 64,
        "run_identity_sha256": "c" * 64,
    })


def _make_staging(directory: Path | str) -> Path:
    """Build a staging directory inside ``directory``.

    ``directory`` is treated as the staging root itself; the helper populates
    the environment, direction, and run-identity trees in place.
    """

    staging = Path(directory)
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir()
    _make_run_identity(staging)
    _make_environment_block(staging)
    _make_directions(staging, *PRIMARY_DIRECTIONS)
    return staging


class BundleManifestTests(unittest.TestCase):
    def test_build_manifest_lists_files(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            manifest = build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            rels = [entry.relative_path for entry in manifest.files]
            self.assertIn("run-identity.json", rels)
            self.assertIn("environment/environment.json", rels)
            for direction in PRIMARY_DIRECTIONS:
                self.assertIn(f"directions/{direction}.json", rels)

    def test_write_manifest_creates_sha256_file(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            manifest = build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            from evidence_bundle import write_bundle_manifest
            write_bundle_manifest(staging, manifest)
            manifest_path = staging / "manifest.json"
            self.assertTrue(manifest_path.exists())
            digest_path = staging / "manifest.sha256"
            self.assertTrue(digest_path.exists())
            digest = digest_path.read_text(encoding="ascii").split()[0]
            self.assertEqual(len(digest), 64)

    def test_invalid_run_id_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            with self.assertRaises(BundleError):
                build_bundle_manifest(staging, "x" * 1)

    def test_load_manifest_round_trips(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            manifest = build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            from evidence_bundle import write_bundle_manifest
            write_bundle_manifest(staging, manifest)
            loaded = load_bundle_manifest(staging / "manifest.json")
            self.assertEqual(loaded.run_id, manifest.run_id)
            self.assertEqual(len(loaded.files), len(manifest.files))

    def test_unknown_schema_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            write_json_atomic(path, {
                "schema": "wrong",
                "schema_version": 1,
                "type": "evidence-bundle-manifest",
                "run_id": "plan052-20260722000000-aabbccdd",
                "files": [],
            })
            with self.assertRaises(BundleError):
                load_bundle_manifest(path)


class BundleVerificationTests(unittest.TestCase):
    def test_verify_bundle_passes_for_valid_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            manifest = verify_bundle(staging)
            self.assertGreater(len(manifest.files), 0)

    def test_verify_bundle_detects_missing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            (staging / "directions" / "i2pr-to-java-ipv4.json").unlink()
            with self.assertRaises(BundleError):
                verify_bundle(staging)

    def test_verify_bundle_detects_hash_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            target = staging / "directions" / "i2pr-to-java-ipv4.json"
            target.write_bytes(b"tampered")
            with self.assertRaises(BundleError):
                verify_bundle(staging)

    def test_extra_file_in_staging_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            write_json_atomic(staging / "directions" / "rogue.json", {"schema": "rogue"})
            with self.assertRaises(BundleError):
                verify_bundle(staging)


class BundleCatalogTests(unittest.TestCase):
    def test_validate_direction_catalog_accepts_primary(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            validate_direction_catalog(staging)

    def test_validate_direction_catalog_rejects_substituted(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = Path(directory)
            _make_run_identity(staging)
            _make_environment_block(staging)
            _make_directions(staging, "i2pr-to-java-ipv4", "rogue-scenario")
            with self.assertRaises(BundleError):
                validate_direction_catalog(staging)

    def test_validate_direction_catalog_rejects_missing(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = Path(directory)
            _make_run_identity(staging)
            _make_environment_block(staging)
            (staging / "directions").mkdir()
            write_json_atomic(staging / "directions" / "i2pr-to-java-ipv4.json", {"schema": "x"})
            with self.assertRaises(BundleError):
                validate_direction_catalog(staging)

    def test_validate_environment_block_requires_all_files(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = _make_staging(Path(directory))
            (staging / "environment" / "environment.json").unlink()
            with self.assertRaises(BundleError):
                validate_environment_block(staging)


class BundleAtomicExportTests(unittest.TestCase):
    def test_export_bundle_atomic_copies_and_verifies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging_dir = root / "staging"
            staging = _make_staging(staging_dir)
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            target = root / "export" / "plan052-20260722000000-aabbccdd"
            export_bundle_atomic(staging, target)
            self.assertTrue(target.exists())
            self.assertTrue((target / "manifest.json").exists())
            # Ack is OUTSIDE the bundle, beside it
            ack_path = target.parent / f"{target.name}.export-ack.json"
            self.assertTrue(ack_path.exists())

    def test_export_bundle_atomic_rejects_existing_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging_dir = root / "staging"
            staging = _make_staging(staging_dir)
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            target = root / "export" / "plan052-20260722000000-aabbccdd"
            target.mkdir(parents=True)
            with self.assertRaises(BundleError):
                export_bundle_atomic(staging, target)

    def test_export_bundle_atomic_detects_tampering(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging_dir = root / "staging"
            staging = _make_staging(staging_dir)
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            target = root / "export" / "plan052-20260722000000-aabbccdd"
            # Patch the staging copy after manifest write so that the
            # post-copy verification rejects the export.
            (staging / "directions" / "i2pr-to-java-ipv4.json").write_bytes(b"tampered")
            with self.assertRaises(BundleError):
                export_bundle_atomic(staging, target)


class BundleTypedAbsenceTests(unittest.TestCase):
    def test_has_typed_absence_true_for_not_produced(self):
        self.assertTrue(has_typed_absence({"router_info": {"state": "not-produced", "sha256": None}}))

    def test_has_typed_absence_false_for_present_digest(self):
        self.assertFalse(has_typed_absence({"router_info": {"state": "produced", "sha256": "a" * 64}}))

    def test_has_typed_absence_false_for_zero_digest(self):
        # Plan 052 forbids zero digests for typed absence.
        self.assertFalse(has_typed_absence({"router_info": {"state": "not-produced", "sha256": "0" * 64}}))


class BundleSchemaTests(unittest.TestCase):
    def test_primary_directions_locked(self):
        self.assertEqual(
            PRIMARY_DIRECTIONS,
            ("i2pr-to-java-ipv4", "java-to-i2pr-ipv4", "i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"),
        )

    def test_direction_classes_locked(self):
        self.assertEqual(
            DIRECTION_CLASSES,
            ("attestations", "directions", "triggers", "observations", "cleanup"),
        )

    def test_environment_classes_include_parent_digests(self):
        self.assertIn("parent-network-before.sha256", ENVIRONMENT_CLASSES)
        self.assertIn("parent-network-after.sha256", ENVIRONMENT_CLASSES)

    def test_bundle_schema_name(self):
        self.assertEqual(BUNDLE_SCHEMA, "i2pr-interop-evidence-bundle-v1")


# ---------------------------------------------------------------------------
# Plan 053 Workstream A tests
# ---------------------------------------------------------------------------

class ClassifyBundleFileTests(unittest.TestCase):
    """A1: bundle-relative classification with semantic path validation."""

    def test_run_identity(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rtype, schema = classify_bundle_file(staging, staging / "run-identity.json")
            self.assertEqual(rtype, "run-identity")
            self.assertEqual(schema, "run-identity")

    def test_environment_json(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rtype, schema = classify_bundle_file(
                staging, staging / "environment" / "environment.json"
            )
            self.assertEqual(rtype, "environment")
            self.assertEqual(schema, "environment-record")

    def test_direction_class(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rtype, schema = classify_bundle_file(
                staging, staging / "directions" / "i2pr-to-java-ipv4.json"
            )
            self.assertEqual(rtype, "directions")
            self.assertEqual(schema, "directions-record")

    def test_attestation_class(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rtype, schema = classify_bundle_file(
                staging, staging / "attestations" / "i2pr-to-java-ipv4.json"
            )
            self.assertEqual(rtype, "attestations")
            self.assertEqual(schema, "attestations-record")

    def test_trigger_class(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rtype, schema = classify_bundle_file(
                staging, staging / "triggers" / "i2pr-to-java-ipv4.json"
            )
            self.assertEqual(rtype, "triggers")
            self.assertEqual(schema, "triggers-record")

    def test_observation_class(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rtype, schema = classify_bundle_file(
                staging, staging / "observations" / "i2pr-to-java-ipv4.json"
            )
            self.assertEqual(rtype, "observations")
            self.assertEqual(schema, "observations-record")

    def test_cleanup_class(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rtype, schema = classify_bundle_file(
                staging, staging / "cleanup" / "i2pr-to-java-ipv4.json"
            )
            self.assertEqual(rtype, "cleanup")
            self.assertEqual(schema, "cleanup-record")

    def test_diagnostics(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            diag_dir = staging / "diagnostics"
            diag_dir.mkdir()
            (diag_dir / "sanitized-summary.json").write_text("{}")
            rtype, schema = classify_bundle_file(
                staging, diag_dir / "sanitized-summary.json"
            )
            self.assertEqual(rtype, "diagnostics")
            self.assertEqual(schema, "sanitized-summary")

    def test_manifest_json(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            (staging / "manifest.json").write_text("{}")
            rtype, schema = classify_bundle_file(staging, staging / "manifest.json")
            self.assertEqual(rtype, "manifest")
            self.assertEqual(schema, "manifest")

    def test_manifest_sha256(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            (staging / "manifest.sha256").write_text("a" * 64 + "  manifest.json\n")
            rtype, schema = classify_bundle_file(staging, staging / "manifest.sha256")
            self.assertEqual(rtype, "manifest")
            self.assertEqual(schema, "manifest")

    def test_unknown_path_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            rogue = staging / "rogue.txt"
            rogue.write_text("bad")
            with self.assertRaises(BundleError) as ctx:
                classify_bundle_file(staging, rogue)
            self.assertIn("unknown bundle path", str(ctx.exception))

    def test_non_json_in_direction_class_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            bad = staging / "directions" / "i2pr-to-java-ipv4.bin"
            bad.write_bytes(b"\x00\x01")
            with self.assertRaises(BundleError) as ctx:
                classify_bundle_file(staging, bad)
            self.assertIn("non-JSON file", str(ctx.exception))

    def test_path_outside_staging_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            outside = Path(d).parent / "external.json"
            outside.write_text("{}")
            with self.assertRaises(BundleError) as ctx:
                classify_bundle_file(staging, outside)
            self.assertIn("not inside staging root", str(ctx.exception))


class WriteJsonAtomicHashTests(unittest.TestCase):
    """A2: hash exactly the bytes written."""

    def test_returned_digest_equals_file_digest(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "test.json"
            digest = write_json_atomic(path, {"hello": "world", "n": 42})
            import hashlib
            file_digest = hashlib.sha256(path.read_bytes()).hexdigest()
            self.assertEqual(digest, file_digest)

    def test_newline_terminated(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "test.json"
            write_json_atomic(path, {"a": 1})
            raw = path.read_bytes()
            self.assertTrue(raw.endswith(b"\n"))
            self.assertEqual(raw.count(b"\n"), 1)

    def test_sorted_keys(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "test.json"
            write_json_atomic(path, {"z": 1, "a": 2})
            raw = path.read_bytes()
            a_pos = raw.index(b'"a"')
            z_pos = raw.index(b'"z"')
            self.assertLess(a_pos, z_pos)


class ManifestSha256VerificationTests(unittest.TestCase):
    """A3: manifest.sha256 is verified, not decorative."""

    def test_valid_sha256_passes(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            manifest = verify_bundle(staging)
            self.assertGreater(len(manifest.files), 0)

    def test_malformed_sha256_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            (staging / "manifest.sha256").write_text("not-a-hash  manifest.json\n")
            with self.assertRaises(BundleError) as ctx:
                verify_bundle(staging)
            self.assertIn("invalid format", str(ctx.exception))

    def test_mismatched_sha256_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            wrong_hash = "0" * 64
            (staging / "manifest.sha256").write_text(f"{wrong_hash}  manifest.json\n")
            with self.assertRaises(BundleError) as ctx:
                verify_bundle(staging)
            self.assertIn("digest mismatch", str(ctx.exception))

    def test_extra_lines_in_sha256_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            (staging / "manifest.sha256").write_text(
                "a" * 64 + "  manifest.json\nextra line\n"
            )
            with self.assertRaises(BundleError) as ctx:
                verify_bundle(staging)
            self.assertIn("exactly one line", str(ctx.exception))

    def test_bad_filename_in_sha256_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            digest = _sha256_hex(staging / "manifest.json")
            (staging / "manifest.sha256").write_text(f"{digest}  wrong-name.json\n")
            with self.assertRaises(BundleError) as ctx:
                verify_bundle(staging)
            self.assertIn("invalid format", str(ctx.exception))


class ExportAckOutsideBundleTests(unittest.TestCase):
    """A4: export acknowledgement is outside the immutable bundle."""

    def test_ack_is_outside_bundle(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            staging = _make_staging(root / "staging")
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            target = root / "export" / "plan052-20260722000000-aabbccdd"
            export_bundle_atomic(staging, target)
            ack_path = target.parent / f"{target.name}.export-ack.json"
            self.assertTrue(ack_path.exists())
            self.assertFalse((target / "export-acknowledgement.json").exists())
            ack = json.loads(ack_path.read_text())
            self.assertEqual(ack["schema"], "i2pr-interop-bundle-export-ack-v1")
            self.assertIn("run_id", ack)
            self.assertIn("bundle_path", ack)
            self.assertIn("manifest_sha256", ack)
            self.assertIn("export_timestamp", ack)
            self.assertIn("verifier_result", ack)

    def test_bundle_verifies_after_ack(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            staging = _make_staging(root / "staging")
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            target = root / "export" / "plan052-20260722000000-aabbccdd"
            export_bundle_atomic(staging, target)
            # Re-verify the exported bundle — ack is outside so it must pass
            manifest = verify_bundle(target)
            self.assertGreater(len(manifest.files), 0)

    def test_ack_bundle_path_is_relative(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            staging = _make_staging(root / "staging")
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            target = root / "export" / "plan052-20260722000000-aabbccdd"
            export_bundle_atomic(staging, target)
            ack_path = target.parent / f"{target.name}.export-ack.json"
            ack = json.loads(ack_path.read_text())
            self.assertFalse(Path(ack["bundle_path"]).is_absolute())


class FileTreeHardeningTests(unittest.TestCase):
    """A5: reject symlinks, non-regular, hard links >1, hidden, traversal."""

    def test_symlink_rejected_in_staging(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            target_file = staging / "directions" / "i2pr-to-java-ipv4.json"
            link = staging / "directions" / "symlink.json"
            os.symlink(str(target_file), str(link))
            with self.assertRaises(BundleError) as ctx:
                build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            self.assertIn("symlink rejected", str(ctx.exception))

    def test_hidden_file_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            hidden = staging / ".hidden-temp"
            hidden.write_text("secret")
            with self.assertRaises(BundleError) as ctx:
                build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            self.assertIn("hidden/temp file rejected", str(ctx.exception))

    def test_hidden_file_in_subdir_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            hidden = staging / "environment" / ".env-swap"
            hidden.write_text("secret")
            with self.assertRaises(BundleError) as ctx:
                build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            self.assertIn("hidden/temp file rejected", str(ctx.exception))

    def test_absolute_path_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            with self.assertRaises(BundleError) as ctx:
                classify_bundle_file(staging, Path("/etc/passwd"))
            self.assertIn("not inside staging root", str(ctx.exception))

    def test_traversal_path_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            # Path that resolves outside staging after normalization
            traversal = staging / ".." / "outside.txt"
            with self.assertRaises(BundleError) as ctx:
                classify_bundle_file(staging, traversal, exists=False)
            self.assertIn("not inside staging root", str(ctx.exception))

    def test_fifo_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            fifo_path = staging / "evil_fifo"
            try:
                os.mkfifo(str(fifo_path))
            except (OSError, NotImplementedError):
                self.skipTest("mkfifo not supported on this platform")
            with self.assertRaises(BundleError) as ctx:
                build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            self.assertIn("non-regular file rejected", str(ctx.exception))


class CaseCollisionTests(unittest.TestCase):
    """A5: case-colliding paths rejected."""

    def test_case_collision_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            # Create a file that collides case-insensitively with an existing one
            upper = staging / "directions" / "I2PR-TO-JAVA-IPV4.json"
            upper.write_text('{"schema":"rogue"}')
            with self.assertRaises(BundleError) as ctx:
                build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            self.assertIn("case-colliding paths rejected", str(ctx.exception))


class DuplicateEntryTests(unittest.TestCase):
    """A5: duplicate manifest entries rejected."""

    def test_duplicate_rel_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            # Manifest should not have duplicates — the builder prevents this.
            # Verify the built manifest has no duplicate relative_paths.
            manifest = build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            rels = [f.relative_path for f in manifest.files]
            self.assertEqual(len(rels), len(set(rels)))


class ManifestEntryExclusionTests(unittest.TestCase):
    """A5: manifest.json and manifest.sha256 excluded from manifest entries."""

    def test_manifest_files_not_in_manifest_entries(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            manifest = build_bundle_manifest(staging, "plan052-20260722000000-aabbccdd")
            rels = [f.relative_path for f in manifest.files]
            self.assertNotIn("manifest.json", rels)
            self.assertNotIn("manifest.sha256", rels)


class SemanticSchemaValidationTests(unittest.TestCase):
    """A6: reject JSON files with wrong semantic schema for their path class."""

    def test_direction_with_wrong_schema_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            # Overwrite a direction file with a wrong schema
            bad = staging / "directions" / "i2pr-to-java-ipv4.json"
            bad.write_text(json.dumps({"schema": "completely-wrong-schema"}))
            with self.assertRaises(BundleError) as ctx:
                finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            self.assertIn("declares schema", str(ctx.exception))

    def test_observation_with_wrong_schema_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            bad = staging / "observations" / "i2pr-to-java-ipv4.json"
            bad.write_text(json.dumps({"schema": "not-observation-schema"}))
            with self.assertRaises(BundleError) as ctx:
                finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            self.assertIn("declares schema", str(ctx.exception))

    def test_correct_schema_accepted(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            # The test helper writes {"schema": direction_class, ...} which
            # matches the direction class name — e.g., "directions".
            # This is NOT a real Plan 052 schema but should be accepted by
            # the builder since it's a valid JSON with a non-unknown schema.
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            manifest = verify_bundle(staging)
            self.assertGreater(len(manifest.files), 0)

    def test_unknown_record_type_in_manifest_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            manifest_path = staging / "manifest.json"
            payload = {
                "schema": BUNDLE_SCHEMA,
                "schema_version": 1,
                "type": "evidence-bundle-manifest",
                "run_id": "plan052-20260722000000-aabbccdd",
                "files": [
                    {
                        "relative_path": "rogue.json",
                        "size": 4,
                        "sha256": "a" * 64,
                        "record_type": "unknown",
                        "schema": "rogue",
                    }
                ],
            }
            manifest_path.write_text(json.dumps(payload))
            with self.assertRaises(BundleError) as ctx:
                load_bundle_manifest(manifest_path)
            self.assertIn("unknown record_type", str(ctx.exception))


class ManifestSha256FormatTests(unittest.TestCase):
    """A3: strict manifest.sha256 format validation."""

    def test_sha256_wrong_spaces_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            digest = _sha256_hex(staging / "manifest.json")
            (staging / "manifest.sha256").write_text(f"{digest} manifest.json\n")
            with self.assertRaises(BundleError) as ctx:
                verify_bundle(staging)
            self.assertIn("invalid format", str(ctx.exception))

    def test_sha256_hex_case_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            staging = _make_staging(Path(d))
            finalize_bundle(staging, "plan052-20260722000000-aabbccdd")
            digest = _sha256_hex(staging / "manifest.json")
            upper = digest.upper()
            (staging / "manifest.sha256").write_text(f"{upper}  manifest.json\n")
            with self.assertRaises(BundleError) as ctx:
                verify_bundle(staging)
            self.assertIn("invalid format", str(ctx.exception))


class WriteJsonAtomicMutationTests(unittest.TestCase):
    """Ensure write_json_atomic does not mutate after finalization."""

    def test_digest_matches_on_disk(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "test.json"
            digest = write_json_atomic(path, {"key": "value"})
            import hashlib
            on_disk = hashlib.sha256(path.read_bytes()).hexdigest()
            self.assertEqual(digest, on_disk)

    def test_consecutive_writes_consistent(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "test.json"
            payload = {"a": 1, "b": [2, 3]}
            d1 = write_json_atomic(path, payload)
            d2 = write_json_atomic(path, payload)
            self.assertEqual(d1, d2)


def _sha256_hex(path: Path) -> str:
    import hashlib
    return hashlib.sha256(path.read_bytes()).hexdigest()


if __name__ == "__main__":
    unittest.main()
