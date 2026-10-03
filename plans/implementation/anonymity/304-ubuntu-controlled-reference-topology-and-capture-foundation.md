# Plan 304 — Ubuntu controlled anonymity reference topology and capture-lane foundation

Status at registration: **registered-anonymity-ubuntu-reference-topology-foundation**

Classification: infrastructure + evidence corrective. This plan follows the stopped
Plan 297 and Plan 298 records; it does not relabel either predecessor as passed.

Hard dependencies:
- Plan 296 closed as `passed-anonymity-service-boundary-implementation-neutrality-and-leak-regression`.
- `plans/closure/anonymity/297-status.md` and `298-status.md` are the authoritative stop
  records defining the missing external topology/runners.
- ADR 0029 remains the architecture/privacy authority.
- Exact reference pins remain those in
  `tests/integration/anonymity/references.lock.toml`.

Execution environment: **Ubuntu only for the external qualification lane**. macOS/Darwin
portability is explicitly out of scope for this corrective. The runner must record
`/etc/os-release`, kernel, architecture, toolchain versions, and the exact i2pr commit.
A non-Ubuntu host fails preflight before building or launching reference routers.

## 1. Objective

Build one bounded, reproducible, loopback-only Ubuntu harness that can exercise the same
synthetic HTTP corpus and hostile-Destination Streaming scenarios against i2pr, exact-pinned
i2pd 2.61.0, and exact-pinned Java I2P 2.13.0. Prove the topology with one bounded smoke
execution per family and retain only sanitized capture metadata.

This plan supplies the missing infrastructure identified by Plans 297 and 298. It does not
choose an HTTP anonymity profile, tune Streaming production constants, or make any anonymity
equivalence claim. Successful closure authorizes fresh corrective qualification plans; it
does not rewrite the stopped predecessor records.

## 2. Why this is dependency-ready

Plan 296 removed the direct service-boundary branding/alias leaks and established the
negative leak regression gate. Plan 297 then stopped because the reference HTTP proxies and
a controlled remote Destination were not composed into one capture lane. Plan 298 landed
the bounded trace schema, scenario vocabulary, parser/classifier, and evidence guard, but
stopped because no exact-family hostile-Destination runners existed.

The remaining dependency is therefore test infrastructure, not an unresolved production
protocol decision. The user has selected Ubuntu as the qualification environment, so the
GNU `find` behavior that stopped the Java staging helper on macOS is not a required
portability target for this pass.

## 3. Current implementation evidence

Retain and consume rather than duplicate:

- `tests/integration/anonymity/references.lock.toml` freezes i2pd
  `635b013a612ff47278ef02acf8580a28e10e26c5` and Java I2P
  `9134f808337b401e8e53c73734c81fab04280c9d`.
- `tests/integration/anonymity/streaming-scenarios.toml` freezes 11 hostile-Destination
  scenarios, 10 ms buckets, bounded repetitions, and the event/deadline ceilings.
- `i2pr-testkit::streaming_fingerprint` already owns the identity-free trace model and
  classifier.
- `scripts/check-streaming-fingerprint-evidence.sh` guards source pins, scenario inventory,
  trace bounds, and forbidden identity/payload fields.
- Plan 297 proved that exact i2pd builds on the development host and that Java artifacts can
  be staged; no family capture was claimed.
- Existing M6/interop launchers are useful process/reference machinery but are not an
  anonymity capture topology and must not be silently reinterpreted as one.

## 4. Invariants that must not regress

1. The runtime topology is private: loopback/local process traffic only, public reseed and
   public I2P participation disabled.
2. Reference source commits are exact. No patching reference source to make the harness pass.
3. A missing reference binary, missing fixture, failed join, or capture timeout is
   **unexecuted/stopped**, never a passing row.
4. Evidence contains no private Destination material, raw application payloads, RouterInfo
   blobs, peer IPs, local filesystem paths, or unredacted process command lines.
5. The same synthetic application inputs and the same hostile Streaming script are used for
   all three client families.
6. The fixture Destination identity is controlled by the lane and stable across the
   comparison runs for one evidence set; family-specific client identities remain separate.
