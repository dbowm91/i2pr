# Plan 099: NTCP2 independent-interoperability exit, harness simplification, and router-build unblock

## Status and authority

- Status: planned; corrective scope reset and execution plan.
- Date: 2026-08-11.
- Planning baseline: `45ee8b3a08287deb833370218d9c43b19d4e22ad` or a clean descendant that has not materially changed the NTCP2 interop/reference-driver architecture described here.
- Parent staged-evidence authority: Plan 067 and ADR 0023.
- Historical execution authority retained for audit: Plans 075-098.
- Active objective after this plan lands: obtain the smallest technically meaningful independent NTCP2 development result, remove the Plan 095-098 CI/provenance apparatus from the critical path, and resume construction of the Rust router.
- Primary independent implementation: pinned i2pd 2.60.0 at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Execution topology: `host-loopback-development`, literal IPv4 `127.0.0.1`, network ID 99, fresh state and ports.
- Release/isolation status: development-only; not release-qualified; not isolation-qualified; no claim that public egress is technically impossible on the GitHub-hosted runner.
- NTCP2 status: experimental, non-advertised, and disabled in normal daemon operation until the bounded activation gates below are met.

Plan 099 supersedes Plan 095 as the active *development-interoperability execution architecture*. It does not erase Plan 095-098 history. It replaces the multi-job artifact/provenance pipeline with a deliberately smaller development smoke lane and amends the continuation policy so unavailable release-grade execution infrastructure cannot block unrelated router construction.

The desired post-Plan-099 sequence is:

```text
Plan 099 implementation + simplification
        |
        +--> one manual Ubuntu single-job two-way i2pd smoke
        |       |
        |       +--> genuine wire defect -> one bounded owner correction
        |       |                         (no new harness architecture)
        |       |
        |       +--> two-way development smoke passed
        |              -> NTCP2 may be composed later behind explicit
        |                 experimental opt-in, still non-advertised
        |
        +--> production router construction continues regardless of
             release-qualification availability:
             daemon composition -> local RouterInfo service -> NetDB core
             -> reseed/bootstrap -> authenticated NetDB exchange -> tunnels...

Plan 079 repeated validation
        = pre-activation / pre-advertisement confidence work,
          not a global router-build gate

Plan 073 / Level 3
        = later release qualification; unchanged and still environment-bound
```

## Why this plan exists

The repository has inverted its implementation priorities. The core project is a Rust I2P router, but a long corrective sequence created a large Python/YAML/shell evidence and CI apparatus around one experimental transport. That apparatus has repeatedly failed on path ownership, artifact transfer, executable permissions, shell continuation, run-root lifecycle, manifest identities, status-token authority, and observer provenance before producing a clean independent wire result.

Those failures were useful until they localized real problems. Continuing to add plan-numbered schemas, status vocabularies, provenance layers, Python test matrices, multi-job artifact gates, and corrective workflow plans now has sharply diminishing value.

Plan 099 therefore applies three rules:

1. **Independent interoperability is valuable, but its development value is bounded.** Prove that real i2pr NTCP2 bytes interoperate with one mature independent implementation in both directions and carry one authenticated correlated I2NP message. Do not require release-grade evidence infrastructure to learn that fact.
2. **The current environment is sufficient for development protocol evidence.** Host loopback and a private non-production network ID exercise the actual TCP/Noise/NTCP2/I2NP implementation. They do not prove egress isolation, NAT behavior, anonymity, or release deployment properties, and this plan makes no such claims.
3. **Router construction is not globally blocked by NTCP2 release qualification.** Daemon composition, RouterInfo lifecycle, NetDB storage/state machines, offline reseed parsing, and other non-public/non-advertised work may continue while NTCP2 remains experimental. Only NTCP2 production activation/advertisement is gated on the appropriate interoperability confidence level.

## Research findings: what independent NTCP2 interoperability is actually worth

Research snapshot: 2026-08-11.

Primary sources reviewed:

- Official NTCP2 specification: <https://i2p.net/en/docs/specs/ntcp2/>
- Official transport overview: <https://i2p.net/en/docs/overview/transport/>
- Official I2NP specification: <https://i2p.net/en/docs/specs/i2np/>
- Official alternative-router overview: <https://i2p.net/en/docs/overview/alternative-clients/>
- Pinned i2pd `Transports` API: <https://github.com/PurpleI2P/i2pd/blob/f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e/libi2pd/Transports.h>
- Pinned i2pd `TransportSession` API: <https://github.com/PurpleI2P/i2pd/blob/f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e/libi2pd/TransportSession.h>

### Finding R1 — NTCP2 is a router-to-router I2NP transport, not a complete-router test

The official transport documentation defines NTCP2 as point-to-point router transport for I2NP messages. A successful independent NTCP2 result therefore has high value for validating:

