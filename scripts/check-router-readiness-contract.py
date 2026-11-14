#!/usr/bin/env python3
"""Check that the public-router readiness decision stays fail-closed."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_FILES = {
    "readiness": "crates/i2pr-core/src/router_readiness.rs",
    "config": "crates/i2pr-daemon/src/config.rs",
    "router_info": "crates/i2pr-netdb/src/local.rs",
    "ssu2_runtime": "crates/i2pr-runtime/src/ssu2_runtime.rs",
    "ntcp2_runtime": "crates/i2pr-runtime/src/ntcp2_runtime.rs",
    "ntcp2_link": "crates/i2pr-runtime/src/ntcp2_link.rs",
    "ntcp2_activation": "crates/i2pr-daemon/src/ntcp2_activation.rs",
    "daemon": "crates/i2pr-daemon/src/lib.rs",
    "router_i2np": "crates/i2pr-daemon/src/router_i2np.rs",
    "floodfill": "crates/i2pr-daemon/src/floodfill.rs",
}


def source_controls() -> dict[str, tuple[str, str]]:
    """The source-level controls this contract protects.

    Plan 496 removed the duplication between the live checks and the
    self-test mutations: both now read this one mapping, so a rule can no
    longer drift out of the self-test.
    """

    return {
            # Plan 520 made the router's own SSU2 address publication a real
            # opt-in, so the advertisement is no longer refused outright: it
            # is refused unless the SSU2 service itself runs, and it is
            # refused outside the controlled loopback profile. The
            # invariant is preserved and made explicit: the router's own
            # address is published only behind an explicit, default-off
            # opt-in, and only while the router's own reachability evidence
            # admits it.
            "ssu2 advertisement must remain refused unless the SSU2 service runs": ("config", "if raw.advertise {\n        if !raw.enabled {"),
            "ssu2 advertisement must remain refused outside the controlled profile": ("config", "if private_lan {\n            return Err(ConfigError::Semantic {\n                field: \"ssu2.advertise\","),
            # Plan 490 made the introducer role available behind an explicit
            # opt-in, and Plan 520 removed the advertisement refusal that
            # used to bound it. Plan 526 found that the composition's
            # **blanket** refusal of the role made the role owner the
            # composition constructs unreachable in every configuration, so
            # the role could never run and the role's advertisement could
            # never be truthful. The invariant this control protects -- the
            # router never publishes an introducer role it does not serve --
            # now lives where it is enforced: the runtime's install gate
            # refuses a role-bearing record while the role is not live, and
            # the composition constructs the role owner **only** behind the
            # explicit opt-in.
            "introducer publication must remain rejected unless the role is live": ("ssu2_runtime", "parsed.capabilities().supports_relay() && !self.introducer_service_enabled()"),
            "the introducer role must be constructed only behind the explicit opt-in": ("daemon", "let mut introducer = if ssu2_config.introducer_service {"),
            "SSU2 advertisement must remain refused by the socket boundary": ("router_i2np", "if config.advertise\n        && config.qualification_profile == crate::config::Ssu2QualificationProfile::PrivateLan\n    {\n        return Err(Ssu2ServiceError::ControlledProfile(\n            \"the private-LAN SSU2 profile cannot publish an SSU2 address\""),
            "SSU2 advertisement opt-in test must remain": ("router_i2np", "fn socket_config_permits_the_controlled_advertisement_opt_in()"),
            "introducer role must remain rejected unless the SSU2 service runs": ("config", "if raw.introducer_service && !raw.enabled {"),
            "SSU2 publication config negative test must remain": ("config", "fn ssu2_advertise_and_introducer_are_rejected()"),
            "SSU2 non-loopback bind must require the named private profile and RFC1918 IPv4": ("config", "if !private_lan || !is_private_ipv4 {"),
            "SSU2 non-loopback config negative test must remain": ("config", "fn ssu2_bind_fields_reject_non_loopback_and_wrong_family()"),
            # Plan 496 widened the set of *named* qualification profiles from one
            # to two. The invariant is preserved and strengthened: an enabled
            # NTCP2 transport still requires an explicit named profile, and each
            # named profile has its own address rule pinned below.
            "enabled NTCP2 must require an explicit named qualification profile": ("config", "enabled NTCP2 requires the explicit controlled_loopback or private_lan_qualification profile"),
            "controlled NTCP2 bind must require a loopback IP": ("config", "controlled_loopback requires a loopback address"),
            "controlled NTCP2 bind must require a fixed nonprivileged port": ("config", "controlled_loopback requires an explicit non-privileged port"),
            "controlled NTCP2 network ID must fit the handshake field": ("config", "controlled NTCP2 requires a network ID representable by the handshake"),
            "private-LAN NTCP2 bind must require an RFC1918 IPv4 literal": ("config", "private_lan_qualification requires an explicit RFC1918 IPv4 address literal"),
            "private-LAN NTCP2 bind must refuse loopback": ("config", "loopback is not a private-LAN address"),
            "private-LAN NTCP2 bind must refuse the unspecified address": ("config", "the unspecified address is not a private-LAN address"),
            "private-LAN NTCP2 bind must refuse IPv6 literals": ("config", "IPv6 literals are not supported"),
            "private-LAN NTCP2 bind must require an explicit nonzero port": ("config", "private_lan_qualification requires an explicit nonzero port"),
            "private-LAN NTCP2 bind must require a nonprivileged port": ("config", "private_lan_qualification requires a port of at least 1024"),
            "private-LAN NTCP2 profile must require the matching SSU2 profile": ("config", "private_lan_qualification requires the ssu2 surface to use the private_lan_qualification profile"),
            "private-LAN NTCP2 profile must require a matching SSU2 address family": ("config", "private_lan_qualification requires the ssu2 surface to bind an RFC1918 IPv4 address"),
            "private-LAN NTCP2 config negative tests must remain": ("config", "fn ntcp2_private_lan_profile_refuses_every_unauthorized_shape()"),
            "private-LAN NTCP2 refusals must not be vacuous": ("config", "fn ntcp2_private_lan_profile_refusals_are_not_vacuous()"),
            "private-LAN NTCP2 profile must accept an RFC1918 endpoint with a matching SSU2": ("config", "fn ntcp2_private_lan_profile_accepts_rfc1918_endpoint_with_matching_ssu2()"),
            "private-LAN NTCP2 profile must never fall back to the controlled profile": ("config", "fn ntcp2_private_lan_profile_never_falls_back_to_the_controlled_profile()"),
            "NTCP2 defaults must stay disabled and loopback only": ("config", "fn ntcp2_defaults_stay_disabled_and_loopback_only()"),
            "NTCP2 activation must go through the profile-aware gate": ("daemon", "crate::ntcp2_activation::ntcp2_activation_scope("),
            "NTCP2 activation gate must name each refusal": ("ntcp2_activation", "pub enum Ntcp2ActivationRefusal {"),
            "NTCP2 activation gate must refuse a non-loopback controlled bind": ("ntcp2_activation", "Ntcp2ActivationRefusal::NonLoopbackBind"),
            "NTCP2 activation gate must refuse a non-RFC1918 private-LAN bind": ("ntcp2_activation", "Ntcp2ActivationRefusal::NonPrivateLanBind"),
            "NTCP2 activation gate must refuse a privileged port": ("ntcp2_activation", "Ntcp2ActivationRefusal::PrivilegedPort"),
            "NTCP2 activation gate must refuse a disabled transport": ("ntcp2_activation", "Ntcp2ActivationRefusal::Disabled"),
            "NTCP2 activation gate must never widen a scope": ("ntcp2_activation", "fn the_gate_never_widens_a_scope()"),
            "NTCP2 activation gate refusals must not be vacuous": ("ntcp2_activation", "fn the_refusals_are_not_vacuous()"),
            "controlled NTCP2 owner must register only when explicitly enabled": ("daemon", "if config.transport.ntcp2.enabled {\n        register_ntcp2_service("),
            "NTCP2 service failure must degrade independently": ("daemon", ".startup_requirement(i2pr_core::StartupRequirement::Optional)"),
            "NTCP2 service must reuse the existing router identity": ("daemon", "IdentityStore::in_data_dir(&data_dir).load()"),
            "NTCP2 must share the process bandwidth governor": ("daemon", "Ntcp2RuntimeService::new_with_bandwidth("),
            "NTCP2 must use the shared authenticated I2NP dispatcher": ("daemon", "decode_authenticated_router_i2np("),
            "NTCP2 inbound sockets must use the runtime responder handshake": ("daemon", "runtime.authenticate_inbound("),
            "NTCP2 responder must apply the runtime replay and clock policy": ("ntcp2_runtime", "ClockSkewPolicy::default_compatibility()"),
            "NTCP2 frame parsing must use authenticated plaintext": ("ntcp2_link", ".plaintext()\n            .parse()"),
            "NTCP2 receive queue lease must be released before dispatch": ("ntcp2_link", "drop(frame);\n        Ok(Some(messages))"),
            "NTCP2 outbound peers must come from the validated bootstrap snapshot": ("daemon", "owner.validated_router_info_snapshot()"),
            "NTCP2 outbound dialing must admit only addresses inside the caller's scope": ("ntcp2_runtime", "if !scope.admits(endpoint.ip()) {"),
            "NTCP2 loopback scope must reject every non-loopback address": ("ntcp2_runtime", "Self::Loopback => ip.is_loopback(),"),
            "NTCP2 private-LAN scope must admit only RFC1918 IPv4 literals": ("ntcp2_runtime", "Self::PrivateLan => matches!(ip, IpAddr::V4(address) if address.is_private()),"),
            "NTCP2 dial scope admission test must remain": ("ntcp2_runtime", "fn dial_scope_admits_only_its_profile_addresses()"),
            "NTCP2 dial scope admission set must be exactly the profile set": ("ntcp2_runtime", "fn dial_scope_admission_set_is_exactly_the_profile_set()"),
            "NTCP2 outbound dialing must verify the signed peer RouterInfo and network": ("ntcp2_runtime", "if verify_router_info(peer_info).is_err()"),
            "NTCP2 outbound links must complete authentication before promotion": ("ntcp2_runtime", ".drive_initiator_handshake("),
            "NTCP2 default-off config test must remain": ("config", "fn omitted_ntcp2_section_means_disabled()"),
            "ordinary enabled NTCP2 config must require an explicit profile": ("config", "fn explicit_ntcp2_enabled_true_is_rejected()"),
            "controlled NTCP2 config negative tests must remain": ("config", "fn ntcp2_controlled_loopback_requires_explicit_loopback_and_port()"),
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

    errors.extend(
        message
        for message, (name, needle) in source_controls().items()
        if needle not in sources[name]
    )
    live_ntcp2_endpoint = re.search(
        r"generate_controlled_ntcp2_router_info\(\s*&bundle,\s*"
        r"listener\.local_addr\(\)\.ip\(\),\s*"
        r"listener\.local_addr\(\)\.port\(\),",
        sources["daemon"],
    )
    if live_ntcp2_endpoint is None:
        errors.append("NTCP2 RouterInfo must use the live bound endpoint")
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
    mutations.extend((name, needle) for name, needle in source_controls().values())
    # The live-endpoint rule is enforced by a regular expression rather than
    # a substring, so it needs its own inverted probe.
    mutations.append(("daemon", "listener.local_addr().ip(),"))
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
