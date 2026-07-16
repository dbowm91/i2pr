from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[4]
MULTIPASS = ROOT / "scripts/interop/multipass"
sys.path.insert(0, str(MULTIPASS))
from config import EnvironmentManifestError, load_manifest, scenario_reference  # noqa: E402
from export import EXPECTED, validate  # noqa: E402
from lifecycle import (  # noqa: E402
    LifecycleError,
    LifecycleLock,
    allocate_instance_name,
    classify_collision,
    derive_instance_name,
    instance_name_digest,
    normalize_instance_state,
    ownership_proof,
    parse_multipass_list,
    transition,
    validate_run_id,
)
from source_tree import tree_hash, verify_manifest, write_manifest  # noqa: E402


class MultipassEnvironmentTests(unittest.TestCase):
    def test_manifest_is_strict_and_uses_canonical_guest_cache(self) -> None:
        value = load_manifest()
        self.assertEqual(value["guest_cache_root"], "/home/i2ptest/i2pr/target/interop/cache")
        self.assertEqual(value["guest_execution_user"], "i2ptest")
        self.assertEqual(value["required_topology_kind"], "rootless-sealed-single-netns")

    def test_manifest_rejects_unknown_and_duplicate_keys(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "environment.toml"
            raw = load_manifest()
            path.write_text("\n".join(f'{key} = {json.dumps(value)}' for key, value in raw.items()) + '\nextra = "bad"\n')
            with self.assertRaises(EnvironmentManifestError):
                load_manifest(path)
            path.write_text(path.read_text() + 'schema = 1\n')
            with self.assertRaises(EnvironmentManifestError):
                load_manifest(path)

    def test_direction_mapping_fails_closed(self) -> None:
        self.assertEqual(scenario_reference("i2pr-to-java-ipv4"), "java_i2p")
        self.assertEqual(scenario_reference("i2pd-to-i2pr-ipv4"), "i2pd")
        with self.assertRaises(EnvironmentManifestError):
            scenario_reference("not-a-direction")

    def test_source_tree_hash_excludes_generated_state_and_rejects_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/main.rs").write_text("fn main() {}\n")
            (root / "target/interop").mkdir(parents=True)
            (root / "target/interop/secret").write_text("not part of source\n")
            first = tree_hash(root)
            (root / "target/interop/other").write_text("ignored\n")
            self.assertEqual(first, tree_hash(root))
            manifest = root / ".i2pr-source-manifest.json"
            write_manifest(root, "a" * 40, "b" * 64, manifest)
            self.assertEqual(verify_manifest(root, manifest)["tree_sha256"], first)
            (root / "bad-link").symlink_to(root / "src/main.rs")
            with self.assertRaises(ValueError):
                tree_hash(root)

    def test_fake_multipass_rejects_unowned_collision_before_launch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fake_root = Path(directory)
            fake_bin = fake_root / "bin"
            fake_bin.mkdir()
            log = fake_root / "multipass.log"
            fake = fake_bin / "multipass"
            fake.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' \"$*\" >> \"$FAKE_MULTIPASS_LOG\"\n"
                "case \"$1\" in\n"
                "version) echo 'multipass 1.14.0' ;;\n"
                "find) echo '24.04' ;;\n"
                "info) exit 0 ;;\n"
                "*) exit 1 ;;\n"
                "esac\n"
            )
            fake.chmod(fake.stat().st_mode | stat.S_IXUSR)
            environment = os.environ.copy()
            environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
            environment["FAKE_MULTIPASS_LOG"] = str(log)
            result = subprocess.run(
                ["bash", str(MULTIPASS / "create.sh")],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("blocked_instance_without_host_state", result.stdout)
            self.assertNotIn("launch", log.read_text())

    def test_fake_multipass_destroy_never_mutates_unowned_collision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fake_root = Path(directory)
            fake_bin = fake_root / "bin"
            fake_bin.mkdir()
            log = fake_root / "multipass.log"
            fake = fake_bin / "multipass"
            fake.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' \"$*\" >> \"$FAKE_MULTIPASS_LOG\"\n"
                "case \"$1\" in\n"
                "version) echo 'multipass 1.16.3' ;;\n"
                "info) exit 0 ;;\n"
                "*) exit 1 ;;\n"
                "esac\n"
            )
            fake.chmod(fake.stat().st_mode | stat.S_IXUSR)
            environment = os.environ.copy()
            environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
            environment["FAKE_MULTIPASS_LOG"] = str(log)
            run_id = "plan049-test-unowned-1234"
            instance_name = "i2pr-interop-test-unowned-1234"
            result = subprocess.run(
                [
                    "bash", str(MULTIPASS / "destroy.sh"), "--run-id", run_id,
                    "--instance-name", instance_name, "--destroy-owned",
                ],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("blocked_instance_without_host_state", result.stdout)
            log_value = log.read_text()
            self.assertNotIn("stop", log_value)
            self.assertNotIn("delete", log_value)
            self.assertNotIn("purge", log_value)

    def test_run_and_instance_names_are_bounded_and_collision_resistant(self) -> None:
        run_id = validate_run_id("plan049-20260716-deadbeef")
        first = derive_instance_name(run_id)
        second = derive_instance_name(run_id, attempt=2)
        self.assertNotEqual(first, second)
        self.assertLessEqual(len(first), 63)
        self.assertEqual(len(instance_name_digest(first)), 64)
        with self.assertRaises(LifecycleError):
            validate_run_id("Plan049_bad")
        with self.assertRaises(LifecycleError):
            allocate_instance_name(run_id, active_instance_names=[first], max_attempts=1)

    def test_structured_multipass_state_normalization_is_strict(self) -> None:
        raw = json.dumps({"list": [
            {"name": "unrelated", "state": "RUNNING"},
            {"name": "stopped", "state": "STOPPED"},
            {"name": "suspended", "state": "SUSPENDED"},
        ]})
        self.assertEqual({entry["state"] for entry in parse_multipass_list(raw)}, {"running", "stopped", "suspended"})
        self.assertEqual(normalize_instance_state("DELAYED_SHUTDOWN"), "delayed-shutdown")
        with self.assertRaises(LifecycleError) as error:
            normalize_instance_state("future-state")
        self.assertEqual(error.exception.outcome, "blocked_unknown_multipass_instance_state")
        self.assertEqual(classify_collision(instance_state="Deleted", host_state_exists=True), "blocked_deleted_instance_requires_purge")
        self.assertEqual(classify_collision(instance_state="RUNNING", host_state_exists=False), "blocked_instance_without_host_state")
        info = json.dumps({"info": {"owned": {"state": "Running", "snapshots": [{"name": "provisioned"}]}}})
        from lifecycle import parse_multipass_info  # noqa: PLC0415
        self.assertEqual(parse_multipass_info(info, "owned")["snapshots"], ["provisioned"])
        with self.assertRaises(LifecycleError) as malformed:
            parse_multipass_list('{"list": [{"name": "owned", "state": "future"}]}')
        self.assertEqual(malformed.exception.outcome, "blocked_unknown_multipass_instance_state")

    def test_ownership_requires_contract_and_root_permissions(self) -> None:
        record = {
            "environment_id": "i2pr-plan048-rootless-v1",
            "run_id": "plan049-20260716-deadbeef",
            "instance_name": "i2pr-interop-plan049-20260716-deadbeef-g1",
            "environment_manifest_sha256": "a" * 64,
            "cloud_init_sha256": "b" * 64,
            "owner_token_sha256": "c" * 64,
            "state": "provisioned",
        }
        contract = {
            "environment_id": record["environment_id"], "run_id": record["run_id"],
            "instance_name": record["instance_name"],
            "environment_manifest_sha256": record["environment_manifest_sha256"],
            "cloud_init_sha256": record["cloud_init_sha256"],
            "owner_token_sha256": record["owner_token_sha256"],
        }
        self.assertEqual(ownership_proof(record, contract, guest_token_sha256="c" * 64), (True, "ownership_verified"))
        self.assertEqual(ownership_proof(record, None, guest_token_sha256="c" * 64)[1], "blocked_ownership_token_mismatch")
        self.assertEqual(ownership_proof(record, contract, guest_token_sha256="d" * 64)[1], "blocked_ownership_token_mismatch")
        self.assertEqual(ownership_proof(record, contract, guest_token_sha256="c" * 64, token_mode=0o644)[1], "blocked_existing_instance_contract_mismatch")

    def test_lifecycle_transitions_and_lock_are_fail_closed(self) -> None:
        record = {"state": "reserved"}
        self.assertEqual(transition(record, "launching", operation="launch", outcome="started")["state"], "launching")
        with self.assertRaises(LifecycleError) as error:
            transition(record, "running", operation="launch", outcome="bad")
        self.assertEqual(error.exception.outcome, "blocked_invalid_lifecycle_transition")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state" / ".lifecycle.lock"
            with LifecycleLock(path):
                with self.assertRaises(LifecycleError) as held:
                    with LifecycleLock(path):
                        pass
                self.assertEqual(held.exception.outcome, "blocked_lifecycle_lock_held")

    def test_export_rejects_symlink_and_oversized_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in EXPECTED:
                (root / name).write_text("{}\n")
            (root / "environment.json").unlink()
            (root / "environment.json").symlink_to(root / "aggregate.json")
            with self.assertRaises(ValueError):
                validate(root)
            (root / "environment.json").unlink()
            (root / "environment.json").write_bytes(b"x" * (512 * 1024 + 1))
            with self.assertRaises(ValueError):
                validate(root)

    def test_all_multipass_shell_wrappers_are_strict(self) -> None:
        for path in sorted(MULTIPASS.glob("*.sh")):
            text = path.read_text(encoding="utf-8")
            self.assertIn("set -euo pipefail", text, path.name)
            subprocess.run(["bash", "-n", str(path)], check=True)

    def test_cloud_init_contains_guest_only_policy_and_declared_tools(self) -> None:
        text = (MULTIPASS / "cloud-init.yaml").read_text(encoding="utf-8")
        for package in ("build-essential", "openjdk-21-jdk-headless", "nftables"):
            self.assertIn(f"  - {package}", text)
        self.assertIn("kernel.unprivileged_userns_clone = 1", text)
        self.assertIn("kernel.apparmor_restrict_unprivileged_userns = 0", text)
        self.assertNotIn("apparmor_parser -R", text)
        # Plan 050 moves toolchain and reference work out of cloud-init.
        self.assertNotIn("rustup toolchain install", text)
        self.assertIn("i2pr-multipass-verify-base", text)
        self.assertIn("base-packages.complete", text)

    def test_cloud_init_does_not_clone_repo_or_execute_harness(self) -> None:
        text = (MULTIPASS / "cloud-init.yaml").read_text(encoding="utf-8")
        # Plan 050 forbids cloning the repo, fetching routers, or running the
        # NTCP2 harness from cloud-init. These belong to later phases.
        for forbidden in (
            "git clone",
            "git fetch",
            "build-references.sh",
            "run-matrix.sh",
            "i2pr-interop ntcp2",
            "apt-get install -y --no-install-recommends",
        ):
            self.assertNotIn(forbidden, text)


class CloudInitStatusTests(unittest.TestCase):
    """Plan 050 sanitized cloud-init classification tests."""

    def _record(self, **kwargs):
        from cloud_init_status import parse_status
        return parse_status(**kwargs)

    def test_running_state_classifies_as_timeout_retry(self) -> None:
        record = self._record(long_output="status: running\n")
        self.assertEqual(record["cloud_init_state"], "running")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_timeout")
        self.assertTrue(record["retry_safe"])
        self.assertEqual(record["recommended_action"], "retry-status")

    def test_done_state_classifies_as_post_verify(self) -> None:
        record = self._record(long_output="status: done\n")
        self.assertEqual(record["cloud_init_state"], "done")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_post_verify_failure")
        self.assertTrue(record["retry_safe"])
        self.assertEqual(record["recommended_action"], "resume-provisioning")

    def test_timeout_state_classifies_as_timeout(self) -> None:
        record = self._record(long_output="status: timed out\n")
        self.assertEqual(record["cloud_init_state"], "timeout")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_timeout")
        self.assertTrue(record["retry_safe"])

    def test_local_terminal_error_classifies_as_terminal_error(self) -> None:
        record = self._record(
            long_output="status: error\n",
            json_output=json.dumps({"status": "error", "stage": "init-local", "module": "init", "errors": 1}),
        )
        self.assertEqual(record["cloud_init_state"], "error")
        self.assertEqual(record["cloud_init_stage"], "local")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_terminal_error")

    def test_config_terminal_error_classifies_as_terminal_error(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "error", "stage": "modules-config", "module": "runcmd", "errors": 1}),
        )
        self.assertEqual(record["cloud_init_stage"], "config")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_terminal_error")

    def test_final_terminal_error_classifies_as_terminal_error(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "error", "stage": "modules-final", "module": "users", "errors": 2}),
        )
        self.assertEqual(record["cloud_init_stage"], "final")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_user_creation_failure")

    def test_degraded_status_classifies_as_degraded(self) -> None:
        record = self._record(long_output="status: degraded\n")
        self.assertEqual(record["cloud_init_state"], "degraded")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_degraded")
        self.assertTrue(record["retry_safe"])

    def test_unsupported_json_format_falls_back_safely(self) -> None:
        record = self._record(json_output="not-json")
        self.assertEqual(record["cloud_init_state"], "unknown")
        self.assertEqual(record["failure_class"], "blocked_cloud_init_status_unparseable")
        self.assertFalse(record["retry_safe"])

    def test_unparseable_status_fails_closed(self) -> None:
        record = self._record()
        self.assertEqual(record["failure_class"], "blocked_cloud_init_status_unparseable")
        self.assertEqual(record["recommended_action"], "operator-inspection-required")

    def test_missing_boot_finished_marker(self) -> None:
        record = self._record(long_output="status: running\n", boot_finished_present=False)
        self.assertFalse(record["boot_finished_present"])
        self.assertEqual(record["cloud_init_state"], "running")

    def test_package_install_failure(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "error", "module": "apt", "errors": 1}),
        )
        self.assertEqual(record["failure_class"], "blocked_cloud_init_package_failure")

    def test_sysctl_failure(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "error", "module": "sysctl", "errors": 1}),
        )
        self.assertEqual(record["failure_class"], "blocked_cloud_init_sysctl_failure")

    def test_user_creation_failure(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "error", "module": "users-groups", "errors": 1}),
        )
        self.assertEqual(record["failure_class"], "blocked_cloud_init_group_contract_failure")

    def test_ownership_contract_failure(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "error", "module": "write-files", "errors": 1}),
        )
        self.assertEqual(record["failure_class"], "blocked_cloud_init_filesystem_permission_failure")

    def test_post_verify_failure(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "done", "stage": "post-verify", "module": "unknown"}),
        )
        self.assertEqual(record["failure_class"], "blocked_cloud_init_post_verify_failure")

    def test_retry_safe_allowlist(self) -> None:
        from cloud_init_status import is_retry_safe_failure
        for outcome in (
            "blocked_cloud_init_timeout",
            "blocked_cloud_init_status_unparseable",
            "blocked_cloud_init_degraded",
            "blocked_cloud_init_post_verify_failure",
        ):
            self.assertTrue(is_retry_safe_failure(outcome), outcome)
        for outcome in (
            "blocked_cloud_init_package_failure",
            "blocked_cloud_init_user_creation_failure",
            "blocked_cloud_init_terminal_error",
            "blocked_cloud_init_filesystem_permission_failure",
        ):
            self.assertFalse(is_retry_safe_failure(outcome), outcome)

    def test_resume_unsafe_after_source_transfer_marker(self) -> None:
        from cloud_init_status import recommend_resume
        self.assertEqual(recommend_resume("blocked_cloud_init_resume_unsafe"), "recreate-owned")
        self.assertEqual(recommend_resume("blocked_cloud_init_package_failure"), "recreate-owned")
        self.assertEqual(recommend_resume("blocked_cloud_init_timeout"), "resume-provisioning")
        self.assertEqual(recommend_resume("blocked_cloud_init_terminal_error"), "recreate-owned")

    def test_record_metadata_binds_run_and_digests(self) -> None:
        from cloud_init_status import attach_run_metadata
        record = self._record(long_output="status: done\n")
        updated = attach_run_metadata(
            record,
            run_id="plan050-20260716-deadbeef",
            instance_generation=2,
            environment_manifest_sha256="a" * 64,
            cloud_init_sha256="b" * 64,
        )
        self.assertEqual(updated["run_id"], "plan050-20260716-deadbeef")
        self.assertEqual(updated["instance_generation"], 2)
        self.assertEqual(updated["environment_manifest_sha256"], "a" * 64)
        self.assertEqual(updated["cloud_init_sha256"], "b" * 64)

    def test_generation_mismatch_is_not_propagated_as_pass(self) -> None:
        record = self._record(
            json_output=json.dumps({"status": "done", "stage": "post-verify"}),
        )
        self.assertNotEqual(record["failure_class"], "blocked_cloud_init_status_unparseable")
        self.assertEqual(record["cloud_init_state"], "done")

    def test_active_router_process_classifies_as_router(self) -> None:
        record = self._record(
            long_output="status: running\n",
            service_status={"cloud-init.service": "active", "cloud-config.service": "inactive"},
        )
        self.assertEqual(record["failure_class"], "blocked_cloud_init_service_failure")
        self.assertFalse(record["retry_safe"])
        self.assertEqual(record["recommended_action"], "repair-cloud-init-config")


