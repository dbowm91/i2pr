# ADR 0037: Persistent managed-app policy and production catalog

Status: **Accepted**

Date: 2026-10-07

## Context

Plan 382 defines signed package identity and an immutable local package store.
The production manager still needs a durable operator decision before a verified
package may receive launch authority. This decision must survive restarts while
remaining unavailable to package parsers, the router protocol, and a live
network administrator endpoint.

## Decision

1. Local filesystem access to `<data-dir>/managed-apps` is the v1 offline
   administrator boundary. The daemon resolves, creates, and canonicalizes
   that root, clears the child environment, and supplies only
   `I2PR_APP_STATE_ROOT` to `i2pr-appd`.
2. Publisher trust is an explicit decision keyed by the exact lowercase
   SHA-256 fingerprint of the Ed25519 publisher key. A valid signature proves
   key possession; it does not imply trust or permission.
3. Grants bind to `(publisher fingerprint, AppId)`, not display name or
   package version. Only `Sam` and `I2cp` are grantable in this milestone.
   Manifest capability, autostart, and restart fields are requests only.
4. Selection stores the exact Plan-382 package identity, including artifact
   digest. Install never selects or trusts. Updates and rollbacks require an
   explicit selection; version strings are not ordered.
5. Untrust clears grants, launch profile, and autostart so retrusting cannot
   restore latent launch authority.
6. `UnsafeDirect` requires a separate explicit acknowledgement and provides
   ordinary host networking without a sandbox. `Secured` may be persisted as
   desired policy but is refused before exec until a qualified sandbox plan
   replaces that gate.
7. Policy mutations are offline and restart-applied. `i2pr-appd` holds an
   exclusive runtime lock for its lifetime and uses one validated policy
   snapshot. Application exit is terminal for that manager lifetime; no crash
   restart policy is implied.
8. Policy generations are immutable directories selected by the numerically
   highest committed generation. A malformed highest generation fails closed
   rather than rolling back. Mutations atomically rename a fully synced staged
   generation and retain at least the newest two.
9. `i2pr-app-state` and `i2pr-app-package` cannot construct
   `LaunchAuthority`. Only the trusted `i2pr-appd` production catalog crosses
   from a verified package and validated local policy to launch authority.
10. Future live administration, repository/TUF updates, and OS sandbox
    backends must consume this authority model rather than creating parallel
    trust or policy state.

## Consequences

- An installed and signed package remains inert until an operator trusts its
  publisher, selects the exact artifact, chooses a profile, grants any
  requested capability, and optionally enables autostart.
- Policy edits are rejected while the app runtime owns its lock. They take
  effect after the router/app runtime restarts.
- Resource ceilings are descriptive only until an OS-specific backend enforces
  them. There is no containment claim.
- This decision does not promote managed apps, SAM, I2CP, or any protocol in
  `specs/support.toml`.

## References

- [Managed-app package v1](../../specs/references/managed-app-package-v1.md)
- [Managed-app policy v1](../../specs/references/managed-app-policy-v1.md)
- Plans 382 and 383
