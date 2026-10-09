# Plan 409 — External Rust Managed-App SDK and Package Builder

Status: **in-progress-external-sdk-and-package-builder**

Global number reconciliation: the imported draft used Plan 388, which Proposal 170 owns on main. This successor is Plan 409; the source draft is preserved at `plans/archive/managed-native-app-runtime/388-external-rust-managed-app-sdk-and-package-builder.md`.

Date: 2026-10-09

Roadmap: `plans/subsystems/managed-native-app-runtime-roadmap.md`

Readiness:

- Managed native app runtime/369–371 closed.
- Plans 382–383 closed.
- Managed native app runtime/408 — local-service minor-version protocol extension — closed as `passed-host-owned-loopback-local-service-ingress`; this plan consumes its v1.1 types.
- Repository license is MIT.

Primary consumers:

- `dbowm91/i2pr-tc`;
- `dbowm91/i2pr-mail`;
- `dbowm91/i2pr-irc`.

Classification: external interface + infrastructure + developer tooling.

## 1. Objective

Provide a supported Rust surface for applications in separate repositories to:

1. speak the managed-application protocol over inherited stdin/stdout without
   copying `i2pr-app-proto` internals;
2. open/close bounded logical service streams and adapt them to AsyncRead /
   AsyncWrite-style consumers;
3. handle lifecycle/health/control replies with the same framing and limits as
   the router;
4. build and verify signed `.i2prapp` artifacts from an application manifest
   and payload tree.

Today `i2pr-app-proto` and `i2pr-app-package` are `publish = false`. A
separate application would otherwise need a git/path dependency on router
internals or duplicate the protocol/package formats. Neither is an acceptable
long-term first-party app contract.

This plan exposes **application** functionality only. AppManager protocol,
administrator authority, local policy state, and package-store mutation remain
internal.

## 2. Public crate boundary

Stabilize/publish the smallest reusable crates, expected to be:

- `i2pr-app-proto` — runtime-neutral validated protocol and manifest types;
- new `i2pr-app-sdk` — application-side session/multiplexer over generic async
  byte streams, with an inherited-stdio constructor for the standard launch;
- new or split `i2pr-app-package-build` — deterministic package assembly and
  local verification primitives that do not expose the router package store.

Do not publish:

- `i2pr-app-manager-proto`;
- `i2pr-app-state`;
- `i2pr-appd` internals;
- administrator/grant construction APIs beyond what an application needs to
  inspect its effective permissions;
- daemon gateway/router types.

If reusing code from `i2pr-app-package` requires a clean split, move canonical
format/verification code into a neutral package-format crate and keep the
mutable store private.

## 3. SDK session model

The application SDK owns one managed-app session over generic inherited I/O.

Required behavior:

- send canonical `hello` and validate `accept`;
- expose effective capabilities as read-only state;
- maintain at most the protocol's bounded in-flight request count and live
  stream count;
- open `sam`/`i2cp` logical streams;
- correlate open/reply/data/close/reset by nonzero stream id;
- surface backend EOF/reset per logical stream rather than killing unrelated
  streams;
- obey cancellation and backpressure;
- never open a host socket itself;
- never interpret raw SAM/I2CP payload bytes.

The SDK may provide Tokio adapters under a feature, but the protocol crate
remains runtime-neutral.

Plan 408 adds the `local_service` minor-version transaction. This plan consumes
that typed extension while preserving v1.0 compatibility; future protocol-minor
extensions must be addable without rewriting the stream owner.

## 4. Package builder

Define a supported developer path equivalent to:

```text
manifest.json + payload/ + Ed25519 signing key
             |
             v
 deterministic .i2prapp builder
             |
             v
 same verifier used by the router
```

Required:

- canonical inventory ordering;
- exact manifest bytes/signing transcript defined by package v1;
- stored-only/deterministic archive profile required by the format;
- executable-bit declaration per inventory entry;
- bounded file count/individual/total sizes;
- duplicate/traversal/symlink/special-file rejection;
- publisher id derived from the signing public key and required to match the
  manifest;
- private signing key never logged or written into the package;
- post-build verification before success is reported.

Provide either a small CLI (`i2pr-app-pack`) or an SDK builder plus an
equivalent documented cargo invocation. A first-party app release must not need
private router source-tree helpers to construct its package.

## 5. Version/publication policy

Use semantic crate versions independent of the router binary release, beginning
at the repository's current `0.1.0` only if the public surface is reviewed as
the intended first external contract.

Before crates.io publication:

- every published crate has complete package metadata, README/docs, license, and
  repository links;
- path dependencies among published crates have explicit compatible versions;
- no unpublished internal crate is in the normal dependency graph;
- MSRV remains 1.89 unless an explicit successor raises it;
- `cargo package` and install-from-package tests pass.

A git-revision consumer fixture is acceptable before publication, but closure
requires the public package artifacts if publication is the selected contract.

## 6. External-consumer fixture

Add a fixture outside the router dependency graph that:

1. depends only on the publishable SDK/protocol/package-build surface;
2. builds a minimal app package;
3. is launched by the real Plan-383 appd/apphost path;
4. completes hello/accept;
5. opens a granted SAM stream and exchanges exact bytes with the router-side
   private service seam;
6. closes cleanly.

The fixture must not import daemon/runtime/manager protocol crates.

## 7. Security and compatibility

- Application input can request permissions but cannot construct grants.
- SDK effective-capability state is host-origin only.
- Unknown future message kinds/minor versions fail according to the frozen
  version-negotiation rules, not permissive JSON parsing.
- Package builder does not trust its own output; it round-trips through the
  canonical verifier.
- No signing key material crosses into runtime protocol messages.
- No environment variable except the documented managed-app inputs is treated
  as authority.
- `UnsafeDirect` is not an SDK networking feature; the SDK never provides
  direct host connectors.

## 8. Work packages

### WP1 — public-surface audit

Identify exactly which current types can be external and split private
store/manager authority away from them.

### WP2 — application SDK

Implement generic I/O session ownership, request correlation, logical stream
adapters, cancellation, and backpressure.

### WP3 — package-build surface

Implement deterministic signed package construction and post-build
verification.

### WP4 — package metadata/publication

Make the selected crates packageable/publishable with versioned dependencies
and MSRV/license metadata.

### WP5 — external consumer qualification

Build/run an out-of-workspace consumer through real appd/apphost + private SAM.

### WP6 — docs/guards/closure

Add quick-start documentation and dependency guards proving the SDK cannot
reach administrator/router internals.

## 9. Acceptance criteria

Plan 409 closes when:

1. a separate Rust repository can depend on the supported app SDK without
   copying protocol code;
2. it can build a signed `.i2prapp` without importing router package-store
   internals;
3. the package passes the router's canonical verifier;
4. the external fixture launches through Plan 383 and opens private SAM;
5. SDK streams obey protocol ceilings/backpressure/cancellation;
6. no AppManager/admin/router-internal authority is published;
7. package/install-from-package and routine verification pass;
8. documentation names the protocol/crate compatibility policy.

## 10. Stop conditions

Stop if publication requires exposing AppManager/admin constructors, if a
separate app still needs a path dependency into the i2pr workspace, if package
construction cannot share the canonical verification transcript without
duplicating it, or if the chosen public crate graph contains unpublished
internal dependencies.

## 11. Closure evidence

Create `plans/closure/managed-native-app-runtime/409-status.md` with public
API/package list, crate versions, external-consumer dependency graph, package
round-trip evidence, real apphost/SAM fixture result, negative authority tests,
publication/package evidence, routine floor, and downstream unblock audit.