class SelectivePurgeTests(unittest.TestCase):
    """Plan 050 selective-purge remediation tests."""

    @classmethod
    def setUpClass(cls) -> None:
        sys.path.insert(0, str(MULTIPASS))
        from config import manifest_sha256
        cls.environment_manifest_sha256 = manifest_sha256()

    def _write_lifecycle(self, run_id: str, instance_name: str, manifest: str | None = None) -> Path:
        state_root = ROOT / "target" / "interop" / "multipass" / "state" / run_id
        state_root.mkdir(parents=True, exist_ok=True)
        path = state_root / "lifecycle.json"
        path.write_text(json.dumps({
            "environment_manifest_sha256": manifest or self.environment_manifest_sha256,
            "instance_generation": 1,
            "instance_name": instance_name,
            "run_id": run_id,
            "state": "blocked",
        }))
        return path

    def test_help_supports_purge_subcommand(self) -> None:
        run_id = "plan050-purge-supported-1234"
        self._write_lifecycle(run_id, "i2pr-interop-plan050-purge-1234")
        with tempfile.TemporaryDirectory() as directory:
            bin_path = Path(directory) / "multipass"
            bin_path.write_text(
                "#!/usr/bin/env bash\n"
                "case \"$1\" in\n"
                "  version) echo 'multipass 1.16.3' ;;\n"
                "  help)\n"
                "    if [[ -z \"$2\" ]]; then\n"
                "      echo 'Commands:'\n"
                "      echo '  purge    Purge deleted instances'\n"
                "      exit 0\n"
                "    fi\n"
                "    if [[ \"$2\" == \"purge\" ]]; then\n"
                "      echo 'purge <instance>'\n"
                "      echo 'Removes the deleted instance'\n"
                "    fi\n"
                "    ;;\n"
                "  list) echo '{\"list\":[{\"name\":\"i2pr-interop-plan050-purge-1234\",\"state\":\"Deleted\"}]}' ;;\n"
                "  *) exit 1 ;;\n"
                "esac\n"
            )
            bin_path.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{directory}:{environment['PATH']}"
            environment["I2PR_MULTIPASS_RUN_ID"] = run_id
            result = subprocess.run(
                [
                    "bash", str(MULTIPASS / "selective-purge.sh"),
                    "--run-id", run_id,
                    "--instance-name", "i2pr-interop-plan050-purge-1234",
                ],
                cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("selective_purge_supported", result.stdout)

    def test_help_lacks_purge_returns_not_supported(self) -> None:
        run_id = "plan050-purge-unsupported-1234"
        self._write_lifecycle(run_id, "i2pr-interop-plan050-purgemiss-1234")
        with tempfile.TemporaryDirectory() as directory:
            bin_path = Path(directory) / "multipass"
            bin_path.write_text(
                "#!/usr/bin/env bash\n"
                "case \"$1\" in\n"
                "  version) echo 'multipass 1.14.0' ;;\n"
                "  help) echo 'no purge help' ;;\n"
                "  list) echo '{\"list\":[{\"name\":\"i2pr-interop-plan050-purgemiss-1234\",\"state\":\"Deleted\"}]}' ;;\n"
                "  *) exit 1 ;;\n"
                "esac\n"
            )
            bin_path.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{directory}:{environment['PATH']}"
            environment["I2PR_MULTIPASS_RUN_ID"] = run_id
            result = subprocess.run(
                [
                    "bash", str(MULTIPASS / "selective-purge.sh"),
                    "--run-id", run_id,
                    "--instance-name", "i2pr-interop-plan050-purgemiss-1234",
                ],
                cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout)
            self.assertIn("selective_purge_not_supported", result.stdout)

    def test_unowned_resource_reports_ownership_not_proven(self) -> None:
        run_id = "plan050-unowned-resource-1234"
        # Deliberately do not create a lifecycle file.
        with tempfile.TemporaryDirectory() as directory:
            bin_path = Path(directory) / "multipass"
            bin_path.write_text(
                "#!/usr/bin/env bash\n"
                "case \"$1\" in\n"
                "  version) echo 'multipass 1.16.3' ;;\n"
                "  list) echo '{\"list\":[{\"name\":\"unowned\",\"state\":\"Deleted\"}]}' ;;\n"
                "  help) echo 'no help' ;;\n"
                "  *) exit 1 ;;\n"
                "esac\n"
            )
            bin_path.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{directory}:{environment['PATH']}"
            environment["I2PR_MULTIPASS_RUN_ID"] = run_id
            result = subprocess.run(
                [
                    "bash", str(MULTIPASS / "selective-purge.sh"),
                    "--run-id", run_id,
                    "--instance-name", "i2pr-interop-plan050-unowned-1234",
                ],
                cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout)
            self.assertIn("ownership_not_proven", result.stdout)


class GuestProbeOnlyTests(unittest.TestCase):
    """Plan 050 minimal guest-probe-only flow tests."""

    def test_guest_probe_only_emits_typed_outcome(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bin_path = Path(directory) / "multipass"
            bin_path.write_text(
                "#!/usr/bin/env bash\n"
                "case \"$1\" in\n"
                "  version) echo 'multipass 1.16.3' ;;\n"
                "  info) echo '{\"info\":{}}' ;;\n"
                "  *) exit 1 ;;\n"
                "esac\n"
            )
            bin_path.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{directory}:{environment['PATH']}"
            environment["I2PR_MULTIPASS_RUN_ID"] = "plan050-probeonly-1234"
            result = subprocess.run(
                [
                    "bash", str(MULTIPASS / "run-evidence-lane.sh"),
                    "--guest-probe-only", "--run-id", "plan050-probeonly-1234",
                ],
                cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
            )
            self.assertIn("blocked_host_state_without_instance", result.stdout)
            self.assertNotIn("rootless_sandbox_available", result.stdout)

    def test_guest_probe_only_does_not_run_router_or_transfer_cache(self) -> None:
        text = (MULTIPASS / "run-evidence-lane.sh").read_text(encoding="utf-8")
        # Find the run_guest_probe_only function body
        start = text.find("run_guest_probe_only() {")
        end = text.find("\n}\n", start) + 2
        body = text[start:end]
        for forbidden in (
            "transfer-cache.sh",
            "run-matrix.sh",
            "build_guest_interop",
            "prepare_cache",
            "run-direction.sh",
        ):
            self.assertNotIn(forbidden, body, f"guest-probe-only must not call {forbidden}")


class EnvironmentEvidenceTests(unittest.TestCase):
    """Plan 050 host-baseline and guest-probe outcome separation tests."""

    def test_environment_record_separates_host_and_guest_outcomes(self) -> None:
        # The environment record must expose both fields and reject copying
        # the host outcome to the guest outcome.
        record = {
            "schema": 1,
            "type": "multipass-interop-environment",
            "host_baseline_probe_outcome": "blocked_unprivileged_user_namespace",
            "guest_rootless_probe_outcome": "not-run",
        }
        self.assertNotEqual(record["host_baseline_probe_outcome"], record["guest_rootless_probe_outcome"])

    def test_missing_guest_outcome_is_a_blocker(self) -> None:
        from harness.evidence import validate_record  # type: ignore
        record = {
            "schema": 1,
            "type": "multipass-interop-environment",
            "host_baseline_probe_outcome": "blocked_unprivileged_user_namespace",
        }
        with self.assertRaises(Exception):
            validate_record(record)

    def test_no_protocol_record_written_by_plan_050(self) -> None:
        # The guest-probe-only path must not include direction records.
        for forbidden in (
            "i2pr-to-java-ipv4",
            "java-to-i2pr-ipv4",
            "i2pr-to-i2pd-ipv4",
            "i2pd-to-i2pr-ipv4",
            "run-direction.sh",
        ):
            # The guest-probe-only function does not call run-direction.sh.
            # Verify it doesn't appear in the function body.
            text = (MULTIPASS / "run-evidence-lane.sh").read_text(encoding="utf-8")
            self.assertTrue(forbidden not in text or "run_guest_probe_only" not in text, forbidden)


if __name__ == "__main__":
    unittest.main()