- RouterInfo/static-key binding and network-ID interpretation;
- SessionRequest / SessionCreated / SessionConfirmed interoperability;
- Noise transcript/key split compatibility;
- masked frame-length and ChaCha20-Poly1305 data-frame compatibility;
- RouterInfo/options/padding ordering and acceptance;
- short I2NP encoding over the authenticated data phase;
- one concrete peer-to-peer I2NP message exchange.

It does **not** establish:

- NetDB correctness or convergence;
- reseed correctness;
- tunnel build/participation behavior;
- garlic/LeaseSet/streaming behavior;
- SSU2 behavior;
- public reachability/NAT behavior;
- anonymity or censorship-resistance properties;
- resource stability under real network load;
- release readiness.

The amount of engineering spent proving NTCP2 must therefore remain proportional to this transport-level value.

### Finding R2 — host loopback is legitimate development protocol evidence

The NTCP2 specification defines a TCP byte protocol and explicitly defines a network-ID field so cross-network/test-network peers can be rejected. It does not require the two routers to be on different physical hosts for the handshake or data-phase semantics to be meaningful.

For i2pr development, two genuine processes on `127.0.0.1`, fresh RouterInfos, a non-production network ID (`99`), and direct RouterInfo exchange exercise the real protocol implementation without public I2P bootstrap.

This topology cannot prove network isolation because GitHub-hosted runners have general network access during the job. Plan 099 therefore records exactly:

```text
development_protocol_evidence = allowed
release_qualification          = false
isolation_qualification        = false
public_egress_impossible       = unproven
```

No additional namespace, container, Multipass, firewall, or egress-proof architecture is required for this development result.

### Finding R3 — i2pd is a sufficient primary independent validator for the development floor

The official I2P documentation describes i2pd as a stable, actively maintained, independent C++ router implementation compatible with the Java network. This makes pinned i2pd materially valuable as a cross-implementation validator: it does not share i2pr's Rust NTCP2 implementation, state machine, crypto wrappers, or runtime.

For this development floor, Java I2P and Emissary are not required:

- Java remains valuable and required later for release-level compatibility confidence.
- Emissary remains useful as a differential third implementation only if i2pr and i2pd disagree at a specific wire stage that cannot be resolved from the specification and source.
- Neither should block normal router construction after the i2pd development floor is obtained.

### Finding R4 — one exact DeliveryStatus round trip is an appropriately small data-phase proof

The I2NP specification defines DeliveryStatus as message type 10 with a fixed 12-byte body. The current i2pr launcher already has exact message-ID and peer-Router-Hash correlation.

One correlated DeliveryStatus is therefore enough for the **minimum development transport proof** because it crosses all of these boundaries:

```text
real TCP
-> NTCP2 Noise authentication
-> authenticated data frame
-> NTCP2 I2NP block framing
-> independent I2NP decode
-> exact message identity correlation
```

Testing every I2NP message type belongs to its owning milestone, not the NTCP2 smoke lane.

### Finding R5 — repeated matrices increase confidence, not architectural knowledge

Three fresh-state repetitions per direction and a broad negative-control matrix are useful before enabling a transport in normal router operation. They are not required to begin implementing the daemon, NetDB, reseed parsing, or other protocol state machines.

Plan 079 therefore remains useful but changes role under Plan 099:

```text
before Plan 099: repeated Level 2 result effectively gates continuation
under Plan 099:   repeated validation gates higher NTCP2 confidence /
                  normal activation decisions, not unrelated router buildout
```

Level 3 remains the release gate.

## Critical implementation finding: the current observer build is not what the repository claims

This is the most important concrete blocker discovered during Plan 099 research.

The observer patch modifies:

```text
libi2pd/NTCP2.cpp
```

and inserts the transport call sites for:

```text
ObserveTcpAccepted
ObserveAuthenticated
ObserveReceivedI2NP
ObserveSentI2NP
```

However, the current `build-driver.sh` performs this sequence:

```text
1. build libi2pd/libi2pdclient/libi2pdlang from pristine pinned i2pd
2. copy the pinned source to PATCHED_SRC
3. apply the observer patch to PATCHED_SRC/libi2pd/NTCP2.cpp
4. build the driver executables
5. link both driver executables against the libraries produced in step 1
```

The current driver CMake receives `I2PD_PATCHED_TREE` and `I2PD_PRISTINE_TREE` as *include* roots, but both binaries share the same `I2PD_LIB_DIR`, which points to the pristine static archives built before the observer patch was applied.

Therefore the patched `NTCP2.cpp` observer call sites are not compiled into the static transport library used by the current instrumented executable.

This explains why adding more observer-generation/provenance machinery cannot make the lane authoritative: the actual transport call sites must first exist in the built transport.

Plan 099 must fix this at the build boundary, not by adding another Python observer layer.

