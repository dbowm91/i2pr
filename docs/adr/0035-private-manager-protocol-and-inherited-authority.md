# ADR 0035 — private manager protocol and inherited authority for the managed application runtime

Status: Accepted  
Date: 2026-10-06  
Supersedes: nothing  
Related: ADR 0032 (`0032-managed-native-app-process-and-capability-boundary.md`)

## Context

Plan 345 froze a runtime-neutral, language-neutral managed-application contract in
`i2pr-app-proto` and Plan 352/353 closed its pre-runtime defects. Plans 354 and 355
then closed the router side of the capability boundary: SAM and I2CP gained
listener-independent private connection seams, and `AppGatewaySession` binds one
trusted `AppPrincipal` to one immutable `EffectiveCapabilities` value, with
capability checks performed before backend allocation and managed SAM denying
`STREAM FORWARD`.

That left a deliberate hole. `crates/i2pr-daemon/src/app_gateway.rs` is
crate-private and, as of Plan 355, had **no production runtime caller**. The router
could decide whether an application was permitted to use SAM or I2CP, but nothing
outside the daemon could ask it that question.

Filling that hole by exposing a listener is not acceptable. The obvious
implementations are all wrong in ways that matter:

1. **A SAM/I2CP TCP listener.** Loopback is not a trust boundary. A managed
   application can reach loopback through a local proxy, helper, or any other
   process on the host, so a loopback credential is not scoped to the application
   that was granted it. `AGENTS.md` and ADR 0032 already treat loopback as
   "not implicitly trusted".
2. **A general Proposal 170 / I2PControl credential.** Proposal 170 is a router
   *administrator* surface. Handing it to an application runtime converts a
   scoped, principal-bound capability into unrestricted router control. This is
   the exact failure ADR 0032 exists to prevent.
3. **Linking router internals into the manager process.** The manager would then
   share the router's address space, its secrets, and its authority.
4. **Trusting application-supplied bytes.** An application's `hello`, its
   requested-capability list, or its manifest are attacker-controlled input. If
   any of those can construct router authorization, the capability boundary in
   Plan 355 is decorative.

What is actually needed is narrower than any of those: a small internal protocol
whose authority surface is *smaller* than Proposal 170's, transported over a
capability that only the intended peer can possess.

## Decision

### 1. The router remains the authority; the manager is a separate trusted process

`i2pr-daemon` remains the router-side authority for I2P protocol state and for
app-facing router capabilities. A future `i2pr-appd` is a **separate trusted
process** that owns application launch-instance identity, the application-facing
protocol state machine, and per-launch process supervision.

The manager does not own router protocol internals, NetDB/tunnel/transport state,
administrator credentials, package storage, or sandbox enforcement.

### 2. Authority travels over a private inherited capability transport

Daemon↔manager authority is carried by an anonymous **inherited** capability
transport, never by a discoverable endpoint. Possession of the inherited handle is
the manager-process authentication fact.

Forbidden as the manager transport: TCP (including loopback), a
filesystem-discoverable Unix-domain socket, a globally named pipe endpoint, a
user-configurable arbitrary command, and any temporary localhost fallback.

The concrete transport binding (inherited anonymous pipe ends) is owned by the
supervising plan; this ADR fixes the *intent*, not the mechanism, so the contract
crate remains transport-agnostic and testable in memory.

### 3. A distinct private manager protocol, smaller than Proposal 170

The manager protocol (`i2pr-app-manager-proto`, normative reference
`specs/references/managed-app-manager-protocol-v1.md`) is an internal
trusted-component contract. It is **not** the application v1 protocol, not SAM,
not I2CP, not Proposal 170, and not an AppManager administrator API.

It may: create and close one app gateway session; open SAM/I2CP service
connections that the session's immutable effective capabilities already permit;
forward opaque protocol octets in order; report backend termination; and answer
bounded health and shutdown.

It may not: install/update/uninstall packages; grant, revoke, or persist
permissions; mutate network policy or launch profile; launch or stop processes;
read or write router configuration; dispatch general I2PControl; or name daemon
methods.

