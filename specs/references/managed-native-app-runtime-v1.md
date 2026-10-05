# Managed native application contract v1

Status: frozen by ADR 0032 and Plan 345. This document is the language-neutral
contract; Rust enum tags, memory layout, and serializer defaults are not wire
ABI. Plan 345 implements vocabulary and pure validation only. It does not
provide an app runtime, transport, sandbox, DNS resolver, or network broker.

## 1. Versions and identities

The app channel handshake starts with the fixed 9-byte preamble `I2PA`,
protocol major `1`, protocol minor `0`, role byte (`1` Application, `2`
Administrator), and two zero reserved bytes. Reserved bytes MUST be zero.
Only one role is selected per connection; role changes require a new
connection. Unsupported major versions and unknown roles are rejected before
any operation. Minor-version differences do not authorize unknown message
types; extensions require a separately negotiated future contract.

`AppId`, `PublisherId`, and `AppVersion` are 1–64 bytes of lowercase ASCII
letters, digits, `.`, `_`, or `-`; first and last bytes must be alphanumeric.
They are case-sensitive and are not normalized. These are distinct types.
`AppInstanceId` and `OwnedResourceId` are opaque nonzero 128-bit values. An
`AppPrincipal` is an `AppId`, `AppInstanceId`, and optional `PublisherId`.
None of these values is a router identity or I2P Destination.

## 2. Frame envelope

Frames are concatenated on a reliable byte stream. Every frame is exactly:

| Field | Size | Encoding |
| --- | --- | --- |
| version | 1 | `1` |
| kind | 1 | `1` control, `2` data |
| flags | 2 | big-endian; must be zero in v1 |
| stream id | 4 | big-endian; zero for control, nonzero for data |
| payload length | 4 | big-endian, exact following byte count |
| payload | length | opaque bytes for data; UTF-8 JSON for control |

Header size is 12 bytes. Payload is at most 65,536 bytes. Empty data payload is
valid. Decoders consume exactly one complete frame and report bytes consumed;
truncation is an error, trailing bytes belong to the next frame. Unknown
version, kind, flags, invalid stream-id/kind combinations, and lengths above
the ceiling are errors. Length arithmetic is checked before allocation.

Control JSON is UTF-8, one JSON object, at most 16,384 bytes, at most 32
members, no duplicate keys, no unknown fields, and no trailing non-whitespace
bytes. String fields are bounded by the limits in §7. The `type` literal is
case-sensitive. Unknown literals fail closed.

## 3. Control vocabulary and roles

Every control message has a required string field `type`. Application-role
messages:

- `hello` `{type, app_id, instance_id, protocol_major, protocol_minor}`
- `capabilities` `{type, capabilities[]}`
- `open` `{type, request_id, stream_id, service}`
- `accept` `{type, request_id, stream_id}`
- `close` `{type, stream_id}`
- `reset` `{type, stream_id, reason}`
- `permission_request` `{type, request_id, capabilities[]}`
- `permission_status` `{type, request_id, status}`
- `ui_message` `{type, message_id, payload}`
- `health` `{type, state, detail?}`

`service` is one of `sam`, `i2cp`, `control_scoped`, or `brokered_tcp`.
`status` is one of `pending`, `denied`, or `recorded`; it is not a grant.
The app role has no grant, revoke, policy mutation, firewall disable, direct
network switch, install, update, or other-app mutation message.

Administrator-role messages use a disjoint vocabulary:

- `admin_hello` `{type, protocol_major, protocol_minor}`
- `install_request` / `update_request` / `uninstall_request` `{type, app_id}`
- `launch_request` / `stop_request` `{type, app_id}`
- `grant_request` / `revoke_request` `{type, app_id, capabilities[]}`
- `network_policy_request` `{type, app_id, rules[]}`
- `launch_profile_request` `{type, app_id, profile}`
- `resource_policy_request` `{type, app_id, resources}`
- `inspect_request` `{type, app_id}`

This describes future structural messages only; Plan 345 defines no handler or
side effect. A message valid for one role is invalid for the other. Numeric
substitution cannot change role or message meaning.

Stream ids are unique and nonzero within a session. Open must precede data;
duplicate open is an error. `close` and `reset` are idempotently representable.
At most 128 live logical streams and 64 in-flight request ids are permitted.

## 4. Capabilities

The closed v1 capability literals are `sam`, `i2cp`, `control_scoped`,
`brokered_tcp`, `ui_bridge`, `health`, and `lifecycle`. Capability lists are
unique and have at most 32 entries. Unknown literals are rejected.

`RequestedCapability` is inert app/package input. `GrantedCapability` is
administrator-origin policy state. `EffectiveCapabilities` is a bounded,
read-only runtime-produced view and cannot be constructed from manifest bytes
or application messages. Any conversion from requested to granted requires a
future explicit administrator owner. Capability scope is associated with an
`AppPrincipal` and never implies a general Proposal 170 credential.