7. Test infrastructure does not alter production HTTP/Streaming policy.
8. No randomization is introduced as a fingerprint defense.
9. Plan 296's `check-service-anonymity-boundaries.sh` remains green.
10. Every owned child process has a bounded startup deadline, runtime deadline, and cleanup
    path; no orphan reference routers are acceptable closure evidence.

## 5. Scope

### In scope

- Ubuntu preflight and exact reference source/build cache.
- A machine-readable topology manifest and process lifecycle owner.
- Fresh isolated router datadirs for i2pr/i2pd/Java runs.
- Explicit local RouterInfo/bootstrap material sufficient to avoid public reseed.
- One controlled fixture Destination reused across family runs.
- A synthetic HTTP capture endpoint suitable for Plan 297 corrective work.
- A hostile Streaming Destination adapter suitable for Plan 298 corrective work.
- Per-family runners and one unified matrix/preflight entry point.
- Sanitized capture serialization, integrity metadata, negative evidence checks, cleanup
  verification, and smoke executions.

### Explicitly out of scope

- HTTP header/profile convergence or changing default rewrite policy.
- Streaming RTO/window/ACK/retransmission tuning.
- Target-scoped service Destination identities or tunnel peer diversity policy (Plan 305).
- Full Plan 297 or Plan 298 scenario matrices.
- Production anonymity claims or security-model promotion.
- Docker/Kubernetes as a required runtime dependency.
- macOS/BSD portability fixes, including a Darwin fallback for GNU `find -printf`.
- Public I2P bootstrap or live-network observation.

## 6. Required topology contract

Freeze the topology in a committed machine-readable manifest under
`tests/integration/anonymity/`. A family run consists of:

~~~text
synthetic local client
        |
family client proxy / Destination stack
(i2pr | exact i2pd | exact Java I2P)
        |
isolated local I2P router link(s)
        |
controlled fixture router + fixture Destination
        |
+---------------------------+
| HTTP capture endpoint     |
| hostile Streaming adapter |
+---------------------------+
~~~

Run the families in fresh datadirs against the same fixture contract rather than requiring
all clients to execute concurrently. Simultaneous coexistence is not evidence of
equivalence and adds avoidable timing/contention noise. The manifest must prove that each
family used the same fixture identity, input corpus, scenario file, reference pins, and
capture schema.

The runtime must not contact a public reseed server. If an exact-pinned reference cannot be
bootstrapped using explicitly supplied local RouterInfo/peer material, stop and record that
reference integration boundary rather than relaxing network isolation.

## 7. Ordered work packages

### WP1 — Ubuntu fail-closed preflight

Add a single preflight entry point, preferably
`scripts/interop/anonymity/preflight-ubuntu.sh`, which:

- requires `uname -s == Linux`;
- requires `ID=ubuntu` from `/etc/os-release`;
- records `VERSION_ID`, architecture, kernel, Rust, Cargo, Java, Ant, CMake/Make/Ninja
  where used, OpenSSL, Boost, Python, and GNU find/coreutils versions;
- checks loopback bind ability and required free disk/file-descriptor ceilings;
- validates the reference lock file before any source fetch/build;
- fails before execution if an exact required tool/reference cannot be supplied.

Do not add a Darwin compatibility branch in this plan.

### WP2 — Exact-pinned reference cache and immutable build metadata

Create/reuse an ignored cache below `target/interop/anonymity/`. Fetch/build exact i2pd and
Java commits from the lock file. Record a compact manifest containing source commit, build
command, binary/artifact path relative to the cache root, binary hash, and tool versions.

The runner must compare the live checkout HEAD to the lock before use. Reference source
modification, dirty patches, or a commit mismatch fails closed.

### WP3 — Bounded process/topology supervisor

Implement one owner for ports, datadirs, process IDs, startup readiness, deadlines, shutdown,
and cleanup. Requirements:

- all listeners bind loopback;
- each family run gets fresh client-router state;
- the fixture identity/state is explicit and reproducible within one evidence set;
- public reseed/discovery is disabled;
- readiness is based on protocol/listener facts, not fixed sleep alone;
- process stdout/stderr may be retained only as diagnostic artifacts outside committed
  evidence and must be scrubbed before any summary is committed;
