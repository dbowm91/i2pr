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

    def test_fake_multipass_rejects_canonical_name_collision_before_launch(self) -> None:
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
            self.assertIn("blocked_instance_name_collision", result.stdout)
            self.assertNotIn("launch", log.read_text())

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
        for package in ("build-essential", "openjdk-21-jdk-headless", "nftables", "rustup"):
            if package == "rustup":
                self.assertIn("rustup toolchain install 1.95.0", text)
            else:
                self.assertIn(f"  - {package}", text)
        self.assertIn("kernel.unprivileged_userns_clone = 1", text)
        self.assertIn("kernel.apparmor_restrict_unprivileged_userns = 0", text)
        self.assertNotIn("apparmor_parser -R", text)


if __name__ == "__main__":
    unittest.main()