## Secondary implementation finding: the current control path is structurally unsuitable as a full observer-equivalent run

The uninstrumented control driver is intentionally compiled without `I2PD_INTEROP_OBSERVER`. Yet `run_listen()` and `run_dial()` currently wait on observer APIs such as:

```text
WaitForTcpAccepted
WaitForAuthenticated
WaitForReceivedDeliveryStatusAfter
WaitForSentDeliveryStatusAfter / WaitForSentI2NP
```

A pristine control transport has no observer call sites and therefore cannot satisfy these waits by design.

This is not a useful control contract.

Pinned i2pd already exposes native transport/session state suitable for a behavior-neutral control:

```text
Transports::IsConnected(IdentHash)
Transports::SendMessage(...) -> future<shared_ptr<TransportSession>>
TransportSession::IsEstablished()
TransportSession::GetNumSentBytes()
TransportSession::GetNumReceivedBytes()
```

The control should use those native APIs to prove that the pristine transport can establish the same session and move authenticated data. It should **not** attempt to reproduce the instrumented exact post-decode observer record.

The instrumented build owns exact DeliveryStatus receive/decode evidence. The pristine control owns observer-neutral session/data viability.

## Scope reset

### What Plan 099 keeps

Keep only the surfaces needed for one real development result:

- pinned i2pd 2.60.0 source lock;
- real i2pd direct driver;
- one correctly built instrumented i2pd transport;
- one genuinely pristine control transport;
- existing i2pr NTCP2 launcher;
- exact RouterInfo / Router Hash / DeliveryStatus correlation;
- bounded forward and reverse runners;
- one manual Ubuntu 24.04 workflow;
- one concise sanitized result summary;
- Rust protocol/runtime tests and focused direct-driver tests.

### What Plan 099 removes from the active critical path

Remove or retire as active requirements:

- multi-job build -> upload -> download -> chmod -> hash-manifest -> instrumented -> upload -> control -> upload -> validate-gate choreography;
- cross-job executable-bit restoration;
- CI artifact transfer as binary identity authority;
- duplicate i2pr/i2pd build-manifest identity plumbing for a development-only smoke;
- Plan 095-098 status-token acceptance matrices as runtime correctness tests;
- plan-numbered workflow regressions for defects that disappear when cross-job artifacts disappear;
- a control requirement that depends on observer events absent from the control build;
- repeated 3/3 Level 2 validation as a blocker for daemon/NetDB implementation;
- Java, Emissary, rootless namespaces, Multipass, Docker, release certificates, candidate freezing, and public-network behavior from this smoke lane.

Historical plan documents remain readable. Historical implementation files should remain only when still used by the active runner or when deletion would remove genuine protocol tests rather than planning scaffolding.

## Hard complexity budget

Plan 099 is a simplification plan. Its implementation must obey all of these constraints:

1. **No new plan-numbered Python test file.** Do not create `test_plan099.py`.
2. **No new Python orchestration layer.** Fix or reuse the smallest existing runner/adapter surface.
3. **Tracked Python LOC must decrease from the Plan 099 implementation baseline.** Measure with `tokei`, `cloc`, or a deterministic `git ls-files '*.py'` + line-count script before and after. The before/after numbers go in the Plan 099 status record.
4. **The Plan 095 manual workflow becomes one execution job** (setup/build/run/summarize in one GitHub-hosted Ubuntu job). A tiny optional metadata/contract job is not allowed merely to preserve the old shape.
5. **No binary upload/download between jobs.** The job executes the exact binaries it just built in the same workspace.
6. **Upload at most one sanitized result artifact.** Do not upload binaries, raw RouterInfos, raw event logs, private state, transcripts, keys, or per-stage evidence bundles.
7. **No embedded provenance Python gate larger than the result writer itself.** Basic shell/Python digest calculation is acceptable; rebuilding the Plan 095 gate is not.
8. **No new evidence schema hierarchy.** Reuse the compact forward/reverse record schemas where practical and write one small summary record over their result/digest fields.
9. **No new environment-placement architecture.** Use `host-loopback-development` exactly as already implemented.
10. **No retry action and no retry loop.** One run is one result.

A Plan 099 implementation that increases Python LOC, adds another CI layer, adds another provenance manifest family, or creates a new topology is non-conforming even if its tests pass.

## Work package 1 — replace the fake instrumented-library relationship with two real i2pd library builds

### WP1.1 Pristine library build

Retain a pristine pinned i2pd source tree at exactly:

```text
f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
```

Build the control archives from the untouched tree:

```text
libi2pd.a
libi2pdclient.a
libi2pdlang.a
```

The control executable links only those archives.

### WP1.2 Instrumented library build

