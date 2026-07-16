"""Rootless inner scenario runner (Plan 046).

The outer ``scripts/interop/rootless-enter.sh`` entrypoint creates the
process-scoped sandbox and execs this script. This module selects the
topology and forwards execution to the existing reference routers.

The first closed-form path is the bounded rootless capability probe. The
``--scenario`` path is wired through the same ``select_topology`` path the
privileged backend uses, but the in-sandbox placement is empty because the
routers already execute in the sealed network namespace.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from harness.interop_topology import (  # type: ignore
        ROOTLESS_TOPOLOGY_KIND,
        select_topology,
    )
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
    from .rootless_supervisor import (
        SandboxError,
        SandboxPolicy,
        build_attestation,
        emit_probe_status,
        run as run_supervisor,
        write_attestation,
    )


def _scenario_main(args: argparse.Namespace) -> int:
    repo_root = Path(__file__).resolve().parents[3]
    policy = SandboxPolicy(
        run_id=args.scenario,
        i2pr_address="192.0.2.1",
        i2pr_port=45680,
        reference_address="192.0.2.2",
        reference_port=45678 if args.reference == "java_i2p" else 45679,
        reference_kind=args.reference,
        ipv6_enabled=False,
        i2pr_ipv6=None,
        reference_ipv6=None,
        parent_digest_pre=os.environ.get("I2PR_INTEROP_ROOTLESS_PARENT_DIGEST_PRE", ""),
    )
    try:
        topology = select_topology(
            ROOTLESS_TOPOLOGY_KIND,
            repo_root=repo_root,
            run_id=args.scenario,
            ipv6=False,
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
    print(json.dumps({"schema": 1, "type": "rootless-inner-runner-result",
                      "outcome": "rootless_sandbox_available",
                      "scenario_id": args.scenario,
                      "attestation_sha256": attestation.attestation_sha256},
                     separators=(",", ":")))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", required=False)
    parser.add_argument("--reference", choices=("java_i2p", "i2pd"), required=False)
    parser.add_argument("--attestation-output")
    args = parser.parse_args(argv)
    if not args.scenario:
        parser.print_help()
        return 2
    return _scenario_main(args)


if __name__ == "__main__":
    raise SystemExit(main())
