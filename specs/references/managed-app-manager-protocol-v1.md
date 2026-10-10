# Managed Application Manager Protocol v1 (private)

Status: **normative internal reference** — Plan 368.

Authority: ADR 0035 (`docs/adr/0035-private-manager-protocol-and-inherited-authority.md`),
ADR 0032 (`docs/adr/0032-managed-native-app-process-and-capability-boundary.md`).

This document defines a **private, internal, language-neutral** protocol between the
i2pr router daemon and a separately supervised trusted application manager. It is
not an external API, not an SDK promise, and not a published specification.

## 1. Scope

The manager is a trusted router-side component that projects an *already
authenticated* application principal into the router's private SAM/I2CP capability
gateway. This protocol carries that projection.

**In scope:** handshake, bounded frames, request/reply correlation, app gateway
session create/close, SAM/I2CP/datagram service open/close/reset,
administrator-granted daemon-owned loopback listener publication/unpublication,
accepted-stream notifications, ordered service data octets, backend termination
notification, bounded health and shutdown.

**Explicitly out of scope.** A peer of this protocol cannot express any of:
package install/update/uninstall; grant, revoke, or permission persistence;
network-policy or launch-profile mutation; process launch or stop; router
configuration read/write; general Proposal 170 / I2PControl dispatch; daemon method
names; `control_scoped`; administrator operations of any kind.

There is deliberately no administrator message variant on this protocol.
The v1.1 `local_service` messages only exercise a grant already fixed by the
offline policy store; they cannot create or change a grant.

## 2. Relationship to other contracts

The private manager protocol remains major version 1 and is currently minor
version 1. Plan 408 added local-service control messages; the minor remains
compatible because the handshake treats minor as informational.

This protocol is **not**:

| Contract | Relationship |
|---|---|
| Managed application protocol v1 (`i2pr-app-proto`, `managed-native-app-runtime-v1.md`) | Separate protocol, separate magic, separate vocabulary. Shared value types only. |
| SAM (`i2pr-api`) | The `sam` service carries opaque SAM octets. `sam_datagram` is a separate typed binary service and is never mixed into the SAM stream. |
| I2CP (`i2pr-api`) | Carried as opaque octets on a service stream. Never parsed here. |
| Proposal 170 / I2PControl (`i2pr-i2pcontrol`) | Not reachable. Its authority is strictly larger. |

Consequence: no `I2PA`-prefixed byte is a valid manager-protocol byte, and no
`I2PM`-prefixed byte is a valid application-protocol byte.

## 3. Transport requirements

The transport must be an anonymous capability: possession of the handle is the
peer authentication fact. Concretely the daemon is the parent and owner, and hands
the manager inherited handles.

Forbidden: TCP (including loopback), filesystem-discoverable Unix-domain sockets,
globally named pipe endpoints, user-configurable arbitrary commands, and any
localhost fallback "for testing" that becomes production behaviour.

The daemon remains the validating party regardless. Transport possession
authenticates the *manager process*; it does not exempt the manager from message
validation or from the authority ceiling in §5.

### 3.1 Concrete inherited binding (normative, Plan 369)

Plan 368 fixed the transport as an anonymous inherited capability but chose no
concrete binding. Plan 369 binds it normatively:

- The daemon creates **two anonymous pipes** and hands the manager the read end
  of the daemon→manager pipe on **file descriptor 0 (stdin)** and the write end
  of the manager→daemon pipe on **file descriptor 1 (stdout)**. The manager's
  read half is therefore stdin and its write half is stdout.
- File descriptor 2 (stderr) is **reserved for the manager's own diagnostics**.
  It is never protocol, and nothing in this contract interprets it as control.
- The manager executable is **distribution-owned**: the `current_exe()` sibling
  of the daemon, with platform suffix rules applied. No configuration value,
  environment variable, or command-line argument may select it.
- The manager MUST refuse **all** arguments rather than ignore them: an ignored
  argument is indistinguishable, from the outside, from one that was understood
  and ignored.
- There is no socket, port, discovery endpoint, or standalone mode. A manager
  started without an inherited transport has nothing to talk to and holds no
  authority.

`scripts/check-managed-app-process-boundary.py` enforces the executable
resolution, the absence of a shell launcher and of any `PATH` lookup, and the
argument refusal.

## 4. Wire format

All integers are big-endian. Bytes are counted in octets.

### 4.1 Handshake (exactly 9 octets, once per direction, immediately on connect)

