# ADR 0039: Daemon-Owned Managed-App Local-Service Ingress

Status: **accepted**  
Date: 2026-10-09  
Plan: 408

## Context

Managed applications need a local client endpoint for services such as a
Transmission RPC server. The managed runtime deliberately runs applications in
separate process trust zones. `i2pr-appd` is listener-free and speaks to the
daemon only over inherited anonymous pipes. Passing a socket to appd or an
application would add a descriptor capability and weaken that boundary.

## Decision

The daemon owns the listener and every accepted socket. The first profile is
IPv4 loopback (`127.0.0.1`) TCP only. There is no configurable bind address,
wildcard binding, proxy metadata, or authentication claim based on the peer's
loopback address. The daemon forwards bytes as bounded logical streams over the
existing manager pipe; the application receives stream identifiers only.

Publication requires the persistent administrator-origin `local_service`
grant. The application protocol advances to v1.1; a v1.0 client remains usable
but does not receive `local_service` in its effective capability list. A
manifest requesting the capability must include v1.1 in its declared host
protocol range. `brokered_tcp` remains reserved and unrelated.

Frozen ceilings: 8 published services and 16 accepted local-service streams per
session; names are unique ASCII tokens up to 64 bytes; requested ports are
1024–65535 or omitted for OS-selected ephemeral allocation; each stream uses an
8-frame inbound queue with each frame capped at 65,536 bytes. Bind conflicts
fail without retrying another externally visible port.

## Consequences

- Neither appd nor apphost gains sockets, ports, descriptors, or discovery.
- A secured application receives one explicit host capability without general
  host networking.
- Session exit, grant revocation followed by restart, unpublish, and router
  shutdown all cancel listener and stream ownership.
- Local callers still need application-level authentication where required;
  loopback is not an identity fact.
- The application and private manager protocol references must evolve together
  with the implementation.
