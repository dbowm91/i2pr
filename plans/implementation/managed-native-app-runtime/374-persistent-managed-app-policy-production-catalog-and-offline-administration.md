# Plan 374 — persistent managed-app policy, production launch catalog, and offline administration

Status: **registered-managed-app-persistent-policy-production-catalog-and-offline-administration**.

Classification: **invariant + capability + persistence/lifecycle**.

Roadmap: plans/subsystems/managed-native-app-runtime-roadmap.md

Hard dependency: Plan 373 must close the signed immutable package/store foundation.

Interface dependencies:
- Plans 369–371;
- Plan 373 package identity/store API;
- ADR 0032 and ADR 0035.

## Objective

Turn the verified local package store into the first restart-safe production launch authority without introducing a network administrator service.

Plan 374 adds:
1. a persistent local administrator-policy store;
2. publisher trust keyed by exact Ed25519 fingerprint;
3. persistent per-app capability grants and exact selected package identity;
4. explicit launch-profile and autostart policy;
5. a production LaunchCatalog backed only by verified installed packages plus administrator policy;
6. fresh launch-instance ids from the OS RNG;
7. deterministic autostart after router/app-manager restart;
8. a separate offline i2pr-appctl administrator CLI for package/policy mutation;
9. an exact daemon-owned application-state root handed to i2pr-appd after env_clear;
10. re-verification of every selected package before launch authority is constructed.

This milestone still makes no secured-sandbox claim. Secured remains unlaunchable until a later qualified backend lands. UnsafeDirect is accepted only after a conspicuous explicit administrator opt-in.

## Why ready after Plan 373

Plan 369 deliberately ships i2pr-appd with EmptyCatalog. Plan 373 provides a cryptographically attributable immutable package store but still no trust/grant decision. Plan 374 owns that missing administrator decision layer.

The package parser remains incapable of granting authority, while the policy layer never parses unverified package bytes directly.

## Research decisions

### Signature is identity, not policy

Publisher-key trust is a separate administrator decision. Grants do not derive from signatures, selecting a package does not grant requested capabilities, and changing publisher key creates a new security principal even when AppId is identical.

### No semantic-version ordering

AppVersion is an identifier, not a semver type. Selection is always an exact Plan-373 package identity: publisher fingerprint + AppId + AppVersion + artifact SHA-256.

Update or rollback is an explicit operator selection. Install never auto-selects. This avoids silent rollback semantics until a future repository/TUF plan owns monotonic update metadata.

### Administration stays offline in v1

The managed-app administrator vocabulary remains reserved. Promoting it now would add authentication, concurrency and live mutation races to the persistence milestone.

Plan 374 instead ships i2pr-appctl. Local filesystem access to the managed-app root is the administrator authority. Mutating operations refuse while i2pr-appd owns the runtime lock; changes become active on the next app-runtime/router start.

## ADR required

Add ADR 0037, or the next free ADR at implementation start, freezing:
- local filesystem access to the managed-app state root as the v1 offline administrator boundary;
- publisher trust as exact Ed25519 fingerprint trust;
- grants bind PublisherId + AppId, never display name or version;
- selected package identity is exact and includes artifact digest;
- install does not select or trust;
- update/rollback are explicit selections, never inferred from version ordering;
- untrust clears launch-enabling policy;
- manifest capability/autostart/restart fields are requests only;
- Plan-374 grantable capabilities are only Sam and I2cp;
- UnsafeDirect requires explicit risk acknowledgement;
- Secured may be stored as desired policy but still fails before exec;
- mutations are offline/restart-applied;
- application crash auto-restart remains unimplemented;
- future live AppManager and TUF work must consume this authority rather than create parallel state.

## New persistent-policy owner

Add workspace crate i2pr-app-state.

It owns:
- package-store discovery through Plan 373;
- strict persistent policy schema;
- generation transactions and recovery;
- publisher/app policy evaluation;
- target entrypoint resolution;
- conversion from stored policy + verified package to a validated launch decision.

It must not construct i2pr-appd::LaunchAuthority and must not depend on i2pr-appd.

Dependency direction:

    i2pr-appctl -> i2pr-app-state -> i2pr-app-package
                         |                  |
                         +---- i2pr-app-proto

    i2pr-appd -> i2pr-app-state
       |
       +---- i2pr-app-manager-proto