| Offset | Size | Value |
|---|---|---|
| 0 | 4 | Magic `I2PM` (`0x49 0x32 0x50 0x4D`) |
| 4 | 1 | Major version (`1`) |
| 5 | 1 | Minor version (`0`; informational) |
| 6 | 1 | Role (`1` = manager) |
| 7 | 2 | Reserved, must be `0x0000` |

Rejection rules — each is a terminal handshake failure:

- length ≠ 9 → malformed;
- magic ≠ `I2PM` → bad magic (this includes the application-protocol magic `I2PA`);
- role ≠ 1 → role mismatch;
- reserved ≠ 0 → malformed;
- major ≠ 1 → unsupported version.

Minor is **not** a gate: a compatible later minor is admitted, so the daemon and
manager versions may move independently.

### 4.2 Frames

Header is exactly 12 octets, followed by the payload.

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | Frame version (`1`) |
| 1 | 1 | Kind (`1` control, `2` data) |
| 2 | 2 | Reserved, must be `0x0000` |
| 4 | 4 | Stream id |
| 8 | 4 | Payload length |

- A control frame **must** carry stream id `0`.
- A data frame **must** carry a non-zero stream id, which is the daemon-assigned
  `service_stream` handle.
- Declared payload length must not exceed the kind's ceiling (§4.3). It is
  validated **before** any allocation or copy, and header addition is checked
  arithmetic, so a near-overflow length fails closed.
- Frame version, kind, and reserved bytes are validated exactly; an unknown value
  is refused, never defaulted.

### 4.3 Ceilings (frozen protocol facts)

| Limit | Value |
|---|---|
| Control payload | 16 384 octets |
| Data payload | 65 536 octets |
| Concurrent sessions per manager transport | 32 |
| Live service streams per session | 128 |
| In-flight correlated requests | 64 |
| Diagnostic / reason string | 1 024 octets, ASCII only |
| Requested gateway connections | 1 … 128 |
| Published local services per session | 8 |
| Accepted local-service streams per session | 16 |
| Local-service inbound queue | 8 data frames per stream |
| Local-service bind address | `127.0.0.1` only |

Every queue, count, frame, request, and stream in this protocol is bounded by one
of these values or by an equivalent router-owner ceiling.

### 4.4 Control encoding

Control payloads are JSON objects with a `"type"` discriminator. Decoders:

- reject an unknown `"type"`;
- reject any unknown member name;
- reject a **repeated** member name at any nesting depth — decoders must not
  resolve duplicates by last-occurrence-wins, which would make two distinct
  payloads indistinguishable;
- reject trailing octets after the JSON value;
- reject payloads above the control ceiling before parsing.

### 4.5 Application instance id

The application instance id carried in a session-create principal is a JSON
**string of canonical decimal digits**, at most 39 bytes (the exact digit count
of a 128-bit maximum). Canonical form is a total function: exactly one spelling
per id is accepted. A JSON number, a sign, whitespace, leading zeros, exponent or
fractional forms, and non-ASCII digits are all rejected, as are the empty
string, the value `0`, and any length above the bound.

This is deliberately the *same* grammar and the *same* bound as the application
protocol's `AppInstanceId`
(`specs/references/managed-native-app-runtime-v1.md` §1), and it is defined
once, by that type. The two protocols therefore cannot drift into accepting
different spellings of one id, and a principal crossing this boundary is not
silently reinterpreted.

The representation is normative because of the encoding rule above: an
internally tagged object must be buffered whole before it can be decoded, and
that buffer cannot carry a 128-bit JSON number. Decimal digits survive it.

## 5. Authority ceiling and validation

The daemon enforces, in this order and before any backend allocation:

1. **Transport** is a valid manager capability.
2. **Handshake** matches §4.1.
3. **Message** decodes and validates per §4.4.
4. **Request correlation** — the `request_id` is non-zero and either already
   in-flight (duplicate → rejected) or newly admitted.
5. **Session create**: the principal is well-formed; the effective grant set
   contains no duplicates; the requested connection limit is within §4.3.
   Grants are then re-derived through the router's administrator-grant path. The
   manager's assertion is an *input*, never the authorization itself.
6. **Service open**: the named session exists and is owned by this transport; the
   service is `sam` or `i2cp`; and the session's immutable effective capabilities
   contain the corresponding capability. A denial here is **side-effect free** — no
   backend context is created, no permit is consumed, no slot changes.

A session's effective capabilities are fixed at creation and cannot change while
the session is live. There is no message that raises them.