A compromised manager must not be able to turn this bridge into
router-administrator authority.

### 4. Manager authority is distinct from application declarations

The daemon bridge never constructs authorization from an application's `hello`, a
`RequestedCapability`, manifest bytes, or service data bytes. There is no decoder
from any of those into the manager's authority type.

Only a session-create request on the already-trusted manager transport may supply a
principal and effective grants, and the daemon still re-derives those grants through
the existing administrator-grant path before any backend is allocated. The daemon
validates every protocol message and enforces the ceiling of authority available
through this bridge regardless of what the manager asserts.

### 5. `control_scoped` is unrepresentable on this protocol

The manager protocol's service vocabulary contains exactly `sam` and `i2cp`.
`control_scoped` cannot be named in the wire contract, and a peer that asks for it
receives a typed unsupported error rather than a silent downgrade. Scoped control
access remains a separate, separately-gated adapter behind canonical Proposal 170
completion.

### 6. The daemon authenticates the transport, not the application

This ADR does not establish app-process authentication. The manager is responsible
for authenticating applications in a later plan; the daemon's obligation stops at
validating the bounded authority the manager submits.

Consequently, an application `hello` is **declaration matching, not
authentication**: it confirms the process is speaking for an identity the manager
already selected. Any plan that treats `hello` as an authentication factor is
incorrect.

### 7. Manager failure is not a router-core availability dependency

Manager crash, EOF, or protocol violation tears down every manager-created gateway
session and backend connection, then degrades the application runtime. It must not
fail the router. Exhausting a bounded restart budget degrades or disables the
application-runtime feature; it does not shut the router down.

A daemon restart loses all manager sessions. No persistence is introduced, and
manager restart recovery is out of scope for this boundary.

### 8. No sandbox guarantee exists yet

This boundary provides **no** containment claim. It does not restrict an
application's direct networking, filesystem, process tree, or grandchildren. The
`secured` launch profile must fail closed until a qualified OS sandbox backend
exists; fabricating a passing attestation is forbidden.

Non-guarantee stated explicitly for the roadmap: blocking unauthorized direct egress
is not the same as preventing an application from encoding identifying data into
traffic it is legitimately permitted to send.

## Consequences

- `i2pr-app-manager-proto` is a new runtime-neutral workspace crate with zero
  workspace dependencies beyond `i2pr-app-proto`. It owns no transport, sockets,
  process APIs, filesystem APIs, DNS, timers, or sandbox backends.
- The dependency direction gains `i2pr-app-manager-proto -> i2pr-app-proto` and
  `i2pr-daemon -> i2pr-app-manager-proto`. Neither the contract crate nor the manager
  may depend on `i2pr-daemon`, `i2pr-runtime`, router protocol/state crates, or
  `i2pr-client`.
- Static guards must prove the boundary rather than assert it: that only the daemon
  consumes the protocol as a router-side implementation, that the contract crate
  cannot reach a router/runtime/OS owner, that the bridge contains no loopback
  socket/listener/connect path, that the control vocabulary contains no
  admin/config/package operation, that `control_scoped` stays denied, and that the
  application-facing `hello` is never referenced as authorization input.
- No SAM/I2CP wire version, support-inventory entry, or RouterInfo advertisement
  changes. The manager protocol is private infrastructure, unreleased, and is not an
  external SDK or API compatibility promise.

## Alternatives rejected

- **Expose the private gateway on a loopback listener.** Rejected: loopback is not a
  capability boundary; the credential would be reachable by unrelated host processes.
- **Give the manager a Proposal 170 credential.** Rejected: that is router
  administrator authority, strictly larger than this bridge is permitted to have.
- **Link router internals into the manager.** Rejected: it collapses the trust
  boundary and shares router secrets and address space.
- **Let the application declare its own capabilities over this protocol.** Rejected:
  it reinstates exactly the promotion attack ADR 0032 prohibits.
- **Ship the protocol without an ADR freeze.** Rejected: the authority ceiling and
  the transport intent are the security-relevant decisions, and implementing first
  would make them implicit.