No app-state/package/admin crate may depend on daemon, runtime, SAM/I2CP, control, NetDB, tunnel, or transport owners.

## Managed application root

Canonical root:

    <router.data_dir>/managed-apps

It is not independently configurable.

When app_runtime.enabled is true, the daemon:
1. resolves router.data_dir against startup cwd if relative;
2. creates managed-apps with owner-private permissions where supported;
3. canonicalizes it;
4. launches the sibling i2pr-appd with env_clear();
5. sets exactly one managed-app environment binding:

    I2PR_APP_STATE_ROOT=<absolute canonical root>

plus only platform variables proven necessary for child startup.

This path is trusted daemon composition, not application authority. A standalone appd still lacks the inherited daemon capability and cannot launch against the router.

The process-boundary checker must require env_clear, require the exact state-root binding, forbid arbitrary environment forwarding, and retain the no-config/no-PATH/no-shell manager rules.

## Layout

Plan 373 owns packages/, .staging/, and admin.lock.

Plan 374 adds:

    managed-apps/
      packages/...
      policy/
        generations/
          00000000000000000001/
            state.json
        .staging/
      runtime.lock
      admin.lock

No mutable policy file lives inside an installed package directory.

## Policy schema v1

state.json contains:
- schema_version = 1;
- non-zero monotonic generation: u64;
- sorted unique trusted publisher fingerprints;
- sorted unique app-policy records keyed by publisher_id + app_id.

One app record contains:
- selected: Option<PackageIdentity>;
- granted_capabilities: Vec<Capability>;
- launch_profile: Option<LaunchProfile>;
- autostart: bool;
- max_connections: u32;
- resource_policy with bounded descriptive memory/open-file ceilings.

Rules:
- unknown and duplicate fields reject;
- collections are bounded and canonically sorted;
- only Sam and I2cp are grantable;
- BrokeredTcp, ControlScoped, UiBridge, Health and Lifecycle cannot be persisted as effective grants in this milestone;
- grants may only be added when the selected verified manifest requests them;
- effective launch capabilities are persisted grants intersect selected manifest requests intersect current implemented grantable set;
- a new package version never gains a new capability merely because it requests one;
- autostart_requested and restart_requested are advisory only;
- restart_requested has no runtime effect;
- autostart requires selected package, trusted publisher and explicit launch profile;
- UnsafeDirect cannot be selected without explicit CLI risk acknowledgement;
- Secured can be persisted but launch resolution returns typed SecuredUnavailable until a later sandbox plan changes that exact gate.

## Policy transaction

A mutating i2pr-appctl command:
1. acquires runtime.lock exclusively/non-blocking; if appd holds it, fail without mutation;
2. acquires admin.lock;
3. reads and validates the highest committed generation;
4. validates every referenced package identity needed by the operation;
5. writes policy/.staging/<nonce>/state.json with generation + 1;
6. flushes/syncs;
7. atomically renames the generation directory to policy/generations/<20-digit generation>/;
8. after commit, prunes old generations while retaining at least the newest two valid generations;
9. releases locks in reverse order.

Readers ignore .staging.

There is no mutable HEAD pointer. Startup selects the numerically highest complete generation after checking that its directory name equals its embedded generation and the whole state validates.

A malformed highest committed generation fails closed; it does not silently roll back to an older state. u64::MAX is a typed terminal state requiring migration.

## Runtime lock

Production i2pr-appd opens runtime.lock and holds the exclusive lock for its full authenticated lifetime before reading policy or launching applications.

Consequences:
- offline mutations cannot race the production catalog;
- appd consumes one immutable policy snapshot per process lifetime;
- administrator changes take effect only after appd/router restart;
- read-only appctl list/inspect may read immutable committed state without mutation lock.

Plan 374 intentionally does not support live mutation.

## Administrator CLI

Add distribution binary i2pr-appctl.

It depends only on i2pr-app-state, i2pr-app-package, contract crates and CLI/filesystem support. It must not depend on i2pr-daemon, router runtime/protocol crates or network clients.

Canonical root argument:

    --data-dir <router data dir>

The CLI derives <data-dir>/managed-apps.