`control_scoped` is not in the service vocabulary. A peer naming it receives a
typed unsupported error.

## 6. Message/authority matrix

| Manager → daemon | Authorizes | Allocates | Notes |
|---|---|---|---|
| `create_session` | one app gateway session | session record only | no backend until a service open |
| `close_session` | teardown of one session | tears down descendants | emits `session_ended` |
| `open_service` (`sam`/`i2cp`) | one backend connection | backend context + permit | capability-gated, pre-allocation |
| `close_service` | one backend connection close | releases that stream | |
| `reset_service` | one backend connection reset | releases that stream | bounded reason string |
| `health` | a bounded status reply | nothing | |
| `shutdown` | manager-initiated shutdown | tears down all sessions | bounded reason string |

| Daemon → manager | Correlation | Meaning |
|---|---|---|
| `session_opened` | request | opaque session handle assigned |
| `session_closed` | request | session teardown acknowledged |
| `service_opened` | request | opaque service-stream handle assigned |
| `service_closed` | request | stream close acknowledged |
| `service_reset` | request | stream reset acknowledged |
| `health_status` | request | bounded state string |
| `shutdown_ack` | request | shutdown accepted |
| `rejected` | request | terminal refusal of a correlated request |
| `service_ended` | none | backend for one stream ended |
| `session_ended` | none | session and all descendants torn down |

Exactly one terminal reply per correlated request. Notifications never carry a
`request_id`; a reply always does. A reply that does not match an in-flight request
is a protocol violation.

## 7. Handles

Session and service-stream handles are **opaque, daemon-assigned, non-zero**,
and meaningful only to the issuing daemon and only within one manager transport.

- No Rust address, pointer, connection id, router state, or daemon-internal
  identifier appears on the wire.
- `0` is reserved so a handle is never ambiguous with "absent".
- A stale or closed handle fails deterministically with `not_found`.
- A handle from another session or another transport is invalid, not merely
  unauthorised; cross-session use is refused before any lookup succeeds.
- Handles are never reused within one transport's lifetime.

## 8. Service data mapping

For an authorised service open:

- one manager service stream maps to exactly one router backend connection;
- manager data payloads are forwarded as **exact** SAM/I2CP protocol octets;
- no base64, JSON wrapping of service data, rewriting, reordering, or
  interpretation is introduced;
- delivery is ordered per stream;
- backpressure is bounded: a slow manager cannot cause unbounded buffering, and a
  bounded buffer fills into a deterministic stream reset rather than a stall or an
  allocation.

## 9. Failure, cancellation, and lifecycle semantics

| Event | Behaviour |
|---|---|
| malformed or unsupported manager input | closes/rejects only the offending request or session, per the above |
| unauthorised service open | side-effect free |
| manager transport EOF | cancels **every** manager-created session and backend connection |
| one backend EOF | closes **only** that service stream |
| one session failure | does not close sibling sessions unless the transport itself failed |
| max+1 session / stream / request | rejected synchronously |
| daemon restart | all manager sessions lost; no persistence |
| manager restart | recovery is out of scope for this boundary; restart begins empty |
| manager hung / crashed | the app runtime degrades; the router stays usable |

There is no unbounded queue and no automatic retry loop.

Supervision is the daemon's, not the protocol's: the manager is spawned as a
direct child, and on shutdown the daemon drops the pipe ends — which is EOF,
which is the manager's only shutdown signal — then escalates to killing and
reaping the direct child after a bounded grace. A manager that fails its
handshake, violates the framing, or reaches EOF has already stopped speaking the
protocol, so it takes the short reap path rather than the full graceful grace.

**Non-goal:** grandchild containment. Only the manager is owned and reaped; the
apphost it starts in turn owns and reaps the application. No claim is made about
processes below that line.

## 10. Message and authority matrix summary

`create_session`, `close_session`, `open_service`, `close_service`, `reset_service`,
`health`, `shutdown` are the complete manager vocabulary. A conforming
implementation MUST reject any other `"type"` rather than ignoring it, because an
ignored administrative request is a silent authority grant.

## 11. Non-guarantees

This protocol provides **no** security containment. Specifically it does not:

- restrict an application's direct networking, DNS, or loopback access;
- provide filesystem or process isolation;
- contain grandchildren or a process tree;
- authenticate the application process (see ADR 0035 §6 — `hello` is declaration
  matching);
- provide a sandbox attestation;
- survive a daemon restart.

Blocking unauthorized direct egress would not stop an application from encoding
identifying data into traffic it is legitimately permitted to send.
