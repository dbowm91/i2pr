"""Plan 052 evidence-bundle tests."""

from __future__ import annotations

import json
import shutil
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


def _make_directions(directory: Path, *scenario_ids: str) -> None:
    for direction_class in DIRECTION_CLASSES:
        class_dir = directory / direction_class
        class_dir.mkdir()
        for scenario_id in scenario_ids:
            write_json_atomic(
                class_dir / f"{scenario_id}.json",
                {"schema": direction_class, "scenario_id": scenario_id},
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
            self.assertTrue((target / "export-acknowledgement.json").exists())

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


if __name__ == "__main__":
    unittest.main()