Required command surface:

    i2pr-appctl package verify <file.i2prapp>
    i2pr-appctl package install <file.i2prapp>
    i2pr-appctl package list
    i2pr-appctl package inspect <publisher> <app> <version>
    i2pr-appctl package remove <publisher> <app> <version>

    i2pr-appctl publisher trust <publisher-fingerprint>
    i2pr-appctl publisher untrust <publisher-fingerprint>

    i2pr-appctl app select <publisher> <app> <version>
    i2pr-appctl app grant <publisher> <app> <sam|i2cp>
    i2pr-appctl app revoke <publisher> <app> <sam|i2cp>
    i2pr-appctl app profile <publisher> <app> <secured|unsafe-direct>
    i2pr-appctl app autostart <publisher> <app> <on|off>
    i2pr-appctl app inspect <publisher> <app>

Setting unsafe-direct additionally requires:

    --allow-direct-host-network

Without it, fail with no state change.

package remove refuses the selected package. Application data is never deleted by package removal.

Untrust:
- removes publisher trust;
- clears grants, launch profile and autostart for every app under that publisher;
- may retain selected package references for inspection;
- does not delete packages/data.

Retrusting therefore never resurrects old authority.

## Target entrypoint selection

Use exact Rust-style target identifiers in manifest entrypoints[].target.

At minimum support target triples still present in the i2pr release matrix, expected to include:
- x86_64-unknown-linux-gnu;
- aarch64-unknown-linux-gnu;
- armv7-unknown-linux-gnueabihf when still supported;
- x86_64-apple-darwin;
- aarch64-apple-darwin;
- x86_64-pc-windows-msvc;
- aarch64-pc-windows-msvc when still supported.

Freeze the actual list from the current release matrix during WP1.

The running target is derived from compile-time cfg, never package input. Exactly one matching entrypoint is required. Zero or duplicate matches refuse launch. The selected path must remain present in the verified inventory and marked executable.

## Resource requests

Plan 374 recognizes exactly:
- memory_bytes;
- open_files.

Unknown resource names cause launch resolution to fail rather than being ignored.

The launch decision uses min(manifest request, administrator resource ceiling), or zero when absent. These values remain descriptive only until the sandbox/resource plan enforces them.

## Production launch catalog

Replace shipped EmptyCatalog only after trusted state root is established and runtime.lock is held.

Startup:
1. authenticate manager transport and complete handshake;
2. read I2PR_APP_STATE_ROOT supplied by daemon composition;
3. acquire runtime.lock;
4. load/validate highest policy generation;
5. enumerate autostart apps in stable publisher_id/app_id order;
6. resolve exact selected package;
7. re-run Plan-373 store/package verification;
8. confirm publisher trust;
9. select exact platform entrypoint;
10. derive safe capability intersection;
11. generate a fresh non-zero 128-bit instance id from rand_core::OsRng with os_rng enabled; retry only a small fixed count for zero/live collision, with no fallback on RNG failure;
12. assemble AuthorityRequest;
13. construct LaunchAuthority through administrator attribution from validated local policy;
14. launch once through the existing Plan-369 runtime.

Package/state crates cannot construct LaunchAuthority; only trusted appd composition crosses that seam.

## Administrator attribution

The existing AdministratorPrincipal::from_authenticated_session name describes a live administrator transport that Plan 374 does not have.

Add a separate non-serializable constructor/typed provenance for trusted local persisted policy, e.g. AdministratorPrincipal::from_trusted_local_policy(generation).

It is not itself authentication; it records the provenance of the already-validated local administrator decision and keeps GrantedCapability::from_administrator_policy as the one grant constructor.

Do not fake a session id.

## Autostart and restart semantics

- Manifest autostart_requested never enables autostart.
- Operator autostart=on launches once each time authenticated appd starts.
- Each launch gets a fresh AppInstanceId.
- One app failing verification/launch does not block siblings.
- Selected but non-autostart apps are valid installed state with no live launch path yet.
- Application exit is terminal for that app in that appd lifetime.
- restart_requested remains advisory and has no effect.
- No crash-loop/relaunch policy lands here.
- Manager restart reconstructs catalog from persisted policy and repeats only operator-approved autostarts.

Thus restart-safe means policy/selection/grants survive process/router restart and are deterministically re-applied, not that crashed apps are restarted.

## Invariants