Create a private copy of the pinned tree, apply the existing narrow observer patch, copy the observer header where required, and build a second set of i2pd archives **from the patched tree** with `I2PD_INTEROP_OBSERVER=1` visible while `NTCP2.cpp` is compiled.

The instrumented executable links only the instrumented archives.

Do not patch the canonical checkout in place.

### WP1.3 Split CMake link ownership

Replace the single shared `I2PD_LIB_DIR` relationship with explicit instrumented and pristine library directories, for example:

```text
I2PD_INSTRUMENTED_LIB_DIR
I2PD_PRISTINE_LIB_DIR
```

or an equivalent unambiguous mechanism.

Required relationship:

```text
i2pd_ntcp2_interop_driver_instrumented
    -> patched NTCP2.cpp compiled with I2PD_INTEROP_OBSERVER
    -> instrumented libi2pd archives

i2pd_ntcp2_interop_driver_control
    -> untouched pinned NTCP2.cpp
    -> pristine libi2pd archives
```

Header include-path differences alone do not satisfy this criterion.

### WP1.4 Object-level proof

Use `nm -C`, `objdump`, or an equivalent deterministic symbol inspection to prove:

- the instrumented NTCP2 object/archive contains references to the required `i2pr::i2pdinterop::Observe*` functions;
- the final instrumented executable resolves those functions;
- the pristine NTCP2 archive/control executable contains no observer call-site references from the i2pd transport;
- both binaries are linked against the pinned source revision and execute `--help` successfully.

Do not use a symbol-definition-only check: linking `interop_observer.cpp` into a binary proves the functions exist, not that `NTCP2.cpp` calls them.

## Work package 2 — make the pristine control a native i2pd session/data control

The control build must not wait for observer metadata it cannot emit.

Use compile-time or build-role branching in the existing C++ driver. Do not add a new Python control implementation.

### WP2.1 Outbound/dial control

For pristine-control `dial`:

1. import the exact i2pr RouterInfo;
2. create the exact nonzero DeliveryStatus;
3. call the real `Transports::SendMessage(peer_hash, message)`;
4. wait on the returned future under the existing bounded deadline;
5. require a non-null `TransportSession`;
6. require `session->IsEstablished()`;
7. require positive native sent-byte/session activity consistent with an established send;
8. require the i2pr side to report successful authenticated receipt of the exact DeliveryStatus ID.

No observer wait is allowed in the pristine branch.

### WP2.2 Inbound/listen control

For pristine-control `listen`:

1. bind the real NTCP2 listener;
2. import/know the exact i2pr Router Hash;
3. wait boundedly for `Transports::IsConnected(peer_hash)` or the equivalent native established-session state;
4. once established, submit the correlated DeliveryStatus response through native `Transports::SendMessage`;
5. require the returned session to remain established;
6. require i2pr to receive/decode the exact response ID;
7. cleanly stop.

The control does not claim exact receive/decode of i2pr's request inside pristine i2pd. That exact receiver proof belongs to the correctly instrumented run. The purpose of control is narrower: the observer patch is not required for a pristine i2pd session and authenticated data send to work.

### WP2.3 Instrumented semantic proof

The instrumented path continues to require the exact post-authentication observer predicates:

```text
TCP accepted / native connection
NTCP2 authenticated
exact peer Router Hash
exact target DeliveryStatus decoded
exact target DeliveryStatus sent when response is required
clean teardown
```

Do not weaken the instrumented target correlation merely to make it pass.

## Work package 3 — replace Plan 095 multi-job choreography with one manual Ubuntu job

Rewrite `.github/workflows/ntcp2-interop-host-loopback-development.yml` into one manual development job.

Required workflow properties:

```text
trigger              = workflow_dispatch only
runner               = ubuntu-24.04
permissions          = contents: read
network topology     = 127.0.0.1 only for live peer endpoints
network ID           = 99
reference            = pinned i2pd 2.60.0
release qualified    = false
isolation qualified  = false
NTCP2 advertised     = false
```

### WP3.1 Single-job lifecycle

The job sequence is conceptually:

```text
checkout
-> install declared build dependencies
-> fetch/verify pinned i2pd source
-> build pristine + instrumented i2pd libraries/drivers
-> build release i2pr-interop
-> verify exact executable paths and hashes
-> run forward instrumented
-> if forward instrumented passed: run forward pristine control
-> run reverse instrumented
-> if reverse instrumented passed: run reverse pristine control
-> write one concise sanitized summary
-> unconditional raw-state cleanup
-> upload only the sanitized summary
```

The forward/reverse ordering may be changed if the existing runner contract requires it, but all four roles execute in the same job/workspace and no binary crosses an artifact boundary.

### WP3.2 Delete obsolete artifact-transfer logic

The simplified workflow must not contain:

```text
actions/upload-artifact for build binaries
actions/download-artifact for build binaries
restore-executable-bit after artifact download
cross-job build-manifest comparison
cross-job binary digest comparison
cross-job BUILD_OUTPUT reconstruction
separate forward-control job gating on downloaded instrumented evidence
separate validate-gate job reconstructing artifact identity
```

The final summary may itself be uploaded with `actions/upload-artifact` once.

### WP3.3 Digest policy

For a development smoke, record only identities that are useful for reproduction:

```text
source_commit
i2pd_revision
i2pr_binary_sha256
i2pd_instrumented_binary_sha256
i2pd_control_binary_sha256
forward_record_sha256
reverse_record_sha256
control outcomes
```

A tracked-source-tree digest may be retained if already cheap and correct, but it must use one canonical implementation. Do not keep three independently reimplemented digest algorithms.

If source-tree digest identity is retained, define one helper/command and reuse it. Do not duplicate Python/shell byte encodings.

## Work package 4 — define the minimum independent-development interoperability exit gate

Plan 099 deliberately replaces the Plan 095/079 development continuation gate with a smaller factual statement.

### WP4.1 Forward direction

One fresh-state `i2pr -> i2pd` instrumented run passes only when all are true:

```text
real TCP connection occurred
pinned i2pd transport accepted the peer
NTCP2 authentication completed
peer RouterInfo/static key/Router Hash binding passed
instrumented i2pd decoded one exact DeliveryStatus from i2pr
DeliveryStatus message ID matched the scenario ID
no duplicate target satisfied the result
expected response, if configured, was sent and decoded by i2pr
cleanup = clean
```

### WP4.2 Reverse direction

One fresh-state `i2pd -> i2pr` instrumented run passes only when all are true:

```text
real TCP connection occurred
pinned i2pd initiated the peer session
NTCP2 authentication completed
peer RouterInfo/static key/Router Hash binding passed
i2pr decoded one exact DeliveryStatus from i2pd
i2pr response / i2pd exact receive correlation is observed where the
current reverse scenario requires it
DeliveryStatus message ID matched exactly
cleanup = clean
```

### WP4.3 Pristine control

After each instrumented direction passes, the corresponding pristine control must establish the same direction using native i2pd session state and must successfully send authenticated data that i2pr receives.

The control is not required to emit the instrumented observer's exact receive record.

### WP4.4 Development exit result

The only Plan 099 development result values are:

```text
two-way-development-smoke-passed
forward-wire-defect
reverse-wire-defect
environment-or-build-blocked
```

Do not create a larger decision vocabulary.

`two-way-development-smoke-passed` means only:

```text
one real fresh forward instrumented pass
one real forward pristine-control session/data pass
one real fresh reverse instrumented pass
one real reverse pristine-control session/data pass
same i2pr source commit
same pinned i2pd revision
exact Router Hash / DeliveryStatus correlation where instrumented
clean teardown
```

It is sufficient to stop the current interoperability-investigation loop and proceed with router buildout.

It is not sufficient to advertise NTCP2 or claim release readiness.

## Work package 5 — hard stop policy for further NTCP2 harness work

Once the simplified lane reaches a genuine protocol stage, failures are owned by protocol/runtime code, not by another evidence architecture.

For each direction:

1. preserve the concise failed result;
2. identify the highest authentic stage reached;
3. inspect the owning i2pr source + official spec + pinned i2pd source;
4. reproduce once from fresh state if needed;
5. correct only the owning Rust NTCP2/runtime code or the demonstrated narrow C++ reference-driver defect;
6. rerun once;
7. stop when the direction passes or a precise reproducible wire defect is documented.

Forbidden responses to a protocol failure:

- a new plan-numbered schema;
- a new provenance manifest family;
- a new runner layer;
- a new execution topology;
- a timeout increase without evidence;
- retries until green;
- adding Java/Emissary before a specific differential question exists;
- rebuilding release qualification infrastructure.

If a genuine NTCP2 defect remains after the bounded correction cycle, record it and continue non-NTCP2 router construction. NTCP2 stays disabled/non-advertised until corrected.

## Work package 6 — retire Plan 090-098 corrective scaffolding from the active Python/CI surface

This package is deliberately destructive to *scaffolding*, not protocol tests.

### WP6.1 Measure the language baseline

At the start of implementation, record tracked code lines for at least:

```text
*.rs
*.py
```

Prefer `tokei`; otherwise use `cloc`; otherwise use a deterministic tracked-file line counter.

Record:

```text
rust_loc_before
python_loc_before
```

At closure record:

```text
rust_loc_after
python_loc_after
```

`python_loc_after` must be strictly lower than `python_loc_before`.

This is a floor, not a target ratio. Do not add meaningless Rust code to manipulate the ratio.

### WP6.2 Remove plan-specific corrective tests that no longer protect an active architecture

