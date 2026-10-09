# Managed application local policy v1

Status: experimental, local-only, disabled by default. This reference defines
the persistent policy consumed by Plan 374; it is not a protocol support claim.

## State root and generations

The daemon derives the state root as `<router.data_dir>/managed-apps`, resolves
relative data directories against startup cwd, creates a private directory,
canonicalizes it, clears the environment of the sibling `i2pr-appd`, and binds
only `I2PR_APP_STATE_ROOT` to that absolute path. `i2pr-appctl --data-dir` uses
the same derivation. Appd still requires the inherited anonymous manager
transport; a standalone process has no router authority.

Policy lives outside installed package trees:

```text
managed-apps/
  packages/...
  app-data/<publisher-fingerprint-digest>/<app-id>/
  policy/
    generations/<20-digit-generation>/state.json
    .staging/<transaction>/state.json
  runtime.lock
  admin.lock
```

Schema version 1 stores a monotonic nonzero `u64` generation, sorted unique
publisher fingerprints, and sorted unique records keyed by publisher
fingerprint plus `AppId`. Unknown or duplicate JSON fields, unsupported schema,
noncanonical collections, malformed highest committed generation, and
generation mismatch fail closed. The reader never falls back to a lower
generation. `u64::MAX` is terminal and requires a future migration. Staging is
non-authoritative and ignored by readers. Successful mutations sync a staged
directory, atomically rename it into the committed generation namespace, then
retain at least the newest two generations.

## Policy semantics

- Publisher trust is an explicit local decision for the exact lowercase
  SHA-256 Ed25519 key fingerprint. Signature verification alone never trusts.
- Grants bind to publisher fingerprint plus `AppId`; only `Sam` and `I2cp` are
  supported. A grant may be added only when the exact selected manifest
  requests it. Manifest requests never become grants.
- Selection is an exact package identity: publisher fingerprint, `AppId`,
  version identifier, and artifact digest. Installation does not select or
  trust. Version identifiers have no ordering; rollback/update selection is
  explicit.
- Untrust removes grants, profile, and autostart for the publisher so
  retrusting cannot resurrect launch authority.
- Autostart requires a trusted publisher, exact selection, and explicit
  profile. Manifest autostart/restart flags are advisory only.
- `UnsafeDirect` is ordinary host networking without a sandbox and requires
  `i2pr-appctl --allow-direct-host-network`. `Secured` is enforced on qualified
  Linux x86_64/aarch64 hosts for static ELF applications; unsupported hosts and
  incomplete setup refuse before exec.
- Linux Secured launches receive a stable private data directory keyed by the
  trusted publisher identity and AppId. It is outside `packages/` and `policy/`,
  survives package replacement and manager restart, and is not removed by
  package removal.
- For Linux Secured launches, nonzero `memory_bytes` and `open_files` are hard
  address-space and descriptor ceilings. Other profiles do not claim these
  limits are enforced.

## Administration and runtime

`i2pr-appctl` operates offline through filesystem access to the state root.
Every mutation takes the nonblocking exclusive runtime lock, then the shared
admin transaction lock, validates and writes a new generation, and releases
locks in reverse order. Mutations fail without changing policy while appd is
running. Changes become active only after app-runtime/router restart.

Appd acquires and holds the runtime lock before loading the highest valid
snapshot. It enumerates autostart records in canonical order, re-verifies each
selected immutable package, checks trust/target/grants/profile, and creates
fresh nonzero instance IDs with the OS RNG. A bad app does not block siblings.
Application exit is terminal for that app in the current manager lifetime;
there is no crash auto-restart. No live administrator endpoint exists.

## Operator sequence

```text
stop app runtime/router
i2pr-appctl package verify <file.i2prapp>
i2pr-appctl package install <file.i2prapp>
i2pr-appctl publisher trust <fingerprint>
i2pr-appctl app select <fingerprint> <app-id> <exact-version>
i2pr-appctl app grant <fingerprint> <app-id> sam|i2cp
i2pr-appctl app profile <fingerprint> <app-id> unsafe-direct
i2pr-appctl app autostart <fingerprint> <app-id> on
start router
```

Package verification identifies the signing key only. Operators must assess and
trust it separately. `unsafe-direct` explicitly permits ordinary host networking
and does not imply containment.
