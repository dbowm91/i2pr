"""Plan 053 pipeline integration tests."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from evidence_bundle import verify_bundle
from plan052_pipeline import (
    DIAGNOSTIC_RESULT,
    PRIMARY_DIRECTIONS,
    PipelineError,
    finalize_diagnostic_bundle,
    load_context,
    write_direction_artifacts,
    write_environment_block,
)
from run_identity import build_run_identity, write_run_identity


def _identity(run_id: str) -> dict[str, object]:
    return build_run_identity(
        run_id=run_id,
        source_commit="a" * 40,
        source_commit_object_sha256="b" * 64,
        source_tree_sha256="c" * 64,
        source_archive_sha256="d" * 64,
        source_archive_format="git-tar",
        source_dirty="clean",
        host_source_manifest_sha256="e" * 64,
        guest_source_manifest_sha256="f" * 64,
        guest_source_listing_sha256="1" * 64,
        environment_manifest_sha256="2" * 64,
        launcher_binary_sha256="3" * 64,
        launcher_build_profile="test-seam",
        rustc_version="rustc 1.95.0",
        cargo_version="cargo 1.95.0",
        target_triple="x86_64-unknown-linux-gnu",
        topology_kind="rootless-sealed-single-netns",
        privilege_model="unprivileged-userns",
        reference_lock_sha256="4" * 64,
        evidence_schema_revision=2,
        created_at="2026-07-29T00:00:00Z",
    )


def _context(root: Path):
    run_id = "plan053-20260729000000-aabbccdd"
    identity_path = root / "run-identity.json"
    staging = root / "bundle-staging"
    write_run_identity(identity_path, _identity(run_id))
    context = load_context(identity_path, staging)
    write_environment_block(staging, context.identity)
    return context


class Plan053PipelineTests(unittest.TestCase):
    def test_four_blocked_directions_finalize_and_verify(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            context = _context(root)
            for direction in PRIMARY_DIRECTIONS:
                reference = "java_i2p" if "java" in direction else "i2pd"
                initiator = "i2pr" if direction.startswith("i2pr-") else "reference"
                write_direction_artifacts(
                    context,
                    direction,
                    reference=reference,
                    initiator=initiator,
                    result="blocked",
                    reason_code="i2pr-mixed-router-profile-not-wired",
                )
            finalize_diagnostic_bundle(context)
            manifest = verify_bundle(context.staging_root)
            self.assertEqual(len(manifest.files), 1 + 6 + 20 + 1)
            self.assertEqual(
                json.loads((context.staging_root / "diagnostics/sanitized-summary.json").read_text())["result"],
                DIAGNOSTIC_RESULT,
            )

    def test_identity_mutation_blocks_artifact_write(self):
        with tempfile.TemporaryDirectory() as directory:
            context = _context(Path(directory))
            value = json.loads(context.run_identity_path.read_text())
            value["source_commit"] = "9" * 40
            context.run_identity_path.write_text(json.dumps(value))
            with self.assertRaises(PipelineError) as error:
                write_direction_artifacts(
                    context,
                    PRIMARY_DIRECTIONS[0],
                    reference="java_i2p",
                    initiator="i2pr",
                    result="blocked",
                    reason_code="blocked_host_contract",
                )
            self.assertEqual(str(error.exception), "run-identity-mutated-after-freeze")

    def test_missing_direction_class_prevents_finalization(self):
        with tempfile.TemporaryDirectory() as directory:
            context = _context(Path(directory))
            for direction in PRIMARY_DIRECTIONS[:-1]:
                reference = "java_i2p" if "java" in direction else "i2pd"
                write_direction_artifacts(
                    context,
                    direction,
                    reference=reference,
                    initiator="i2pr",
                    result="blocked",
                    reason_code="blocked_host_contract",
                )
            with self.assertRaises(PipelineError):
                finalize_diagnostic_bundle(context)

    def test_unknown_reason_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            context = _context(Path(directory))
            with self.assertRaises(PipelineError):
                write_direction_artifacts(
                    context,
                    PRIMARY_DIRECTIONS[0],
                    reference="java_i2p",
                    initiator="i2pr",
                    result="blocked",
                    reason_code="arbitrary-peer-text",
                )


if __name__ == "__main__":
    unittest.main()
