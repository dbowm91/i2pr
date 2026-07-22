"""Rootless inner scenario runner (Plan 046).

The outer ``scripts/interop/rootless-enter.sh`` entrypoint creates the
process-scoped sandbox and execs this script. This module:

1. Builds a sanitized ``IsolationAttestation`` whose SHA-256 binds every
   mixed-router evidence record produced under the rootless topology.
2. Dispatches ``mixed_runner.py --topology-kind rootless-sealed-single-netns``
   for the bounded direction, propagating the attestation SHA-256 and the
   parent-network state-unchanged flag through environment variables.

This module runs **inside** the sealed user/network namespace that the
outer entrypoint created via ``unshare --user --net --mount --pid``. It
must never invoke ``sudo``, ``ip netns``, ``nft``, or any host capability
and must never fall back to the privileged backend.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from harness.interop_topology import (  # type: ignore
        ROOTLESS_TOPOLOGY_KIND,
        select_topology,
    )
    import harness.rootless_topology  # type: ignore  # noqa: F401  # module-level register_topology side-effect
    from harness.rootless_supervisor import (  # type: ignore
        SandboxError,
        SandboxPolicy,
        build_attestation,
        emit_probe_status,
        run as run_supervisor,
        write_attestation,
    )
else:
    from .interop_topology import (
        ROOTLESS_TOPOLOGY_KIND,
        select_topology,
    )
    from . import rootless_topology  # noqa: F401  # module-level register_topology side-effect
    from .rootless_supervisor import (
        SandboxError,
        SandboxPolicy,
        build_attestation,
        emit_probe_status,
        run as run_supervisor,
        write_attestation,
    )


def _build_policy(scenario_id: str, reference: str, *, ipv6: bool) -> SandboxPolicy:
    return SandboxPolicy(
        run_id=scenario_id,
        i2pr_address="192.0.2.1",
        i2pr_port=45680,
        reference_address="192.0.2.2",
        reference_port=45678 if reference == "java_i2p" else 45679,
        reference_kind=reference,
        ipv6_enabled=ipv6,
        i2pr_ipv6="2001:db8:36::1" if ipv6 else None,
        reference_ipv6="2001:db8:36::2" if ipv6 else None,
        parent_digest_pre=os.environ.get("I2PR_INTEROP_ROOTLESS_PARENT_DIGEST_PRE", ""),
    )


def _scenario_main(args: argparse.Namespace) -> int:
    repo_root = Path(__file__).resolve().parents[4]
    policy = _build_policy(args.scenario, args.reference, ipv6=args.ipv6)
    try:
        topology = select_topology(
            ROOTLESS_TOPOLOGY_KIND,
            repo_root=repo_root,
            run_id=args.scenario,
            ipv6=args.ipv6,
            reference_port=policy.reference_port,
            i2pr_port=policy.i2pr_port,
            reference_kind=args.reference,
        )
        topology.create()
        attestation = run_supervisor(
            policy=policy,
            i2pr_commit=os.environ.get("I2PR_INTEROP_COMMIT", ""),
            parent_digest_post=policy.parent_digest_pre,
        )
        if args.attestation_output:
            write_attestation(Path(args.attestation_output), attestation)
    except SandboxError as exc:
        emit_probe_status(exc.code)
        return 1
    env = os.environ.copy()
    env["I2PR_INTEROP_ROOTLESS_INNER"] = "1"
    env["I2PR_INTEROP_ROOTLESS_ATTESTATION_SHA256"] = attestation.attestation_sha256
    env["I2PR_INTEROP_ROOTLESS_PARENT_STATE_UNCHANGED"] = "1" if attestation.parent_network_state_unchanged else "0"
    env.setdefault("I2PR_INTEROP_DIAGNOSTICS", "off")
    mixed_runner = repo_root / "tests/integration/ntcp2/harness/mixed_runner.py"
    evidence_dir = repo_root / "target/interop/evidence"
    evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    env["INTEROP_EVIDENCE_DIR"] = str(evidence_dir)
    command = [
        sys.executable,
        str(mixed_runner),
        "--scenario",
        args.scenario,
        "--reference",
        args.reference,
        "--topology-kind",
        ROOTLESS_TOPOLOGY_KIND,
    ]
    if args.build_cache:
        command.extend(["--build-cache", args.build_cache])
    if args.run_root:
        command.extend(["--run-root", args.run_root])
    completed = subprocess.run(command, env=env, capture_output=True, text=True, check=False)
    sys.stdout.write(completed.stdout)
    sys.stderr.write(completed.stderr)
    if completed.returncode not in (0, 2):
        return completed.returncode
    summary = {
        "schema": 1,
        "type": "rootless-inner-runner-result",
        "outcome": "rootless_sandbox_available",
        "scenario_id": args.scenario,
        "reference": args.reference,
        "attestation_sha256": attestation.attestation_sha256,
        "parent_network_state_unchanged": attestation.parent_network_state_unchanged,
        "mixed_runner_returncode": completed.returncode,
    }
    print(json.dumps(summary, separators=(",", ":")))
    return 0 if completed.returncode == 0 else 2


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", required=False)
    parser.add_argument("--reference", choices=("java_i2p", "i2pd"), required=False)
    parser.add_argument("--ipv6", action="store_true")
    parser.add_argument("--attestation-output")
    parser.add_argument("--build-cache")
    parser.add_argument("--run-root")
    args = parser.parse_args(argv)
    if not args.scenario or not args.reference:
        parser.print_help()
        return 2
    return _scenario_main(args)


if __name__ == "__main__":
    raise SystemExit(main())