# Plan 385 — Managed-App Private Persistent Data and Launch Workspace

Status: **ready — managed-native-app-runtime continuation**

Date: 2026-10-09

Roadmap: `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:

- Managed native app runtime/369 closed — trusted `i2pr-appd` / `i2pr-apphost` lifecycle.
- Plans 382–383 closed — signed immutable packages, persistent local policy, exact launch catalog.

Primary consumers:

- `dbowm91/i2pr-tc` M004 managed integration.
- future mail/IRC managed applications.

Classification: infrastructure + persistence/lifecycle invariant.

## 1. Objective

Give every managed application a host-owned persistent private data root and
ephemeral launch workspace without exposing the router's managed-app state root,
package-policy files, sibling application state, or arbitrary host paths.

The persistent identity is publisher + AppId, not package version and not
launch-instance id, so application state and long-lived I2P Destination keys can
survive upgrades and restarts.

This plan establishes filesystem ownership and launch plumbing only. A later
Secured sandbox plan enforces the filesystem boundary against hostile code.

## 2. Baseline

Plan 383 currently gives `i2pr-appd` the canonical
`<router.data_dir>/managed-apps` root through `I2PR_APP_STATE_ROOT`. That
binding is manager authority and must never be forwarded to an application.

The production catalog can select and verify an exact package, but applications
have no canonical writable root. Consumers therefore cannot persist resume
state, application databases, or Destination private key material without
inventing host paths.

## 3. Required ownership model

Host-side layout is owned by trusted runtime code and must be path-safe and
collision-free, conceptually:

```text
managed-apps/
  packages/...
  policy/...
  app-data/
    <publisher-fingerprint>/
      <app-id>/
        data/        persistent
        cache/       persistent but discardable
        run/
          <instance-id>/   ephemeral launch workspace
```

Exact on-disk spelling may differ, but these ownership domains may not.

The application receives only its own canonical `data`, `cache`, and
instance `run` paths. It never receives `I2PR_APP_STATE_ROOT`, package-store
administration paths, policy-generation paths, or sibling roots.

## 4. Application-facing contract

After `env_clear`, `i2pr-apphost` may supply exactly named runtime bindings:

- `I2PR_APP_DATA_DIR`;
- `I2PR_APP_CACHE_DIR`;
- `I2PR_APP_RUNTIME_DIR`.

They are convenience paths, not authorization tokens.

For `UnsafeDirect`, the application already has ordinary host filesystem
authority and these paths make no containment claim.

For `Secured`, the future sandbox backend must make the same paths the writable
filesystem allow-set and deny traversal outside it. Plan 385 must therefore keep
the paths stable enough to become sandbox inputs without changing the app
contract.

Package resources remain immutable and separately owned.

## 5. Creation and permissions

Trusted runtime code must:

1. derive the application data identity from validated publisher fingerprint +
   AppId;
2. reject path separators, alternate encodings, traversal, aliases, and
   noncanonical identities before filesystem construction;
3. create each directory with owner-private permissions where supported;
4. canonicalize/verify components without following an attacker-controlled
   symlink out of the managed root;
5. create the instance runtime directory before exec;
6. remove the instance runtime directory on clean or failed launch teardown;
7. leave persistent data/cache untouched by package uninstall unless a future
   explicit data-removal operation is authorized.

No package archive path participates in writable-root derivation.

## 6. Secret-storage contract

The persistent data root is suitable for application-owned secrets such as an
I2P Destination private key, but Plan 385 does not interpret those bytes.

Required properties:

- secret files are application-owned, not router identity material;
- package update/rollback does not copy or replace data;
- a different publisher using the same AppId receives a different root;
- untrust/disable does not silently delete data;
- an application cannot name another app's root through launch metadata;
- backup/export is future administrator functionality, not an app capability.

## 7. Capacity and quotas

Do not claim a disk quota in this milestone unless it is actually enforced by
the selected platform.

The policy may retain descriptive resource vocabulary, but Plan 385 must not
convert an unenforced storage request into a security claim. A later explicit
storage-quota plan may add one.

The runtime must still surface ordinary filesystem exhaustion as typed launch or
application I/O failure and must not fall back to an arbitrary writable path.

## 8. Concurrency, restart, and failure

- Two launch instances for one persistent application share `data`/`cache`
  only if policy explicitly permits concurrent instances; current single-launch
  policy should keep this impossible by construction.
- Instance `run` roots are unique and nonzero-instance keyed.
- Manager restart reuses persistent roots and creates a fresh runtime root.
- Failed exec removes only the failed runtime root.
- Cleanup failure is reported and retried within a bounded policy; it never
  deletes persistent data as compensation.
- State-root lock and package/policy transactions remain separate from
  application data I/O; the app must not hold the administrator lock.

## 9. Work packages

### WP1 — freeze path/identity contract

Add the durable ADR or reference amendment for application data identity,
lifetime, environment names, and delete/non-delete semantics.

### WP2 — trusted data-root owner

Implement validated per-application root creation in a runtime-neutral helper
crate or the existing app-state/runtime boundary without allowing applications
to construct arbitrary host paths.

### WP3 — appd/apphost launch plumbing

Pass only the three application paths through the trusted launch request and
sanitized environment. Never forward the manager state root.

### WP4 — lifecycle cleanup

Own per-instance runtime directories across exec failure, normal exit,
manager shutdown, and router restart.

### WP5 — qualification and guards

Prove cross-publisher/AppId separation, symlink/traversal refusal, restart
persistence, runtime cleanup, and absence of manager/policy-root leakage.

## 10. Verification

Run the ordinary repository floor plus focused package/policy/process boundary
guards. Add negative tests for:

- same AppId, different publisher;
- same publisher/AppId, different package version;
- traversal and symlink substitution;
- sibling root read/write attempts at the trusted path-construction seam;
- manager restart with unchanged persistent state;
- failed launch and stale runtime-root cleanup;
- malformed/overlong identity;
- app environment contains no `I2PR_APP_STATE_ROOT`.

## 11. Acceptance criteria

Plan 385 closes when:

1. every approved launch receives exactly one persistent private data root,
   cache root, and instance runtime root;
2. roots are keyed by publisher + AppId and survive version/instance changes;
3. app-visible environment contains no manager state/policy/store authority;
4. path construction cannot escape the managed root through input or symlink
   substitution;
5. restart preserves persistent data and removes/replaces only runtime state;
6. package removal/update does not implicitly delete application data;
7. the contract is consumable by a future Secured sandbox backend;
8. full focused and routine verification passes.

## 12. Stop conditions

Stop rather than weaken the boundary if a portable path API requires exposing
the full managed-app state root, if application-controlled package paths can
influence writable-root identity, or if safe canonical creation cannot be
implemented without a lower-level filesystem corrective.

## 13. Closure evidence

Create `plans/closure/managed-native-app-runtime/385-status.md` with
implementation SHAs, exact layout/identity contract, permission and
symlink/traversal negatives, restart/cleanup matrix, environment evidence,
routine-floor results, residual findings, and the unblock decision for the
Secured sandbox plan and downstream managed applications.