## 5. Manifest v1

Manifest JSON is UTF-8, at most 65,536 bytes and 16 top-level fields, a single object with duplicate
and unknown keys rejected, and no trailing data. Top-level fields are exactly:
`schema_version` (integer `1`), `app_id`, `publisher_id`, `version`, `name`,
`description`, `host_protocol_min`, `host_protocol_max`, `entrypoints`,
`requested_capabilities`, `resources`, optional `ui`, `autostart_requested`,
and `restart_requested`.

`name` is 1–128 Unicode scalar values; `description` at most 1,024. Protocol
ranges are two `(major, minor)` pairs and minimum must not exceed maximum.
`entrypoints` has 1–8 `{target, path}` entries. Targets are unique bounded
ASCII identifiers. Paths are slash-separated package-relative components;
absolute paths, non-ASCII or characters outside ASCII alphanumeric plus `._-`,
empty components, `.`/`..`, backslash, colon, percent-encoding, query/fragment,
control bytes, URL schemes, host/port forms, and NUL are rejected. Entrypoints name the app
executable only; they do not authorize shell or installer hooks.

`requested_capabilities` has at most 32 unique v1 capability literals.
`resources` has at most 16 unique `{name, requested}` entries, where name is a
bounded identifier and requested is a nonnegative integer no larger than the
per-resource ceiling. Requests are not effective limits or grants.
`autostart_requested` and `restart_requested` are booleans and convey no
authority. `ui`, when present, is `{entrypoint, max_message_bytes}` where the
entrypoint is a package-relative path and the message ceiling is 1–16,384.

Manifest v1 has no installer hooks, arbitrary command, remote/localhost UI
URL, effective grant, firewall policy, router credential, or administrator
credential. Future signature formats must authenticate retained raw canonical
manifest bytes or the exact manifest representation without reinterpreting
these semantics. Package archive and signature formats are unspecified.

## 6. UI messages and network policy

UI messages carry UTF-8 text containing one valid JSON value, no larger than
16,384 bytes.
Descriptors only identify package-relative static resources. A future console
host treats UI as the app principal: no admin token, console DOM authority, or
independent direct network access.

Direct networking defaults to deny, including DNS/resolution, public, private,
loopback, link-local, multicast, and unspecified addresses. I2P router access
is represented by separate capabilities. The policy vocabulary supports TCP
(UDP is reserved and unsupported), exact lowercase ASCII DNS hostname, exact IP, or CIDR selector,
and a single port or inclusive port range. Rules are administrator-owned;
deny takes precedence. A hostname request must pass hostname policy and then a
second pure check of every resolved IP address and address scope. Hostname
permission never implies permission for a forbidden resolved scope. IPv4
private includes RFC1918; IPv6 private includes ULA. IPv4-mapped IPv6 is
classified by its mapped IPv4 address. Loopback and private/LAN scopes require
an explicit administrator allow rule; no scope is implicitly safe.

The launch profile is either `Secured` or explicit operator-selected
`UnsafeDirect`. `UnsafeDirect` is a distinct profile, never a partial secured
attestation and never app-selectable.

## 7. Sandbox, resources, and ceilings

`Secured` requires all of: direct network denied, loopback denied, private
filesystem boundary, host process inspection denied or contained, child
process tree contained, resource limits installed, sanitized environment, and
inherited broker channel installed. Attestation includes backend kind/version,
evidence generation, and the asserted property set. Missing any required
property fails validation. No v1 value claims that an OS backend exists.

Resource requests are bounded hints; a future host clamps them to operator
ceilings or denies launch. They cannot raise limits. Fixed ceilings:

| Item | Ceiling |
| --- | ---: |
| identifier bytes | 64 |
| manifest bytes | 65,536 |
| manifest top-level fields | 16 |
| manifest entrypoints / resources | 8 / 16 |
| capabilities / network rules | 32 / 64 |
| frame payload / control JSON | 65,536 / 16,384 bytes |
| live streams / in-flight requests | 128 / 64 |
| UI message / diagnostic string | 16,384 / 1,024 bytes |
| sandbox properties / resource entries | 16 / 16 |
| resource request value | 1,099,511,627,776 |
| network rules' port range | 1–65,535 |

Implementations may use lower operational limits but not higher wire ceilings.

## 8. Non-guarantees and ownership

This contract does not launch or contain a process, open a socket, resolve a
name, serve UI, install packages, adapt SAM/I2CP/Proposal 170, or enforce a
firewall. It does not establish application capability, anonymity, privacy,
or clearnet safety. Unauthorized egress containment cannot prevent an app from
encoding identifying data in traffic it is allowed to send. Proposal 170
binding is a successor-plan interface dependency, not a Plan 345 capability.