After the single-job workflow and corrected reference build have focused replacement coverage, remove the Plan 090-098 Python test matrices whose sole purpose is enforcing superseded workflow/status/provenance shapes.

Expected deletion candidates include, subject to `git grep` dependency confirmation:

```text
tests/integration/ntcp2/harness/test_plan090.py
...
tests/integration/ntcp2/harness/test_plan098.py
```

Do not blindly delete files still containing the only regression for a genuine Rust protocol bug. Move such a regression to the owning Rust crate test or the smallest active direct-driver test before deleting the historical plan test.

### WP6.3 Remove the Plan 095 workflow-audit architecture

`scripts/check-plan095-workflow.sh` should be deleted or reduced to a trivial bounded check only if some non-obvious safety invariant still requires it. The preferred result is deletion: a one-job manual development workflow does not justify hundreds of lines of plan-specific grep validation.

Remove corresponding invocations/marker checks from `scripts/check-ntcp2-interoperability.sh`.

### WP6.4 Simplify the static interoperability checker

The active checker should verify architectural boundaries, not historical plan prose.

Keep checks for:

- NTCP2 remains non-advertised/default-disabled;
- host-loopback development runner cannot select public endpoints/profiles;
- direct i2pd driver is pinned;
- reference build really distinguishes patched instrumented archives from pristine control archives;
- release/candidate artifacts cannot consume development smoke evidence.

Remove checks whose only purpose is requiring old plan-numbered test files, old status tokens, old workflow job names, or old artifact paths.

### WP6.5 Historical documents remain documents

Do not delete Plans 090-098 or rewrite their historical outcomes. Add a concise supersession note where needed pointing to Plan 099.

Historical plans are audit history, not executable dependency roots.

## Work package 7 — formally unblock router construction

Plan 099 must update the active status authority to separate three different questions that have been conflated:

```text
router_build_continuation
ntcp2_development_interop
ntcp2_activation_or_advertisement
```

Required semantics after Plan 099 implementation lands, even before the manual smoke completes:

```text
router_build_continuation       = allowed
ntcp2_development_interop       = pending-plan099-two-way-smoke
ntcp2_normal_daemon_activation  = blocked-pending-development-smoke-and-later-integration-plan
ntcp2_advertised                = false
ntcp2_release_qualified         = false
```

`router_build_continuation = allowed` permits work that does not claim working public NTCP2, including:

- production daemon/supervisor composition;
- persistent identity load and lifecycle;
- local RouterInfo construction/publication service architecture;
- NetDB record validation/storage/expiry/quotas;
- DatabaseLookup/SearchReply state machines under deterministic tests;
- offline/local SU3 reseed parsing and trust-store design;
- persistence/restart validation;
- resource/budget policy;
- tunnel and later-protocol design work whose tests do not require a public router transport.

After `two-way-development-smoke-passed`, a later router-composition plan may wire NTCP2 into `i2pr-daemon` only behind an explicit experimental opt-in. It must remain non-advertised until the project reaches the later confidence gate selected for advertisement.

### WP7.1 Reclassify Plan 079

Plan 079 remains useful but is no longer the entry gate for unrelated Milestone 4 implementation.

Reclassify it as:

```text
purpose = repeated NTCP2 confidence + negative controls
required_before = normal NTCP2 activation/advertisement decision
not_required_before = daemon composition / NetDB implementation / offline reseed work
```

Do not delete Plan 079.

### WP7.2 Preserve Level 3

Do not weaken Plan 073 / Level 3 release qualification.

Java compatibility, stronger isolation/no-public-egress evidence, repeated qualification, and release review remain later requirements before a production support claim.

## Work package 8 — next router roadmap handoff

Plan 099 closes the interoperability detour. The next substantial planning artifact must be a router implementation roadmap, not Plan 100 interoperability infrastructure.

The next roadmap should begin with:

```text
1. production i2pr-daemon composition over i2pr-runtime Supervisor
2. persistent router identity + local RouterInfo service
3. bounded NetDB core (validated RouterInfo store, expiry, quotas, persistence)
4. DatabaseStore / DatabaseLookup / DatabaseSearchReply state machines
5. offline SU3 reseed ingestion
6. HTTPS reseed acquisition only after parser/trust/storage are stable
7. experimental NTCP2 service composition after Plan 099 two-way smoke
8. authenticated NetDB exchange over peer links
```

The next roadmap must not require Plan 073/Level 3 release qualification to begin items 1-6.

## Focused validation policy

Plan 099 intentionally reduces verification scope.

Required before the implementation commit:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Plus only the focused active NTCP2/reference-driver tests touched by this plan, expected to include the smallest applicable subset of:

```bash
python3 -m unittest tests/integration/ntcp2/harness/test_i2pd_direct_driver.py
python3 -m unittest tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py
python3 -m unittest tests/integration/ntcp2/harness/test_plan083_runner.py
python3 -m unittest tests/integration/ntcp2/harness/test_plan084.py
```

Adjust exact module invocation syntax to the repository layout, but do not replace this focused list with `test_*.py` across the entire historical harness.

Required build proof:

```text
instrumented patched libi2pd contains observer call-site references
control pristine libi2pd contains no observer call-site references
instrumented driver links instrumented archives
control driver links pristine archives
both drivers --help successfully
```

Required workflow validation:

- YAML parses;
- `workflow_dispatch` only;
- one Ubuntu execution job;
- no binary artifact upload/download;
- no reverse/public profile leakage;
- one sanitized result upload at most.

Do not require full historical rootless, Multipass, candidate, release-bundle, Java-driver, rustdoc, clippy, fuzz, or all-Python-harness matrices unless this plan changes those specific surfaces.

## Explicit acceptance criteria

Plan 099 implementation is complete only when all of the following are true.

### Authority and scope

1. Plan 099 is recorded as the active development-interoperability simplification/exit authority.
2. Plan 095-098 remain historical records but no longer define the active CI architecture.
3. `router_build_continuation = allowed` is explicit in active status/docs.
4. Plan 079 is explicitly reclassified as NTCP2 repeated-confidence work rather than a global Milestone 4 build gate.
5. Level 3 / Plan 073 release qualification remains unchanged.
6. NTCP2 remains default-disabled/non-advertised.

### Reference build correctness

7. The instrumented i2pd transport library is compiled from the patched pinned source tree, not merely against patched headers.
8. `I2PD_INTEROP_OBSERVER=1` is visible when patched `libi2pd/NTCP2.cpp` is compiled.
9. The instrumented driver links the instrumented i2pd archives.
10. The control driver links only pristine pinned i2pd archives.
11. Symbol/object inspection proves real observer call-site references in instrumented NTCP2 transport objects.
12. Symbol/object inspection proves no observer call-site references in pristine control NTCP2 transport objects.
13. Both driver binaries remain tied to i2pd `f618e417...` and run successfully in inspect/help mode.

### Control semantics

14. Control listen/dial paths do not wait on observer APIs absent from the control build.
15. Pristine outbound control proves a non-null established native `TransportSession` from the real `SendMessage` path.
16. Pristine inbound control proves native peer-connected state and successfully sends correlated authenticated data that i2pr receives.
17. Instrumented paths retain exact DeliveryStatus/Router-Hash observer correlation.

### Workflow simplification

18. The manual Plan 099 development workflow runs build + forward + reverse + controls in one `ubuntu-24.04` job.
19. No built binary is transferred through GitHub Actions artifacts between jobs.
20. No post-download chmod restoration exists because there is no binary download boundary.
21. No separate Plan 095 `validate-gate` job remains.
22. No large embedded Python provenance-equivalence gate remains.
23. At most one sanitized summary artifact is uploaded.
24. Raw/private state, RouterInfos, keys, transcripts, packet data, and binaries are not uploaded.
25. The live peer endpoints remain literal `127.0.0.1` and network ID 99.
26. No sudo/namespaces/Multipass/Docker/public I2P/reseed/SAM/I2CP/SSU2 is used by the live interop phase.

### Development interoperability exit

27. One forward instrumented pass proves real TCP, authentication, exact target DeliveryStatus decode, exact peer identity correlation, and clean teardown.
28. One forward pristine control proves observer-neutral native session establishment and authenticated data send to i2pr.
29. One reverse instrumented pass proves the corresponding real reverse NTCP2/I2NP path with exact correlation.
30. One reverse pristine control proves observer-neutral native session establishment and authenticated data send to i2pr in the reverse role.
31. The four results share one i2pr source commit and one pinned i2pd revision.
32. A successful result is recorded only as `two-way-development-smoke-passed`; it is not called release-qualified, production-ready, isolated, or anonymity-tested.
33. A genuine failure records one of only `forward-wire-defect`, `reverse-wire-defect`, or `environment-or-build-blocked` and does not synthesize a pass.

### Complexity reduction

34. No `test_plan099.py` exists.
35. No new Python orchestration module is introduced.
36. Tracked Python LOC after Plan 099 is lower than the implementation baseline; before/after counts are recorded.
37. Superseded Plan 090-098 Python workflow/status/provenance regression matrices are removed where they no longer protect active runtime behavior.
38. Any genuine protocol regression formerly protected only by a deleted historical test is moved to its owning Rust or focused direct-driver test first.
39. `scripts/check-plan095-workflow.sh` is removed or reduced to a demonstrably tiny still-relevant boundary; the preferred state is deletion.
40. `scripts/check-ntcp2-interoperability.sh` no longer requires historical plan-numbered Python files/status tokens merely to pass.
41. Default/focused validation does not execute the entire historical Python harness.
42. The implementation change is net-negative in Python LOC and does not add an equivalent replacement layer in shell/YAML.