- cleanup proves all owned PIDs exited and only the owned temporary directories were removed.

### WP4 — Controlled HTTP capture fixture

Provide a fixture that accepts the remote request generated through each family's real HTTP
client proxy path. Drive a fixed synthetic corpus at minimum containing:

- origin-form GET;
- absolute-form GET to an alias that resolves to the fixture Destination;
- POST with a small deterministic body;
- the header classes identified by Plan 297: User-Agent, Accept, Accept-Language,
  Accept-Encoding, Client Hints, Fetch Metadata, Priority, Upgrade-Insecure-Requests,
  Referer/From, Via/Forwarded/X-Forwarded, one custom header, and connection fields.

Committed evidence may retain synthetic header values because they are test constants, but
must replace the fixture B32 with a stable placeholder and retain body length/hash rather
than body bytes. Preserve request-line form, header name spelling/case, order, multiplicity,
and connection-close semantics because those are fingerprint dimensions.

### WP5 — Hostile Destination adapter

Add a test-only Destination adapter that receives/sends Streaming packets at the
Destination-delivery boundary **without delegating the scripted ACK/loss behavior to the
normal server Streaming manager**. It must consume the existing
`streaming-scenarios.toml` vocabulary and support at least:

- clean establishment;
- delayed ACK;
- ACK withheld through two retransmission opportunities;
- one deterministic loss;
- deterministic reorder;
- advertised-window constraint;
- choke/un-choke when representable by the protocol surface;
- orderly close and reset.

The adapter emits only the existing `StreamingFingerprintTrace` fields. No application
payload or private identity is serialized. If a narrow test seam in production crates is
unavoidable, it must be capability-neutral, hidden from ordinary daemon composition, and
reviewed as an interface seam rather than a behavior change.

### WP6 — Exact family runners

Provide explicit commands/scripts for:

- i2pr HTTP smoke capture;
- i2pd HTTP smoke capture;
- Java HTTP smoke capture;
- i2pr Streaming smoke capture;
- i2pd Streaming smoke capture;
- Java Streaming smoke capture.

A unified wrapper may compose them, but the family-specific runners must remain callable
independently so one family failure is attributable. Every runner prints a closed terminal
classification and the evidence root it produced.

### WP7 — Sanitization and evidence integrity

Add a checker, preferably `scripts/check-anonymity-reference-topology.sh`, that verifies:

- exact source pins and Ubuntu host record are present;
- no forbidden identity/payload/path fields appear;
- all expected family smoke rows exist;
- fixture identity placeholder/integrity hash agrees across families;
- every capture obeys count/size/deadline ceilings;
- evidence files are canonical and parseable;
- a seeded forbidden field and a seeded pin mismatch are both rejected.

### WP8 — One bounded smoke execution

After local/unit verification, perform one smoke pass per family:

- HTTP: one canonical GET reaches the fixture and produces a parseable capture.
- Streaming: `clean_handshake` and one active-control scenario (prefer
  `ack_withheld`) produce parseable traces and demonstrate the fixture controlled the
  response schedule.

This is a topology proof only. Do not execute the full Plan 297/298 matrices under this
plan and do not infer profile equivalence from smoke output.

## 8. Failure, cancellation, restart, and contention semantics

- Startup/join/capture operations have explicit hard deadlines.
- A failed family smoke row stops that family and the plan closes stopped; do not
  retry-until-green.
- Port allocation occurs before process launch and is recorded in non-committed runtime
  metadata; bind races fail the run rather than silently shifting evidence midway.
- SIGINT/SIGTERM/error paths cancel the process tree, wait under a bounded shutdown
  deadline, then force-kill only owned PIDs if necessary.
- Reference datadirs are never reused between counted family runs.
- Build caches may be reused only when commit/hash/toolchain metadata match.
- Fixture state may be reused only within the same declared evidence set; a new evidence
  set starts with a new clean fixture state.

## 9. Compatibility and migration

No production configuration, wire format, listener default, Destination identity policy,
or persistent storage schema changes are authorized. Existing M6/M10 external runners keep
their current meaning. New scripts and testkit surfaces are additive and isolated under the
anonymity lane.

