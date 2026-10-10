#!/usr/bin/env python3
"""Plan 440 guard for the recovery line's evidence and topology contracts.

This is a source guard, not interop evidence. It keeps the profile taxonomy,
positive-control rule, loopback/public distinction, and successor dependencies
present in the live planning authority. ``--self-test`` proves each rule rejects
its corresponding in-memory mutation.
"""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROADMAP = ROOT / "plans/subsystems/core-router-recovery-roadmap.md"
REGISTRY = ROOT / "plans/registry.md"
DIAGNOSTIC = ROOT / "plans/diagnostics/2026-10-10-emissary-router-qualification-comparison.md"
PLAN_441 = ROOT / "plans/implementation/ntcp2-transport/441-current-pin-single-session-control-and-interop-recovery.md"
PLAN_443 = ROOT / "plans/implementation/core-router-recovery/443-controlled-router-product-integration-without-public-ssu2.md"


def findings(roadmap: str, registry: str, diagnostic: str,
             plan_441: str, plan_443: str) -> list[str]:
    requirements = {
        "profile taxonomy": all(
            name in diagnostic
            for name in ("Deterministic protocol unit/state-machine", "Real loopback transport",
                         "Controlled multirouter product", "Independently addressed/private LAN",
                         "Normal/public readiness")
        ),
        "loopback is not public reachability": (
            "loopback **does not inherently invalidate**" in registry
            and "not external reachability" in diagnostic
            and "no normal public network claim" in plan_443
        ),
        "stock positive control precedes protocol attribution": (
            "known-working stock-to-stock control **before** attributing" in plan_443
            and "STOP and classify fixture/environment" in plan_441
        ),
        "NTCP2 acceptance is session-specific": (
            "per-connection ephemeral correlation" in plan_441
            and "not only a helper future" in plan_441
        ),
        "no fake links or zero-hop fallback": (
            "no static manually injected links" in plan_443
            and "no hidden zero-hop fallback" in plan_443
        ),
        "public recovery gate still depends on Plan 431": (
            "| 433 | blocked on 431,432 |" in roadmap
            and "431+432 → real independent-router multihop" in registry
        ),
        "Plan 443 controlled work does not close Plan 433": (
            "does **not** close Plan 433" in plan_443
            and "closure of Plan 433's public/external gate" in registry
        ),
    }
    return [name for name, passed in requirements.items() if not passed]


def self_test() -> bool:
    sources = [p.read_text() for p in (ROADMAP, REGISTRY, DIAGNOSTIC, PLAN_441, PLAN_443)]
    if findings(*sources):
        return False
    mutations = [
        (sources[0], sources[1], sources[2].replace("Deterministic protocol unit/state-machine", "fake-success", 1), *sources[3:]),
        (sources[0], sources[1], sources[2].replace("not external reachability", "public reachability", 1), sources[3], sources[4]),
        (sources[0], sources[1], sources[2], sources[3].replace("STOP and classify fixture/environment", "blame i2pr protocol", 1), sources[4]),
        (sources[0], sources[1], sources[2], sources[3].replace("per-connection ephemeral correlation", "process-wide event count", 1), sources[4]),
        (sources[0], sources[1], sources[2], sources[3], sources[4].replace("no hidden zero-hop fallback", "zero-hop fallback allowed", 1)),
        (sources[0].replace("| 433 | blocked on 431,432 |", "| 433 | ready |", 1), sources[1], *sources[2:]),
        (sources[0], sources[1].replace("closure of Plan 433's public/external gate", "closes Plan 433 gate", 1), *sources[2:]),
    ]
    return all(findings(*candidate) for candidate in mutations)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        ok = self_test()
        print("core-router recovery contract self-test passed" if ok
              else "core-router recovery contract self-test failed")
        return 0 if ok else 1
    errors = findings(ROADMAP.read_text(), REGISTRY.read_text(), DIAGNOSTIC.read_text(),
                      PLAN_441.read_text(), PLAN_443.read_text())
    if errors:
        for error in errors:
            print(f"core-router recovery contract check failed: {error}")
        return 1
    print("core-router recovery evidence contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
