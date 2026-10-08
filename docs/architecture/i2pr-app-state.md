# `i2pr-app-state` — persistent managed-app policy

Path: `crates/i2pr-app-state/`. Plan 383 owns strict local policy generations,
offline mutations, and verified launch decisions. It cannot construct
`i2pr-appd::LaunchAuthority` and has no router protocol or process capability.

## Public contract

`AppStateStore::open` binds to an injected managed-app root. `load` chooses the
numerically highest committed generation and fails closed on malformed or
unsupported state. `mutate` takes the nonblocking runtime lock followed by the
admin transaction lock, validates a complete new snapshot, syncs it in a
private staging directory, atomically renames it, and retains the newest two
generations. A staging directory is never authoritative. Generation exhaustion
at `u64::MAX` is typed.

`resolve` takes one immutable state snapshot and exact app record. It confirms
publisher trust and selected package identity, re-verifies the installed
signature and payload inventory, resolves exactly one platform target, and
intersects explicit grants with requests and the implemented Sam/I2cp set.
Unknown requested resources and unavailable Secured policy fail closed.
Resource ceilings are descriptive only.

## Locking

Appd holds `runtime.lock` exclusively for its lifetime. Offline mutations take
that same lock nonblocking before `admin.lock`, so they fail without changing
state while the production catalog is active. Package install/remove follows
the same runtime-then-package-admin order. Read-only inspection uses immutable
committed state.

## Dependencies and tests

Workspace dependencies are `i2pr-app-package` and `i2pr-app-proto`. Tests cover
generation retention, malformed-highest failure, strict schema rejection,
staging exclusion, generation exhaustion, canonical sorted policy, and runtime
lock exclusion. CLI tests cover trust/untrust persistence, runtime-lock refusal,
and the explicit UnsafeDirect acknowledgement gate.
`scripts/check-managed-app-policy-boundary.py` verifies the
authority/dependency seams and uses negative source mutations.
