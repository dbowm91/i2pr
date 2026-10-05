# Managed native application contract v1

Status: Plan 345 froze the initial unreleased draft; Plan 349 corrects that
draft before any runtime owner or external consumer exists. The v1 contract is
still pre-release and has never been shipped. No downstream consumer may rely
on the superseded Plan-345 message shapes or policy algorithm. This document
is the language-neutral contract; Rust enum tags, memory layout, and serializer
defaults are not wire ABI. The implementation remains vocabulary and pure
validation only. It does not provide an app runtime, transport, sandbox, DNS
resolver, or network broker.

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

## 3. Directional control vocabulary

Every control object has a required string `type`. Direction is represented by
four disjoint decoders; a message from one direction is never accepted by
another direction's decoder. The connection handshake selects the application
or administrator role. It does not select message direction or authorize an
operation.

**Application → host** messages:

- `hello` `{type, request_id, app_id, instance_id, protocol_major, protocol_minor}`
- `open` `{type, request_id, stream_id, service}`
- `permission_request` `{type, request_id, capabilities[]}`
- `close` `{type, stream_id}`
- `reset` `{type, stream_id, reason}`
- `ui_message` `{type, message_id, payload}`

**Host → application** messages:

- `reply` `{type, request_id, outcome}` for a `hello` or `open` request
- `permission_reply` `{type, request_id, status}`
- `stream_closed` `{type, stream_id}`
- `stream_reset` `{type, stream_id, reason}`
- `capabilities` `{type, capabilities[]}` — effective, administrator-granted
  capabilities only
- `health` `{type, state, detail?}` — unsolicited host event, never a reply

`service` is one of `sam`, `i2cp`, or `control_scoped`. `brokered_tcp` is not
an openable v1 service. `status` is one of `pending`, `denied`, or `recorded`;
it is not a grant. Application reply `outcome` is either
`{"outcome":"succeeded"}` or
`{"outcome":"failed","error":{"code":<error-code>,"diagnostic":<string-or-null>}}`.

**Administrator → host** has one structurally representable request:

- `reserved_request` `{type, request_id, operation}`

`operation` is a closed enum covering `install`, `update`, `uninstall`,
`launch`, `stop`, `grant`, `revoke`, `network_policy`, `launch_profile`,
`resource_policy`, and `inspect`. All of these operations are reserved and
unsupported in this pre-runtime contract. No package or AppManager semantics
are implied.

**Host → administrator** messages:

- `reply` `{type, request_id, error}`; reserved administrator operations MUST
  receive the typed `unsupported_operation` error and MUST have no side effect.

Every request ID is an unsigned, nonzero 32-bit integer. It is unique among at
most 64 active requests in its session. A response echoes exactly one active
request ID; an unknown, completed, or duplicate ID is rejected. Requests that
expect completion cannot be fire-and-forget. Error codes are a closed enum:
`unsupported_operation`, `permission_denied`, `invalid_request`,
`resource_limit`, `conflict`, `not_found`, and `internal`. Optional diagnostic
text is at most 1,024 UTF-8 bytes and is never a machine decision key. Events
are distinct message types and cannot be decoded as replies.

`hello` declares the app and instance identity for the application session; it
is not authentication proof. A future trusted transport owner must bind that
claim to its authenticated process/IPC principal before authorizing access.

The Plan-345 `hello` without a request ID, `accept`, `permission_status`, and
directionless role enums are not accepted aliases. Version 1 remains at
major/minor `1.0` because Plan 345's form was explicitly an unreleased draft
and no consumer can observe it. This is a pre-release correction, not a wire
migration.

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
or application messages. `brokered_tcp` is reserved/requestable only: it
cannot be granted or appear in effective capabilities until a later broker
plan defines its connect transaction. Any other conversion from requested to
granted requires a future explicit administrator owner. Capability scope is
associated with an `AppPrincipal` and never implies a general Proposal 170
credential.

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
(UDP is reserved and unsupported), exact lowercase ASCII DNS hostname, exact
IP, or CIDR selector, and a single port or inclusive port range. Rules are
administrator-owned; deny takes precedence.

For a hostname request, an exact hostname allow rule authorizes the named
service. Each resolved address is still classified independently. A globally
routable address may pass on hostname authorization alone unless a matching
IP/CIDR deny exists. Every non-global address remains denied unless a matching
explicit IP/CIDR allow exists; any matching deny wins. The pure policy does not
resolve names, choose among answers, or race connections. A future broker must
apply this decision to each address it selects.

For broker policy, "global" means ordinary public unicast space outside the
special-purpose and non-global ranges listed below. Every address outside the
IPv4 ordinary-unicast space and outside IPv6 `2000::/3` is non-global. The
classification is frozen against the IANA IPv4 and IPv6 Special-Purpose
Address Registries as retrieved 2026-10-05; special-purpose blocks are
conservatively non-global even when IANA marks a particular block globally
reachable. IPv4-mapped IPv6 inherits the mapped IPv4 classification. New or
unrecognized address families/classification inputs fail closed.

IPv4 non-global blocks: `0.0.0.0/8`, `10.0.0.0/8`, `100.64.0.0/10`,
`127.0.0.0/8`, `169.254.0.0/16`, `172.16.0.0/12`, `192.0.0.0/24`,
`192.0.2.0/24`, `192.31.196.0/24`, `192.52.193.0/24`, `192.88.99.0/24`,
`192.168.0.0/16`, `192.175.48.0/24`, `198.18.0.0/15`, `198.51.100.0/24`,
`203.0.113.0/24`, `224.0.0.0/4`, and `240.0.0.0/4` (including limited
broadcast). The first and last addresses of every listed prefix are covered.

IPv6 non-global blocks include `::/128`, `::1/128`, `::ffff:0:0/96`,
`64:ff9b::/96`, `64:ff9b:1::/48`, `100::/64`, `100:0:0:1::/64`,
`2001::/23` (including `2001:2::/48` benchmarking), `2001:db8::/32`,
`2002::/16`, `3fff::/20` (documentation), `5f00::/16`, `fc00::/7`,
`fe80::/10`, and `ff00::/8`. In addition, all IPv6 outside `2000::/3` is
non-global. The overlap in this list is intentional; tests exercise the
most-specific named boundary and its adjacent controls where meaningful.

Registry provenance: [IANA IPv4 Special-Purpose Address Registry](https://www.iana.org/assignments/iana-ipv4-special-registry)
and [IANA IPv6 Special-Purpose Address Registry](https://www.iana.org/assignments/iana-ipv6-special-registry).

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
