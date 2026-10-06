# `i2pr-app-manager-proto` — trusted router/AppManager protocol contract

`i2pr-app-manager-proto` (Plan 368) owns the **router-facing half** of the
managed native application runtime: the wire contract between `i2pr-daemon` and a
future separately supervised trusted application manager. It is a runtime-neutral
leaf contract crate whose only production `i2pr-*` dependency is
`i2pr-app-proto`.

It owns no transport, no sockets, no process launching, no filesystem, no DNS, no
Tokio, no sandbox backend, and no router state. The normative,
language-neutral authority is
[`managed-app-manager-protocol-v1.md`](../../specs/references/managed-app-manager-protocol-v1.md);
the security rationale and the transport intent are ADR 0035.

## What it is

An internal trusted-component protocol whose authority ceiling is deliberately
**smaller** than Proposal 170. It lets a trusted manager project an already
authenticated application principal into the router's existing private
capability gateway:

| Manager → daemon | Daemon → manager |
| --- | --- |
| `create_session` | `session_opened` / `session_closed` |
| `close_session` | `service_opened` / `service_closed` / `service_reset` |
| `open_service` (`sam` \| `i2cp`) | `health_status` / `shutdown_ack` |
| `close_service` / `reset_service` | `rejected` (terminal, correlated) |
| `health` / `shutdown` | `service_ended` / `session_ended` (notifications) |

It cannot install packages, grant or revoke permissions, mutate launch or
network policy, launch or stop processes, read or write router configuration, or
dispatch general Proposal 170. There is deliberately **no administrator message
variant** on this protocol.

## Public surface

Frozen protocol facts (see §4.3 of the reference spec):

| Constant | Value |
| --- | --- |
| `MANAGER_PROTOCOL_MAJOR` / `MINOR` | `1` / `0` |
| `HANDSHAKE_MAGIC` | `I2PM` (never `I2PA`) |
| `FRAME_HEADER_BYTES` / `FRAME_VERSION` | `12` / `1` |
| `MAX_CONTROL_BYTES` | 16 384 |
| `MAX_DATA_FRAME_BYTES` | 65 536 |
| `MAX_MANAGER_SESSIONS` | 32 |
| `MAX_SERVICE_STREAMS_PER_SESSION` | 128 |
| `MAX_INFLIGHT_REQUESTS` | 64 |
| `MAX_GATEWAY_CONNECTIONS` | 128 |

Types: `Handshake`/`ManagerRole`; `ManagerToDaemonMessage` and
`DaemonToManagerMessage` (direction-strict, `deny_unknown_fields` tagged
enums); `ManagerService` (`Sam`, `I2cp` only); `ManagerSessionId` and
`ManagerServiceStreamId` (opaque, daemon-assigned, non-zero, never reused);
`ManagerGatewayLimits`; `EffectiveGrant`; `ManagerError`/`ManagerErrorCode`;
`ServiceEndReason`; `Frame`/`FrameKind`; and the pure bounded accounting types
`ManagerScopeLimits` and `ServiceStreamLedger`.

`EffectiveGrant` is deliberately **not** `RequestedCapability`. There is no
decoder from an application `hello`, a `RequestedCapability`, or manifest bytes
into it, so the bridge has no path by which application-declared authority could
become router authority.

## Decoding is stricter than serde's defaults

Two properties required explicit work and are easy to lose in a rewrite:

1. **Duplicate member names are refused.** `#[serde(deny_unknown_fields)]` does
   *not* reject `{"request_id":1,"request_id":2}`; serde keeps the last
   occurrence and decodes successfully, making two distinct payloads
   indistinguishable on the wire. `reject_duplicate_object_keys` is a bounded,
   allocation-free JSON scanner that fails closed on any repeated member name at
   any nesting depth.
2. **Reserved handle value 0 is rejected on decode.** `ManagerSessionId` and
   `ManagerServiceStreamId` use `#[serde(try_from = "u64")]` so `"0"`, `"-1"`, and
   non-numeric values cannot decode into a handle.

`control_scoped` is **unrepresentable**: `ManagerService` has no variant for it,
so no decode path can name it. `ManagerService::parse` additionally recognises the
literal spelling and returns `UnsupportedService`, so a peer that asks is refused
by type rather than silently downgraded.

## Consumer

The only production consumer is `i2pr-daemon`, in
`crates/i2pr-daemon/src/app_manager_bridge.rs`. That bridge implements the daemon
side over an **injected** reliable duplex stream; the concrete anonymous
inherited transport is Plan 369's concern. `scripts/check-managed-app-manager-boundary.py`
and `scripts/check-runtime-boundaries.sh` both enforce the boundary statically.

## Known limitations

- Transport-agnostic by design, so this crate cannot itself prove that the
  eventual transport is anonymous. That is asserted at the daemon supervisor
  (Plan 369) and by ADR 0035.
- `ServiceEndReason` distinguishes observed causes but the bridge cannot always
  attribute one precisely; it reports what it observed rather than guessing.
- No administrator vocabulary exists, by design. Proposal 170 remains the router
  administrator surface, and it is deliberately **not** reachable here.