# `i2pr-app-proto` — managed native application contract

`i2pr-app-proto` (Plan 345 foundation, corrected by managed-runtime Plan 349)
owns validated, runtime-neutral values for a future managed native-app
boundary. It is a leaf contract crate with no production `i2pr-*` dependency.
It owns no sockets, process launching,
filesystem access, DNS, Tokio, sandbox backend, or live policy owner.

## Public surface

The crate exposes bounded app/publisher/version identifiers, opaque launch,
request, and resource IDs, app principals, requested/granted/effective
capability types, four direction-specific control enums and decoders, a role
handshake, a 12-byte frame envelope, strict bounded JSON control/manifest
decoders, package-relative resource paths, pure default-deny TCP policy
evaluation, sandbox requirement attestation validation, and resource ceilings.
`PrincipalOwnedResource` always pairs an opaque resource handle with its app
principal. The language-neutral authority is
[`managed-native-app-runtime-v1.md`](../../specs/references/managed-native-app-runtime-v1.md),
not serde tags or Rust layout.

`RequestedCapability` is untrusted inert input. `GrantedCapability` can only
be made through an explicitly administrator-authorized constructor, and
`EffectiveCapabilities` can only be projected from grants. `brokered_tcp` is
requestable but cannot be granted or opened as a v1 service. Every v1
administrator operation is structurally represented only as a reserved
request and receives a typed `unsupported_operation` response; it has no
side-effect owner. The four decoders are app-to-host, host-to-app,
admin-to-host, and host-to-admin; cross-direction and cross-role inputs fail
closed. Request/reply IDs are nonzero and session accounting rejects duplicate,
unknown, completed, and over-limit IDs. Proposal 170 adaptation is a future
interface dependency and is not implemented here.

The app `hello` declares an app and instance identity but proves neither; a
future trusted transport owner must bind it to an authenticated process/IPC
principal. Host-side stream-close/reset, capability, and health notices use
host-to-app variants and cannot be parsed as app requests or replies.

## Security and limitations

The pure policy defaults to deny. A hostname allow authorizes a globally
routable resolution by itself, but a matching IP/CIDR deny still wins and every
non-global resolution requires an explicit IP/CIDR allow. IPv4/IPv6 special
purpose ranges are frozen in the language-neutral reference against the IANA
registries retrieved 2026-10-05. IPv4-mapped IPv6 inherits the mapped IPv4
classification. Plan 352 freezes the corresponding policy identity rule:
mapped targets canonicalize to IPv4 before exact/CIDR matching, while mapped
IPv6 exact and CIDR policy selectors are invalid. Scope and rule evaluation
therefore make the same decision for mapped and ordinary IPv4 forms. The
evaluator performs neither DNS nor networking.
The `Secured` attestation checks that every required property is asserted; it
does not verify an operating-system backend. This crate does not establish a
usable application capability, containment, anonymity, or privacy guarantee.

## Validation

Tests cover strict IDs and paths, all four message directions and the two
session roles, request/reply correlation and typed errors, reserved admin
operations, brokered-TCP rejection, handshake/frame golden bytes and
truncation/oversize cases, strict JSON fields and duplicate keys, special
purpose IPv4/IPv6 boundaries, hostname/post-resolution policy and deny
precedence, and required sandbox attestation properties. Fuzz smoke covers all
four control decoders. The runtime-boundary checker includes an active positive
control.
