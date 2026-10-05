# ADR 0032: Managed native applications use a separate process and scoped capabilities

Status: accepted for the Plan 345 contract foundation.

## Context

Native applications may need access to router services, optional brokered
network access, package resources, and a console UI bridge. Giving arbitrary
application code router internals or a general administrator credential would
make that code part of the router's trusted computing base. A normal child
process with unrestricted host networking is also not a secured application
profile: loopback services and local IPC can provide indirect host access.

## Decision

- Managed apps are separate processes. They are never runtime-loaded Rust
  plugins or dylibs in the router or console security domain.
- `i2pr-daemon` remains the router-side authority and composition point for
  router capabilities. A separate trusted user-space application manager owns
  package lifecycle, process supervision, OS sandboxing, resource containment,
  and any future clearnet broker. These responsibilities do not move into
  `i2pr-runtime`.
- The secured launch profile denies direct host networking, including
  loopback. I2P access is provided over an inherited, app-scoped capability
  channel adapted to existing SAM/I2CP owners. It does not create a second
  router, SAM, I2CP, or Proposal 170 implementation.
- Proposal 170 remains administrative control semantics. A future app adapter
  must authorize a narrow operation set against the app principal and
  app-owned resources; it must never hand an app the administrator credential.
  Binding that adapter depends on the canonical completed Proposal 170
  contract and is outside Plan 345.
- Direct clearnet access, if later offered, is brokered under administrator-
  owned default-deny policy. DNS resolution is trusted broker work and resolved
  addresses are checked against the permitted address scopes. Apps cannot
  modify policy or switch launch profiles. Direct-host-network operation is a
  distinct operator-selected relaunch profile.
- First-party apps use the same trust model as third-party apps unless the
  operator explicitly selects a separately named override.
- Embedded UI code is an untrusted app principal/origin. It receives no
  administrator token, console DOM authority, or independent networking path.
  UI resources are package-relative static resources.
- App, publisher, launch-instance, router, and I2P Destination identity domains
  are distinct. Package permission requests, administrator grants, and current
  effective capabilities are represented separately.

## Consequences

The portable contract can be implemented and tested without selecting an OS
sandbox mechanism. Future secured backends must fail closed when they cannot
attest all required properties. The contract is infrastructure only; it does
not launch an app, establish containment, or make a router-facing capability
available.

Preventing unauthorized direct clearnet, LAN, or loopback egress does not make
malicious application code anonymous. An app may intentionally encode
identifying information into traffic it is allowed to send.

## Scope

This ADR freezes trust ownership and non-guarantees. It does not decide a
package archive/signature format, OS backend, UI toolkit, broker implementation,
process protocol adapter, or package lifecycle behavior.
