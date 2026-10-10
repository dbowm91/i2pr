#!/usr/bin/env python3
"""Check that the public-router readiness decision stays fail-closed."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_FILES = {
    "readiness": "crates/i2pr-core/src/router_readiness.rs",
    "config": "crates/i2pr-daemon/src/config.rs",
    "router_info": "crates/i2pr-netdb/src/local.rs",
    "ssu2_runtime": "crates/i2pr-runtime/src/ssu2_runtime.rs",
    "daemon": "crates/i2pr-daemon/src/lib.rs",
    "floodfill": "crates/i2pr-daemon/src/floodfill.rs",
}


def violations(sources: dict[str, str]) -> list[str]:
    checks = {
        "normal-router profile must be narrowly scoped to transport": "RouterProfile::NormalRouter,\n            AdvertisementClaim::TransportAddress",
        "optional roles must require the optional-role profile": "| (RouterProfile::OptionalNetworkRole, _)",
        "readiness must gate claims": "!readiness.satisfies(required_readiness)",
        "owner health must gate claims": "!evidence.owner_healthy",
        "reachability must gate claims": "!evidence.address_reachable",
        "independent protocol qualification must gate claims": "!evidence.protocol_qualified",
        "operator authorization must gate claims": "!evidence.operator_authorized",
        "owner generation must gate claims": "!evidence.generation_current",
        "degraded state must never satisfy network readiness": "Self::Degraded => false",
        "claim decisions must not be authorization tokens": "not an authorization\n//! token",
    }
    errors = [message for message, needle in checks.items() if needle not in sources["readiness"]]
    source_controls = {
        "ssu2 advertisement must remain rejected": ("config", "if raw.advertise {"),
        "introducer publication must remain rejected": ("config", "if raw.introducer_service {"),
        "SSU2 publication config negative test must remain": ("config", "fn ssu2_advertise_and_introducer_are_rejected()"),
        "SSU2 bind parser must reject non-loopback literals": ("config", "if !address.is_loopback()"),
        "SSU2 non-loopback config negative test must remain": ("config", "fn ssu2_bind_fields_reject_non_loopback_and_wrong_family()"),
        "normal-daemon NTCP2 activation must remain rejected": ("config", "normal-daemon NTCP2 activation is unavailable"),
        "NTCP2 activation config negative test must remain": ("config", "fn explicit_ntcp2_enabled_true_is_rejected()"),
        "ordinary RouterInfo must have no transport addresses": ("router_info", "let addresses: Vec<RouterAddress> = Vec::new();"),
        "SSU2-installed RouterInfo must be signature verified": ("ssu2_runtime", "i2pr_crypto::verify_router_info(&info)"),
        "SSU2-installed RouterInfo must be freshness checked": ("ssu2_runtime", "local_router_info_fresh(info.published().as_millis(), wall_now_ms)"),
        "SSU2-installed RouterInfo must bind the local identity": ("ssu2_runtime", "hash != self.shared.local_peer.hash()"),
        "SSU2 publication must bind to an actually bound endpoint": ("ssu2_runtime", "bound.ip() == endpoint.ip() && bound.port() == endpoint.port()"),
        "unverified SSU2 RouterInfo negative test must remain": ("ssu2_runtime", "async fn install_local_router_info_enforces_policy_and_bumps_generation()"),
        "normal transit must start disabled": ("daemon", "publish_transit_participation(crate::transit_volume::TransitParticipation::Disabled)"),
        "floodfill opt-in must default off": ("config", "const fn default_floodfill_enabled() -> bool {\n    false\n}"),
        "normal floodfill must require qualified public address evidence": ("floodfill", "fn is_normal_qualified_address"),
        "normal floodfill must require confirmed reachability": ("floodfill", "material.reachability == i2pr_transport::ReachabilityState::Reachable"),
        "normal floodfill activation must recheck eligibility": ("floodfill", "if !snapshot.eligible()"),
        "normal floodfill negative reachability regression must remain": ("floodfill", "fn normal_unconfirmed_reachability_stays_disabled()"),
    }
    errors.extend(message for message, (name, needle) in source_controls.items() if needle not in sources[name])
    return errors


def check(sources: dict[str, str]) -> None:
    errors = violations(sources)
    if errors:
        raise ValueError("; ".join(errors))


def self_test(sources: dict[str, str]) -> None:
    check(sources)
    mutations = [
        "!readiness.satisfies(required_readiness)",
        "!evidence.owner_healthy",
        "!evidence.address_reachable",
        "!evidence.protocol_qualified",
        "!evidence.operator_authorized",
        "!evidence.generation_current",
        "Self::Degraded => false",
        "| (RouterProfile::OptionalNetworkRole, _)",
    ]
    mutations.extend(
        (name, needle)
        for message, (name, needle) in {
            "ssu2 advertisement must remain rejected": ("config", "if raw.advertise {"),
            "introducer publication must remain rejected": ("config", "if raw.introducer_service {"),
            "SSU2 publication config negative test must remain": ("config", "fn ssu2_advertise_and_introducer_are_rejected()"),
            "SSU2 bind parser must reject non-loopback literals": ("config", "if !address.is_loopback()"),
            "SSU2 non-loopback config negative test must remain": ("config", "fn ssu2_bind_fields_reject_non_loopback_and_wrong_family()"),
            "normal-daemon NTCP2 activation must remain rejected": ("config", "normal-daemon NTCP2 activation is unavailable"),
            "NTCP2 activation config negative test must remain": ("config", "fn explicit_ntcp2_enabled_true_is_rejected()"),
            "ordinary RouterInfo must have no transport addresses": ("router_info", "let addresses: Vec<RouterAddress> = Vec::new();"),
            "SSU2-installed RouterInfo must be signature verified": ("ssu2_runtime", "i2pr_crypto::verify_router_info(&info)"),
            "SSU2-installed RouterInfo must be freshness checked": ("ssu2_runtime", "local_router_info_fresh(info.published().as_millis(), wall_now_ms)"),
            "SSU2-installed RouterInfo must bind the local identity": ("ssu2_runtime", "hash != self.shared.local_peer.hash()"),
            "SSU2 publication must bind to an actually bound endpoint": ("ssu2_runtime", "bound.ip() == endpoint.ip() && bound.port() == endpoint.port()"),
            "unverified SSU2 RouterInfo negative test must remain": ("ssu2_runtime", "async fn install_local_router_info_enforces_policy_and_bumps_generation()"),
            "normal transit must start disabled": ("daemon", "publish_transit_participation(crate::transit_volume::TransitParticipation::Disabled)"),
            "floodfill opt-in must default off": ("config", "const fn default_floodfill_enabled() -> bool {\n    false\n}"),
            "normal floodfill must require qualified public address evidence": ("floodfill", "fn is_normal_qualified_address"),
            "normal floodfill must require confirmed reachability": ("floodfill", "material.reachability == i2pr_transport::ReachabilityState::Reachable"),
            "normal floodfill activation must recheck eligibility": ("floodfill", "if !snapshot.eligible()"),
            "normal floodfill negative reachability regression must remain": ("floodfill", "fn normal_unconfirmed_reachability_stays_disabled()"),
        }.items()
    )
    for mutation in mutations:
        if isinstance(mutation, tuple):
            name, needle = mutation
        else:
            name, needle = "readiness", mutation
        mutated = dict(sources)
        mutated[name] = mutated[name].replace(needle, "MUTATED_GATE")
        if mutated[name] == sources[name] or not violations(mutated):
            raise ValueError(f"self-test failed to detect mutation: {needle}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        sources = {
            name: (ROOT / relative).read_text(encoding="utf-8")
            for name, relative in SOURCE_FILES.items()
        }
        if args.self_test:
            self_test(sources)
            print("router-readiness contract self-test passed")
        else:
            check(sources)
            print("router-readiness contract checks passed")
    except (OSError, ValueError) as error:
        print(f"router-readiness contract check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