Ubuntu becomes the only supported environment for this external anonymity qualification
lane until a separate portability plan says otherwise. Ordinary workspace development and
tests remain cross-platform.

## 10. Required tests

At minimum add deterministic tests for:

- host/preflight parser and non-Ubuntu rejection;
- reference lock mismatch/dirty source rejection;
- process supervisor startup timeout and cleanup;
- capture schema bounds and canonical serialization;
- B32/identity redaction;
- HTTP duplicate/order/case preservation in the capture model;
- hostile-scenario script parsing and unsupported stimulus rejection;
- fixture event-count/deadline ceilings;
- evidence checker seeded-negative cases;
- smoke evidence composer refusing a missing family.

## 11. Exact verification commands

The implementation handoff should converge on commands equivalent to:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-testkit --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-streaming-fingerprint-evidence.sh
bash scripts/interop/anonymity/preflight-ubuntu.sh
bash scripts/check-anonymity-reference-topology.sh
bash scripts/interop/anonymity/run-http-smoke.sh --family i2pr
bash scripts/interop/anonymity/run-http-smoke.sh --family i2pd
bash scripts/interop/anonymity/run-http-smoke.sh --family java
bash scripts/interop/anonymity/run-streaming-smoke.sh --family i2pr
bash scripts/interop/anonymity/run-streaming-smoke.sh --family i2pd
bash scripts/interop/anonymity/run-streaming-smoke.sh --family java
```

Names may differ if existing interop naming conventions require it, but one documented
canonical command per family/capability is mandatory.

## 12. Documentation updates

- Update the anonymity roadmap and registry from ready to the executed closure state.
- Document the Ubuntu-only external-lane requirement and exact preflight command.
- Document the topology and evidence schema close to
  `tests/integration/anonymity/`; do not broaden `docs/security-model.md`.
- Point future corrective plans for 297/298 at the retained evidence root and runner
  commands rather than duplicating topology setup.

## 13. Acceptance criteria

Plan 304 passes only if all are true:

1. Ubuntu preflight is fail-closed and records the host/toolchain facts.
2. Exact i2pd and Java reference artifacts build/use the frozen source commits without
   patches.
3. The runtime operates without public reseed/live-network dependency.
4. Each of i2pr, i2pd, and Java can reach the same controlled fixture Destination in a fresh
   family run.
5. One HTTP smoke capture exists and parses for each family.
6. `clean_handshake` plus one active Streaming stimulus exists and parses for each family,
   proving the hostile fixture—not a normal server Streaming manager—controlled the reply.
7. Evidence sanitization and seeded-negative checks pass.
8. Owned processes/datadirs are cleaned with no orphan process.
9. The ordinary Rust/test/security floor and Plan 296 leak gate remain green.
10. No production HTTP/Streaming tuning and no anonymity-equivalence claim landed.

## 14. Stop conditions

Stop and record the exact boundary if:

- either exact-pinned reference cannot be built/launched on Ubuntu without patching;
- a reference requires public reseed/network participation to join the controlled fixture;
- the hostile Destination cannot be placed before normal server Streaming behavior without
  a broad production redesign;
- evidence cannot be sanitized without losing a required comparison dimension;
- a production protocol defect, rather than harness absence, is discovered.

Do not weaken isolation, widen evidence retention, patch reference source, or substitute
source-code inference for a missing capture.

## 15. Closure evidence required

The closure record must retain:

- implementation commit(s);
- Ubuntu/kernel/arch/toolchain manifest;
- exact reference source/artifact hashes;
- topology manifest and process/readiness contract;
- one sanitized HTTP smoke capture per family;
- two sanitized Streaming smoke traces per family as defined above;
- cleanup/orphan-process proof;
- evidence-checker seeded-negative proof;
- exact verification command results;
- finding severity table;
- unblock audit naming which new corrective HTTP/Streaming plans may now be registered.

## 16. Handoff

On pass, register fresh corrective qualification plans for the stopped Plan 297 HTTP work
and Plan 298 Streaming work. Those successors consume this topology and execute their full
matrices; they do not reopen or edit the old stop records. Plan 299 still requires executed
Streaming differential evidence after that successor. Plan 301 remains blocked until the
HTTP, Streaming, and Plan-300/305 branches reconverge.
