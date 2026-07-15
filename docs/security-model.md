# Initial security model

Milestone 0 establishes boundaries and validation behavior. It does not claim
anonymity, privacy, censorship resistance, production readiness, or protocol
security.

## Assets

Future assets include router identity keys, peer and destination metadata,
RouterInfo/LeaseSet data, tunnel state, client traffic, configuration, local
service endpoints, resource capacity, and diagnostic output. The bootstrap has
no persistent identity and does not create network state.

## Adversaries and trust boundaries

The design treats the following as untrusted:

- Remote unauthenticated peers and authenticated-but-malicious peers.
- Malicious or malformed SAM/I2CP and application clients once those adapters exist.
- Malformed command-line arguments and local TOML configuration.
- Corrupted or stale persisted network state.
- Malicious or misleading reseed material.
- Resource-exhaustion attempts and oversized inputs.
- Dependencies, build tools, CI actions, and other supply-chain inputs.

Trust boundaries are the protocol decoder, configuration parser, persisted-state
loader, client adapters, local service boundary, and daemon composition root.
Each future boundary must validate input before handing a narrower capability to
the next subsystem.

## Objectives and required properties

Future implementations must use explicit size/count/time/nesting limits,
complete-consumption parsing where canonical encodings require it, bounded
queues, deadlines, cancellation, and cleanup. Resource exhaustion must cause
rejection, deferral, backoff, or shedding rather than unbounded memory growth.
Persisted network data must be revalidated on load, and security-sensitive
writes must be atomic or recoverable.

Secret-bearing types must avoid accidental `Debug`, `Display`, unrestricted
serialization, and unnecessary cloning. Production cryptography must use
reviewed implementations; deterministic randomness belongs only to tests and
explicit reproducibility tooling.

## Observability and non-claims

Default logs and metrics must not expose full router hashes, destination
identities, keys, session material, LeaseSet secrets, packet bodies, user
traffic, or unbounded identity-bearing labels. Detailed identity-rich tracing
requires an explicit unsafe-debug mode and warning.

Nothing in the bootstrap proves anonymity, resistance to traffic analysis,
correctness against hostile peers, secure identity persistence, protocol
interoperability, or safe public-network operation. Malformed and stress tests
must run only in an authorized isolated testnet.
