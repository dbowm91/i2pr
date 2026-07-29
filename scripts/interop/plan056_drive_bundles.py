#!/usr/bin/env python3
"""Plan 056 two-bundle evidence driver.

Produces two independently-staged Plan 052 evidence bundles with the
canonical primary directions and runs the Plan 056 certificate verifier
against them. The driver is intentionally local-only and uses the
``plan052_pipeline.write_direction_artifacts`` synthetic-fallback so it
runs without external references; the resulting bundles therefore
classify as ``diagnostic-complete-not-certificate`` on this host.

A passing certificate requires external execution on a host whose
rootless sealed-namespace probe returns ``rootless_sandbox_available``.
This driver is the local-evidence path used to exercise the verifier
end-to-end and to leave a reproducible Plan 056 closure artefact on
disk when external execution cannot be performed.

Usage:

```bash
python3 scripts/interop/plan056_drive_bundles.py \
    --repo-root . \
    --run-a-id plan056-a-20260729000000-testbundle \
    --run-b-id plan056-b-20260729000000-testbundle \
    --evidence-root target/interop/evidence/plan056
```

The script writes two sanitized bundles under
``<evidence-root>/run-a/<run-a-id>`` and
``<evidence-root>/run-b/<run-b-id>`` and the certificate at
``<evidence-root>/certificate/milestone3-certificate.json``. Exit
status mirrors ``verify_milestone3_certificate.py`` (0 verified,
3 not verified, 2 verification errored).
"""

from __future__ import annotations

import argparse
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT_HINT = HERE.parents[1]
HARNESS = REPO_ROOT_HINT / "tests" / "integration" / "ntcp2" / "harness"
sys.path.insert(0, str(HARNESS))

from evidence_bundle import export_bundle_atomic  # noqa: E402
from plan052_pipeline import (  # noqa: E402
    create_context,
    finalize_diagnostic_bundle,
    write_direction_artifacts,
)
from verify_milestone3_certificate import verify_certificate  # noqa: E402

PRIMARY_DIRECTION_INITIATORS = {
    "i2pr-to-java-ipv4": ("java_i2p", "i2pr"),
    "java-to-i2pr-ipv4": ("java_i2p", "java_i2p"),
    "i2pr-to-i2pd-ipv4": ("i2pd", "i2pr"),
    "i2pd-to-i2pr-ipv4": ("i2pd", "i2pd"),
}


def _build_diagnostic_bundle(
    *,
    repo_root: Path,
    run_id: str,
    staging_root: Path,
    run_identity_path: Path,
    result: str,
    reason_code: str,
) -> None:
    if staging_root.exists():
        shutil.rmtree(staging_root)
    if run_identity_path.exists():
        run_identity_path.unlink()
    context = create_context(
        repo_root=repo_root.resolve(),
        run_id=run_id,
        run_identity_path=run_identity_path.resolve(),
        staging_root=staging_root.resolve(),
    )
    for direction, (reference, initiator) in PRIMARY_DIRECTION_INITIATORS.items():
        write_direction_artifacts(
            context,
            direction,
            reference=reference,
            initiator=initiator,
            result=result,
            reason_code=reason_code,
            cleanup_result="clean",
        )
    finalize_diagnostic_bundle(context)


def _main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path("."))
    parser.add_argument("--run-a-id", required=True)
    parser.add_argument("--run-b-id", required=True)
    parser.add_argument(
        "--evidence-root",
        type=Path,
        default=Path("target/interop/evidence/plan056"),
    )
    parser.add_argument("--result", default="blocked")
    parser.add_argument("--reason-code", default="blocked_host_contract")
    args = parser.parse_args(argv)
    evidence_root = args.evidence_root.resolve()
    run_a_staging = evidence_root / "run-a" / args.run_a_id
    run_b_staging = evidence_root / "run-b" / args.run_b_id
    run_a_identity = run_a_staging.parent / f"{args.run_a_id}.identity.json"
    run_b_identity = run_b_staging.parent / f"{args.run_b_id}.identity.json"

    _build_diagnostic_bundle(
        repo_root=args.repo_root,
        run_id=args.run_a_id,
        staging_root=run_a_staging,
        run_identity_path=run_a_identity,
        result=args.result,
        reason_code=args.reason_code,
    )
    export_bundle_atomic(run_a_staging, evidence_root / "run-a" / args.run_a_id)
    _build_diagnostic_bundle(
        repo_root=args.repo_root,
        run_id=args.run_b_id,
        staging_root=run_b_staging,
        run_identity_path=run_b_identity,
        result=args.result,
        reason_code=args.reason_code,
    )
    export_bundle_atomic(run_b_staging, evidence_root / "run-b" / args.run_b_id)

    certificate = verify_certificate(
        evidence_root / "run-a" / args.run_a_id,
        evidence_root / "run-b" / args.run_b_id,
    )
    certificate_path = evidence_root / "certificate" / "milestone3-certificate.json"
    certificate_path.parent.mkdir(parents=True, exist_ok=True)
    import json as _json

    certificate_path.write_text(
        _json.dumps(certificate, sort_keys=False, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    certificate_path.chmod(0o600)
    print(
        f"wrote {run_a_staging}, {run_b_staging}, {certificate_path}; "
        f"verified={certificate['verified']} failures={len(certificate['failures'])}"
    )
    return 0 if certificate["verified"] else 3


if __name__ == "__main__":
    raise SystemExit(_main())