1. Verification never grants trust.
2. Trust never grants capabilities.
3. Manifest requests never become grants automatically.
4. Selection never changes trust/grants.
5. New versions never gain new grants automatically.
6. Untrust cannot leave latent authority that reappears on retrust.
7. Same AppId under a different publisher fingerprint is a different principal.
8. Policy mutation cannot race a running production catalog.
9. Appd re-verifies selected package before authority construction.
10. Only Sam/I2cp can be effective Plan-374 grants.
11. UnsafeDirect always reflects explicit operator acknowledgement.
12. Secured remains fail-closed.
13. No live administrator endpoint/listener exists.
14. No automatic update/version ordering exists.
15. No app crash auto-restart exists.
16. Existing router/app-manager process boundaries remain intact.
17. No support advertisement changes.

## Scope

In scope:
- ADR 0037 or next free;
- i2pr-app-state;
- policy schema/generation store;
- runtime/admin locking;
- i2pr-appctl;
- trust/grant/select/profile/autostart mutations;
- exact package re-verification;
- target/resource resolution;
- fresh OS-random instance ids;
- persistent production LaunchCatalog;
- daemon-owned state-root env binding;
- restart/autostart black-box evidence;
- policy guards.

Out of scope:
- live admin IPC/API;
- remote access;
- TUF/Sigstore repository integration;
- automatic updates/version ordering;
- app crash auto-restart;
- app data deletion/migration;
- secured sandbox/resource enforcement;
- brokered clearnet;
- scoped Proposal 170;
- UI/SDK.

## Work packages

WP1 — ADR + policy schema freeze.
WP2 — persistent state crate and generation transaction.
WP3 — offline administrator CLI.
WP4 — production catalog and daemon/appd state-root composition.
WP5 — restart/autostart qualification through real SAM/I2CP.
WP6 — guards/docs/closure.

## Failure, restart and contention semantics

- No policy file: empty policy, no launches; first mutation creates generation 1.
- Malformed highest generation: app runtime degrades/fails closed, no rollback.
- Missing/tampered selected package: that app refuses; siblings continue.
- Untrusted publisher: no launch.
- Missing platform target/unsupported resource: no launch.
- OS RNG failure: no launch, no deterministic/time/PID fallback.
- runtime.lock busy during mutation: CLI fails without state change.
- policy crash before rename: prior generation remains authoritative.
- crash after rename: new generation authoritative.
- daemon/appd restart: reload highest generation and run approved autostarts once.
- app exit: no relaunch.
- no mutation retry may duplicate a state transition.

## Compatibility and migration

No previous persistent app-policy format exists.

schema_version 1 is strict. Unknown future schema fails closed.

No existing router behavior changes unless app_runtime.enabled is true and valid trusted/autostart state exists.

Plan 373 package format is unchanged. Managed-app and manager protocols remain unreleased.

## Required tests

Policy:
- empty state;
- strict schema/duplicate/unknown/sorted/bounds;
- generation monotonicity/exhaustion;
- malformed highest generation no rollback;
- crash-before/after commit;
- newest-two retention;
- mutation lock exclusion.

Trust/grants:
- signed untrusted package cannot launch;
- trust alone cannot launch;
- selection alone cannot launch;
- request without grant absent;
- grant not requested by selected package rejected;
- only Sam/I2cp grantable;
- new-version request does not auto-grant;
- different publisher same AppId inherits nothing;
- untrust clears authority and retrust does not resurrect it.

Profile:
- no profile no launch;
- Secured unavailable/no exec;
- UnsafeDirect without acknowledgement unchanged;
- UnsafeDirect with acknowledgement persists.

Selection/target:
- install does not select;
- exact selected artifact persisted;
- explicit older installed version may be selected;
- no implicit version ordering;
- zero/duplicate platform target refuses;
- entrypoint must be inventory executable.

Runtime:
- daemon passes only canonical managed-app root after env_clear;
- standalone appd cannot gain router authority;
- runtime lock blocks mutation;
- package reverified before authority;
- post-install tamper prevents launch;
- fresh instance id on every restart;
- RNG failure fails closed;
- deterministic multi-app autostart order;
- one bad app does not block sibling;
- persisted Sam/I2cp grants reach the existing Plan-369 path.

CLI:
- all required commands;
- selected package cannot be removed;
- mutations atomic;
- no private material or payload logging.

Structural:
- appctl/state own no network/router protocol capability;
- package/state cannot construct LaunchAuthority;
- only appd crosses state decision to LaunchAuthority;
- no live admin listener/API.

