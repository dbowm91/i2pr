# ADR 0029: Anonymity boundaries, implementation neutrality, and profile convergence

- Status: Accepted
- Date: 2026-10-02
- Decision owner: repository maintainer
- Related: GUARDRAILS.md, specs/CONFORMANCE.md, docs/security-model.md, ADR 0001, ADR 0026, M10 service tunnels, Destinations/Streaming, Plans 296–301

## Context

i2pr is still experimental and makes no production anonymity or privacy claim. That non-claim remains correct. The current router nevertheless already exposes application-facing service tunnels, and an anonymity audit found several places where router implementation identity can cross or shape an anonymity boundary.

The most direct defect is the HTTP client profile's stable `User-Agent: i2pr/0.1`. The same HTTP path can synthesize `Host` from the caller-visible name rather than the resolved Destination, which can disclose a user-local address-book alias. The local HTTP CONNECT response also identifies `Proxy-Agent: i2pr`, and the IRC stable reason-rewrite token is `i2pr` when that optional mode is selected. Beyond literal strings, deterministic HTTP serialization and Streaming behavior can form an implementation classifier even after explicit branding is removed.

The mainstream Java I2P and i2pd HTTP proxy implementations intentionally converge on an implementation-neutral in-network HTTP user agent and canonical `.b32.i2p` Host behavior. i2pr should join an existing anonymity set rather than invent a new stable token or randomize behavior into a potentially unique profile.

This decision concerns implementation unlinkability and observable-profile convergence. It is not a claim of resistance to a global passive adversary, end-to-end timing analysis, browser fingerprinting, TLS ClientHello fingerprinting, application-level self-identification, or all forms of correlation.

## Decision

### 1. Define an application/service anonymity boundary

For service-tunnel traffic delivered to a remote I2P Destination, router-synthesized bytes must not contain the i2pr product name, package/release version, git/build identifier, host operating-system/runtime identifier, router identity, local hostname/IP address, filesystem path, or user-local Destination alias unless an I2P protocol at that exact layer requires the value.

The rule applies to HTTP/IRC profile rewriting and to future application-aware tunnel profiles. Opaque generic/SOCKS/CONNECT forwarding is required not to add router branding of its own, but it cannot make the forwarded application anonymous by rewriting encrypted or arbitrary application protocols.

### 2. Canonical remote naming uses the resolved Destination

When an application protocol requires a remote I2P host name on the wire, i2pr derives the canonical `<destination-hash>.b32.i2p` representation from the resolved Destination. A caller's local address-book alias is local state and must not cross the service boundary by default.

### 3. Converge on established profiles; do not randomize fingerprints

Stable router-generated application behavior should converge on a documented, widely deployed I2P profile where practical. Random per-request header casing, order, token strings, timing, or protocol options are not an anonymity strategy because rare combinations may increase fingerprintability.

For HTTP, the first convergence references are the repository-pinned Java I2P 2.13.0 and i2pd 2.61.0 families. For Streaming, values are not changed merely by reading source constants: black-box observable behavior is measured first, then a coherent compatibility target is selected from evidence.

### 4. Privacy profiles are bundled, not a combinatorial default surface

Application-aware tunnel privacy behavior should expose a small number of named profiles. The default profile is the compatibility/anonymity profile proven by the workstream. Raw/keep behavior may remain as an explicit compatibility escape hatch, but it must be documented as reducing the common anonymity set.

Independent low-level toggles may exist internally or for expert compatibility use; they are not the default product contract.

### 5. Destination identity isolation remains explicit

Dedicated client Destination identity remains the default service-tunnel policy. Sharing is explicit, bounded, and documented as increasing linkability between remote services.

Any service listener that can reach multiple unrelated remote Destinations must be audited for target-level identity reuse. Where one client Destination would let colluding remote services correlate otherwise unrelated targets, the anonymity profile must introduce target/first-party isolation or fail the workstream's qualification gate.

### 6. Lower-layer implementation fingerprints are qualified separately

A hostile remote Destination can observe Streaming packet shape, retransmission/ACK behavior, timing, and application passthrough behavior. A network observer may also learn statistical path characteristics. These surfaces are separate qualification dimensions.

The workstream therefore separates literal service-boundary leaks, HTTP application-profile convergence, active Streaming fingerprint measurement/convergence, Destination/tunnel path isolation/diversity, and integrated evidence/documentation gates.

A clean HTTP header does not imply lower-layer indistinguishability.

### 7. Router-plane protocol metadata is not automatically branding

Protocol-mandated router metadata remains governed by the I2P protocol and truthful support rules. RouterInfo `router.version` is a protocol/support declaration, not the package release string. It must remain truthful and must not be removed merely because it is called a version.

### 8. Claims remain evidence-gated and narrow

No plan in this workstream authorizes the statement that i2pr is production-anonymous, Tor-Browser-equivalent, resistant to global traffic analysis, or able to anonymize arbitrary applications.

The strongest claim this workstream may eventually authorize is scoped: tested service profiles do not add a known i2pr-specific application fingerprint, Destination identity isolation satisfies the registered policy, and selected observable Streaming/path behaviors meet the pinned differential qualification.

`docs/security-model.md` remains conservative until the final integrated plan closes with executed evidence.

## Rejected alternatives

- Replace `i2pr/0.1` with another i2pr-specific generic token: still partitions the anonymity set.
- Strip every HTTP field without comparison to deployed I2P proxies: absence patterns can themselves be implementation fingerprints and may break compatibility.
- Randomize HTTP serialization or Streaming timing/options: creates unstable and potentially unique profiles instead of joining a deployed population.
- Treat CONNECT/SOCKS as equivalent to Tor Browser: opaque forwarding exposes the application's own TLS/protocol fingerprint.
- Share one client Destination across unrelated service targets for convenience: enables direct cross-service correlation by colluding Destinations.
- Fold this work into M12: the concerns cross service tunnels, Streaming, NetDB/Destination isolation, and tunnel selection and should not block unrelated floodfill progression.

## Review triggers

Revisit this ADR through a new accepted ADR if I2P HTTP proxy conventions materially change; pinned Java/i2pd black-box evidence shows the selected profile is no longer representative; the service-tunnel model changes from fixed-target to general multi-target behavior; a new application-aware tunnel family is introduced; or a future design proposes a broad anonymity claim beyond the scoped evidence defined here.
