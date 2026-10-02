# Plan 301 — Integrated anonymity evidence and scoped claim gate

Status: stopped-predecessor-qualification-evidence-incomplete

Classification: evidence + closure + polish. No new protocol capability is expected.

Hard dependencies: Plans 297, 299, and 300 closed with executed evidence. Plan 296 invariants retained.

## Objective

Create one machine-checked anonymity qualification matrix that composes service-boundary, HTTP, Streaming, Destination-isolation, NetDB-route, and tunnel-path evidence, then update documentation to the strongest narrow claims actually proven without converting the project into a broad production-anonymity claim.

## Why ready after dependencies

Each predecessor owns a different observable layer. This plan exists to prevent one layer's success from being misrepresented as end-to-end anonymity.

## Invariants

- Closure/status evidence remains authoritative over prose.
- Unexecuted external lanes are not passes.
- Out-of-scope dimensions remain explicitly out of scope.
- No Tor Browser equivalence, arbitrary-application anonymity, global passive-adversary resistance, or production-readiness claim.
- No raw payload/private identities/path membership in committed evidence.
- Regression in any required predecessor gate fails integrated result.

## Scope

### In

Machine-readable dimension/status matrix; evidence-integrity checker; exact-pin provenance aggregation; negative service transcript regression; HTTP differential result; Streaming differential result; Destination/target isolation; NetDB tunneled lookup/publication and namespace; tunnel/path diversity/profile; docs/security-model and support/architecture wording reconciliation; stable CI integration.

### Out

New tuning heuristics, new network protocol behavior, global anonymity score, synthetic marketing language.

## Required work

### A. Qualification matrix

Add a machine-readable artifact under `specs/` or established evidence schema with:
- dimension id;
- threat observer;
- boundary;
- required plan/closure token;
- exact evidence/checker;
- reference pin where applicable;
- state: proven / intentionally divergent / unproven / out-of-scope;
- authorized documentation wording.

Minimum dimensions:
1. router branding in HTTP remote transcript;
2. local alias in HTTP Host;
3. IRC/router branding;
4. generic/SOCKS router-added branding;
5. HTTP default profile differential;
6. CONNECT/TLS application fingerprint limitation;
7. Streaming active-probe differential;
8. dedicated/target-isolated client Destination identity;
9. explicit shared-group linkability;
10. client NetDB namespace separation;
11. client lookup/publication tunneled routing;
12. tunnel path duplicate/diversity policy;
13. service tunnel length/profile;
14. global timing/traffic-analysis resistance (out-of-scope unless future plan changes it).

### B. Integrity checker

Add `scripts/check-anonymity-qualification-evidence.sh` that fails on missing predecessor evidence, stale commit/pin references, required dimension not proven, documentation claiming stronger state than matrix, seeded forbidden transcript markers, or malformed/unbounded evidence artifacts.

Checker runs deterministically without external routers against committed sanitized evidence. External requalification stays separate/manual or scheduled CI.

### C. Documentation reconciliation

Update `docs/security-model.md`, architecture/support docs, and service-tunnel docs to:
- retain experimental/non-production warning;
- state exact proven implementation-neutrality properties;
- state exact reference/tunnel/Streaming qualification scope;
- explain opaque application/TLS/browser fingerprint limits;
- distinguish implementation neutrality from traffic-analysis resistance;
- explain custom/raw/shared settings can leave qualified anonymity set.

Do not change `specs/support.toml` protocol-support semantics unless schema has an appropriate non-protocol dimension.

### D. CI gate

Add deterministic evidence-integrity checker to ordinary CI if runtime is small/stable. Keep external Java/i2pd reruns environment-gated/manual if needed. CI never silently skips missing committed evidence.

### E. Regression drill

Seed one controlled failure for each class—product token, alias token, stale Streaming evidence, path-policy mismatch—and prove checker rejects it before removing seed.

## Failure/cancellation/restart/contention

No production runtime changes expected. CI/evidence tools are bounded/fail-closed. Missing files, malformed rows, stale hashes, or unavailable required closure tokens are failures.

## Compatibility and migration

Documentation/config guidance may label raw/custom/shared modes outside qualified profile. No runtime migration expected.

## Required tests

- matrix schema/complete dimension inventory;
- evidence checker positive path;
- seeded failure tests;
- documentation-claim lint;
- transcript leak fixtures;
- evidence hash/pin verification;
- ordinary CI execution;
- routine workspace floor.

## Exact verification commands

~~~text
bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-http-anonymity-evidence.sh
bash scripts/check-streaming-fingerprint-evidence.sh
bash scripts/check-anonymity-qualification-evidence.sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
~~~

Use actual predecessor checker names recorded by closures.

## Documentation updates

This plan owns final scoped wording, cites exact closure evidence, and preserves experimental status.

## Acceptance criteria

Plan 301 passes only if:
1. every required matrix dimension is mechanically classified/linked;
2. required predecessor states are proven on recorded commits;
3. checker rejects seeded leak/staleness/overclaim failures;
4. security model contains no stronger claim than matrix authorizes;
5. opaque app/TLS/browser and global traffic-analysis limits are explicit;
6. custom/raw/shared modes are outside default qualified profile where applicable;
7. ordinary CI/routine floor green;
8. no new production protocol behavior is smuggled into closure pass.

## Stop conditions

Stop if predecessor evidence is stale/incomplete/contradictory; documentation cannot express result without broad claim; or a required external lane has not actually executed.

## Closure evidence required

Plan 301 closure is workstream authority and includes matrix hash, predecessor closure tokens/commits, checker output, exact reference pins, ordinary CI run, routine floor, authorized claim wording, and explicit unproven/out-of-scope dimensions.

## Handoff notes

Closing this plan completes the parallel anonymity workstream as scoped by ADR 0029. It does not mark i2pr production-ready and does not gate M12/mainline completion.
