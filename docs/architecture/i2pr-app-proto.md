# `i2pr-app-proto` — managed native application contract

`i2pr-app-proto` (Plan 345) owns validated, runtime-neutral values for a
future managed native-app boundary. It is a leaf contract crate with no
production `i2pr-*` dependency. It owns no sockets, process launching,
filesystem access, DNS, Tokio, sandbox backend, or live policy owner.

## Public surface

The crate exposes bounded app/publisher/version identifiers, opaque launch and
resource IDs, app principals, requested/granted/effective capability types,
application and administrator message enums, a role handshake, a 12-byte
frame envelope, strict bounded JSON control/manifest decoders, package-relative
resource paths, pure default-deny TCP policy evaluation, sandbox requirement
attestation validation, and resource ceilings. `PrincipalOwnedResource`
always pairs an opaque resource handle with its app principal. The language-neutral authority
is [`managed-native-app-runtime-v1.md`](../../specs/references/managed-native-app-runtime-v1.md),
not serde tags or Rust layout.

`RequestedCapability` is untrusted inert input. `GrantedCapability` can only
be made through an explicitly administrator-authorized constructor, and
`EffectiveCapabilities` can only be projected from grants. The app message
enum has no administrator mutation variants. Proposal 170 adaptation is a
future interface dependency and is not implemented here.

## Security and limitations

The pure policy defaults to deny and separates hostname authorization from
post-resolution address authorization. It performs neither DNS nor networking.
The `Secured` attestation checks that every required property is asserted; it
does not verify an operating-system backend. This crate does not establish a
usable application capability, containment, anonymity, or privacy guarantee.

## Validation

Tests cover strict IDs and paths, app/admin role separation, administrator
grant origin, handshake/frame golden bytes and truncation/oversize cases,
strict JSON fields and duplicate keys, default-deny address scopes,
post-resolution checks, and required sandbox attestation properties. The
runtime-boundary checker includes an active positive control.