### Router continuation

43. Daemon composition and Milestone 4 offline/local work are explicitly allowed after the Plan 099 implementation correction, regardless of Level 3 environment availability.
44. Normal-daemon NTCP2 activation remains blocked until at least the Plan 099 two-way development smoke and a later explicit composition decision.
45. NTCP2 advertisement remains false after Plan 099.
46. The next substantial roadmap is production daemon + RouterInfo + NetDB/reseed work, not another interoperability-evidence roadmap.

## Failure branches

### Branch A — build proof still shows no instrumented transport call sites

Owner: `build-driver.sh` / i2pd library build / CMake link relationship.

Action:

- do not dispatch CI;
- correct the patched-library compile/link relationship;
- rerun symbol proof;
- do not change Rust NTCP2 code.

### Branch B — single-job CI fails before TCP

Owner: exact local build/config/process preparation step.

Action:

- fix that concrete step in place;
- do not create another workflow architecture;
- do not create another plan-numbered evidence layer.

### Branch C — forward reaches TCP/auth/data and fails

Owner: earliest authentic protocol/runtime stage.

Action:

- preserve concise record;
- inspect official NTCP2 spec + i2pr owner + pinned i2pd owner;
- one bounded reproduction;
- correct only demonstrated owner;
- rerun once.

### Branch D — forward passes, reverse fails

Do not reopen forward CI architecture. Correct the reverse owner only.

### Branch E — instrumented passes but pristine control fails

Determine whether the failure is:

```text
control-driver native-state logic defect
or
observer patch affects protocol behavior
```

Do not call the instrumented result final until resolved. Use source diff and native session state; do not add a second observer mechanism.

### Branch F — environment/build remains blocked after the simplification

Record `environment-or-build-blocked`. Continue production daemon/NetDB work that does not activate NTCP2. Do not reintroduce rootless/Multipass as a development prerequisite.

## Small-model execution order

Execute in this order. Do not parallelize conceptual ownership boundaries.

1. Read only this plan, ADR 0023, current Plan 087/088 statuses, `build-driver.sh`, i2pd driver CMake, observer patch, i2pd driver source, and the Plan 095 workflow.
2. Record Rust/Python tracked LOC baseline.
3. Fix the i2pd library build first. Do not touch workflow or Rust protocol code until object-level observer proof passes.
4. Add/adjust the minimum existing direct-driver tests for patched-vs-pristine library linkage. Do not create `test_plan099.py`.
5. Change the pristine control branch to native `Transports`/`TransportSession` state. Keep the instrumented branch exact.
6. Run the direct-driver build locally/CI-compatible and prove both binary roles with symbol inspection.
7. Collapse the GitHub Actions workflow to one job. Delete cross-job binary artifact/provenance logic instead of adapting it.
8. Delete/reduce superseded Plan 090-098 workflow/status/provenance Python tests and checker rules. Use `git grep` before each deletion.
9. Run the focused validation policy only.
10. Record Python LOC after; it must be lower.
11. Update status/docs to make Plan 099 active, Plan 095-098 historical, Plan 079 non-global, router build continuation allowed, NTCP2 still non-advertised.
12. Commit the implementation correction cleanly.
13. From that exact clean commit, dispatch the single manual Plan 099/NTCP2 loopback workflow once.
14. If a genuine protocol defect appears, perform only the bounded owner correction described above. No new harness plan.
15. When the two-way smoke passes, record the concise result and stop NTCP2 harness expansion.
16. Handoff immediately to a production daemon + RouterInfo + NetDB roadmap.

## Non-goals

Plan 099 does not:

- enable NTCP2 in `i2pr-daemon` by default;
- publish an NTCP2 RouterAddress to the public I2P network;
- reseed or join the public I2P network;
- prove NAT traversal, external reachability, or firewall behavior;
- prove anonymity, censorship resistance, timing resistance, or resource stability;
- run Java I2P;
- run Emissary unless a later explicit differential question exists;
- implement SSU2;
- implement NetDB, tunnels, garlic, streaming, SAM, or I2CP itself;
- close Level 3 release qualification;
- preserve historical Python/YAML complexity merely because prior plans created it;
- create a new release/evidence certificate.

## Closure statement

Plan 099 exists to end the current interoperability-infrastructure spiral.

A successful closure means the project has learned the only NTCP2 fact needed to move its architecture forward now:

```text
i2pr can establish real NTCP2 sessions with an independent mature router
implementation in both directions and exchange a correctly correlated
authenticated I2NP message on the development loopback topology.
```

Everything stronger is a later confidence/release question.

After this plan, engineering priority returns to the Rust router.