## Exact verification commands

    cargo fmt --all --check
    cargo check --locked -p i2pr-app-state -p i2pr-appctl -p i2pr-appd -p i2pr-daemon --all-targets
    cargo build --locked -p i2pr-appctl -p i2pr-appd -p i2pr-apphost -p i2pr-app-fixture
    cargo test --locked -p i2pr-app-state --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-appctl --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-appd --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-daemon app_runtime -- --test-threads=1
    cargo clippy --locked -p i2pr-app-state -p i2pr-appctl -p i2pr-appd -p i2pr-daemon --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-state -p i2pr-appctl -p i2pr-appd -p i2pr-daemon --no-deps
    bash scripts/check-dependency-direction.sh
    bash scripts/check-runtime-boundaries.sh
    python3 scripts/check-managed-app-package-boundary.py
    python3 scripts/check-managed-app-policy-boundary.py
    python3 scripts/check-managed-app-policy-boundary.py --self-test
    python3 scripts/check-managed-app-manager-boundary.py
    python3 scripts/check-managed-app-process-boundary.py
    python3 scripts/check-global-plan-number-uniqueness.py
    python3 scripts/check-adr-number-uniqueness.py
    python3 -m unittest discover -s tests/planning -p 'test_*.py'
    cargo deny check advisories bans sources

Run the installed-package/autostart/restart black-box lane and the complete current AGENTS.md routine floor.

## Documentation updates

Required:
- ADR 0037 or next free;
- managed-app policy/state v1 reference;
- app-state/appctl architecture docs;
- updates to appd, daemon, dependency graph, security model and tooling;
- package-v1 cross-link;
- roadmap/registry/closure.

CLI docs must state plainly:
- signatures identify publishers but do not mean trusted;
- unsafe-direct gives ordinary host networking because no sandbox is applied;
- changes are offline and require app-runtime restart;
- app exit is not automatically restarted.

## Acceptance criteria

Plan 374 passes only when:
1. persistent policy generations survive restart and malformed highest state fails closed;
2. publisher identity is exact key fingerprint identity;
3. grants bind publisher+app and never derive from requests;
4. only Sam/I2cp are grantable;
5. exact selected package is required and reverified;
6. install never auto-selects or auto-trusts;
7. unsafe-direct requires explicit acknowledgement;
8. Secured remains unavailable before exec;
9. appd holds runtime mutation lock and uses one immutable policy snapshot;
10. production appd replaces EmptyCatalog only with the verified persistent catalog;
11. fresh OS-random instance ids are used across restarts;
12. operator-approved autostarts reach private SAM/I2CP;
13. one bad app does not block siblings;
14. no live admin endpoint/update/relaunch/sandbox/broker/UI/Proposal170 work lands;
15. guards and negative mutations prove authority seams;
16. focused, black-box and full floors pass.

## Stop conditions

Stop and register a corrective/successor if:
- a live administrator IPC surface becomes necessary;
- launch requires policy mutation while appd is running;
- package trust requires remote transparency/repository access;
- platform target selection requires executing package code;
- UnsafeDirect can be selected without explicit acknowledgement;
- policy needs a mutable pointer with ambiguous crash semantics;
- package/state crates need router protocol internals;
- sandbox/resource enforcement becomes necessary for an acceptance claim.

## Closure evidence required

- ADR/policy-schema freeze;
- transaction crash matrix;
- trust/grant/version matrix;
- untrust-no-resurrection proof;
- CLI command/output samples;
- target/resource matrix;
- package tamper-before-restart proof;
- OS RNG/fresh-instance evidence;
- two-app restart/autostart black-box transcripts through real SAM/I2CP;
- runtime/admin lock contention proof;
- boundary-checker mutation evidence;
- full routine floor and exact-head CI if available;
- support/config diff;
- unblock audit for the first OS sandbox plan.

## Handoff notes

The intended v1 operator workflow is:

    stop app runtime/router
      -> verify/install package
      -> trust publisher
      -> select exact package
      -> grant Sam/I2cp
      -> choose profile
      -> enable autostart if desired
      -> start router

This makes every authority transition durable and reviewable before untrusted code runs.

After Plan 374, the next feature milestone is OS-specific Secured backends, starting with a platform whose supported primitives can satisfy every required SandboxProperty without weakening the contract.
