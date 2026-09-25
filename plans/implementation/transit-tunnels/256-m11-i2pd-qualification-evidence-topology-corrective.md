# Plan 256 — M11 exact-pinned i2pd qualification evidence/topology corrective

Status at registration:
**registered-m11-i2pd-qualification-evidence-topology-corrective-ready**

Baseline: `014f72d3e9c0a4128b0d6cacdb003b55c0943b12`

Corrects:
`plans/implementation/transit-tunnels/255-m11-exact-pinned-i2pd-transit-qualification.md` and
`plans/closure/transit-tunnels/255-status.md`.

Hard dependencies:

- Plan 250 corrected the runtime-neutral admission/reply/registry contract.
- Plan 252 retained the full-message ShortTunnelBuild transaction.
- Plan 253 retained the bounded runtime-neutral data plane, rollback/drain, and peer-state work.
- Plan 254 closed the live ingress/body-threading owner boundary.
- Plan 255 landed useful qualification scaffolding, but its post-closure source audit found that
  its external evidence semantics and reference topology cannot establish the claims recorded by
  the mandatory rows. Plan 255 is therefore retained as historical infrastructure and this plan is
  the corrective authority.

## 1. Objective

Repair the Plan 255 exact-pinned i2pd qualification lane so that every counted M11 external
result is produced by a real, independently observable protocol event in a valid controlled
topology.

The corrected lane must:

1. use the same i2pr RouterIdentity X25519 static private key that corresponds to the public
   encryption key advertised in the signed RouterIdentity when opening ECIES tunnel-build
   records;
2. install the genuine signed i2pr RouterInfo into the exact NetDB owned by each reference
   process before that process performs peer selection;
3. construct real stock-i2pd role topologies for OBEP, IBGW, and intermediate Participant,
   including a second exact-pinned i2pd reference when the Participant role requires it;
4. derive role evidence from the decoded build request and independent reference/topology facts,
   never from configuration intent or a generic "some build arrived" boolean;
5. execute real code-30 rejection, data-plane replay, logical expiry, cancellation,
   session-close, and restart experiments;
6. make each mandatory evidence row depend on the exact event/counter/digest that row claims;
7. pass the complete external matrix twice on the same i2pr SHA with fresh datadirs before M11
   experimental progression is considered qualified.

This is a **corrective capability-qualification plan**. It does not redesign the Plan 250-254
transit implementation unless the corrected external lane localizes a real production defect
there.

## 2. Why the corrective is required

Post-closure review of Plan 255 found defects that the static evidence checker did not detect.

### 2.1 Generic build observation fans out into unrelated "true" rows

The Plan 255 driver records many independent success keys inside one
`if observed_build` branch. A single `LiveBuildOutcome::Dispatched` can therefore mark OBEP,
IBGW, Participant, fragmented OBEP delivery, IBGW multi-cell delivery, replay suppression,
code-30 rejection, bandwidth disposition, expiry, cancellation, restart, and Participant
forwarding as true without observing those events.

This is the primary evidence-integrity defect. Plan 256 must make this construction impossible.

### 2.2 Role attribution is not typed evidence

Plan 255 does not require a decoded `TransitBuildRoute` / hop role to match the row being
recorded. One generic dispatched build can satisfy all three role groups. The corrected driver
must bind every role row to a typed decoded role and the expected topology instance.

### 2.3 The Participant topology is incomplete

The runner defines a second reference port but does not start an i2pd-B process and does not
construct the explicit two-reference topology required to prove i2pr as an intermediate
Participant. Plan 256 must provision the second exact-pinned reference and prove the selected
path rather than infer it.

### 2.4 RouterInfo bootstrap does not target the running reference reliably

The current driver writes the generated i2pr RouterInfo after i2pd-A has already started and
uses a fallback path derived from the evidence directory rather than a proven reference NetDB
owner. File existence is not proof that the running reference loaded the RI.

The corrected topology must install the public RI before reference startup or use a
source-locked runtime import mechanism, and it must retain an observable "reference loaded this
router hash" fact.

### 2.5 Tunnel-build static key does not match the advertised RouterIdentity

The current driver generates a `RouterIdentityBundle`, signs the i2pr RouterInfo from that
identity, and then creates a separate fresh X25519 private key for `TransitHopMaterial`.
ECIES tunnel-build Noise-N request encryption is addressed to the hop's X25519 static key from
the RouterIdentity. The private key used by the transit service must therefore correspond to
the public encryption key in that exact signed RouterIdentity.

Plan 256 must source the build responder secret from the same `RouterIdentityBundle`
encryption key. The SSU2 transport static key is a separate transport key and must not be
substituted. No private key may be serialized into retained evidence.

### 2.6 Several ownership rows are recorded before the event they claim

Plan 255 writes keys such as `live-next-inbound-observed` and
`authenticated-peer-bound` before the inbound loop proves a matching event. Plan 256 records
such rows only after the exact authenticated inbound event has been consumed.

### 2.7 Rejection, expiry, replay, cancellation, and restart are not external experiments

The current external driver does not actually switch to deterministic rejection, replay a
genuine TunnelData cell, advance the injected transit clock beyond the 600-second lifetime,
cancel with live state, or restart the topology and re-check zero state. Plan 256 must execute
those transitions rather than project local-test authority into external rows.

## 3. Frozen protocol/reference authority

Keep the Plan 255 reference frozen:

    implementation = i2pd
    version        = 2.61.0
    commit         = 635b013a612ff47278ef02acf8580a28e10e26c5
    transport      = SSU2 IPv4 loopback only
    public reseed  = disabled
    public network = disabled

Do not patch the reference source, mutate reference objects in memory, use LD_PRELOAD, or fall
back to the public network.

Protocol authority remains the current ECIES-X25519 tunnel-creation specification. In
particular, the hop's Noise-N static X25519 keypair is the RouterIdentity encryption keypair,
not the SSU2 transport static keypair and not an unrelated test key.

Source-lock exact i2pd 2.61.0 behavior for:

- NetDB filename/directory mapping and startup load/import behavior;
- `explicitPeers` parsing and peer ordering;
- tunnel-pool creation for inbound and outbound pools;
- short-build request creation and role flags;
- build-reply handling;
- the observable facts used to prove a selected path.

Documentation may guide the investigation, but the exact pinned source is the counted authority.

## 4. Corrected qualification ownership model

Prefer a **single qualification owner** for identity, reference lifecycle, and evidence. The
recommended shape is:

    Rust external qualification driver
      -> generate one ephemeral RouterIdentityBundle
      -> derive signed public i2pr RouterInfo
      -> retain RouterIdentity encryption private key only in memory
      -> create fresh i2pd-A / i2pd-B datadirs
      -> install public i2pr RI into the exact source-locked NetDB layout
      -> write source-locked tunnels.conf / i2pd.conf
      -> start exact-pinned reference process(es)
      -> verify each reference loaded the expected i2pr router hash
      -> start i2pr SSU2 service with the same RouterIdentityBundle
      -> construct TransitHopMaterial from that bundle's encryption private key
      -> consume real Ssu2DaemonHandle::next_inbound()
      -> record typed event evidence
      -> terminate/drain all owners and child processes

The shell runner may remain the outer cache/pin/workflow wrapper, but it must not own a second
independent identity lifecycle that forces private key transfer between processes.

If implementation constraints require a two-process bootstrap, use a bounded IPC/pipe that
keeps private key material ephemeral. Do not write the RouterIdentity private key, tunnel
layer keys, reply keys, or SSU2 private keys into the retained evidence directory. A temporary
secret file is not an acceptable shortcut unless a new security review explicitly authorizes
and securely deletes it; prefer in-memory ownership.

## 5. Work package A — identity/key coherence regression

Add a focused test-only composition seam that proves:

1. the public encryption key in the generated RouterIdentity equals the public key derived from
   the private key handed to `TransitHopMaterial`;
2. the same RouterIdentity hash appears in the signed RouterInfo supplied to i2pd;
3. the SSU2 static transport key remains distinct and is not used for tunnel-build ECIES;
4. no extra X25519 responder key is generated after the RouterIdentityBundle is created;
5. secret-owning wrappers remain non-`Clone`, redacted, and zeroizing.

Add a static/evidence guard that rejects the old Plan 255 pattern: a fresh
`X25519PrivateKey::generate` used as the transit responder in the external driver after the
RouterIdentityBundle already exists.

Mandatory corrective evidence:

    m11-i2pd-routeridentity-build-key-coherent
    m11-i2pd-routerinfo-hash-coherent
    m11-i2pd-ssu2-key-not-build-key
    m11-i2pd-no-independent-transit-responder-key

## 6. Work package B — deterministic reference bootstrap

Rework provisioning so public i2pr RouterInfo is available to the reference before role
selection.

For each reference process:

1. create a fresh datadir;
2. generate the i2pr identity/RI first;
3. source-lock i2pd's exact NetDB storage/import convention;
4. place/import the RI into that exact datadir before i2pd startup, unless the pinned source
   proves a supported runtime import/reload mechanism;
5. start i2pd with public reseed disabled;
6. wait for its SSU2 listener and RouterInfo;
7. prove from a source-locked observable that the expected i2pr router hash was loaded;
8. only then create/start the tunnel pool that selects i2pr.

Do not count "file written" as "reference knows i2pr".

The runner must pass explicit datadir paths if shell still owns provisioning. Remove fallback
path guessing from `EVIDENCE_DIR`.

Mandatory rows:

    m11-i2pd-a-netdb-owner-exact
    m11-i2pd-a-loaded-i2pr-ri
    m11-i2pd-b-netdb-owner-exact
    m11-i2pd-b-loaded-i2pr-ri
    m11-i2pd-reference-knows-i2pr-ri

For one-hop OBEP/IBGW rows, i2pd-B may be absent, but the Participant matrix must start and
verify it.

## 7. Work package C — real role topologies

Create separate fresh topology epochs for each role. Do not reuse one generic build observation
to satisfy multiple roles.

### C1. OBEP epoch

Configure a stock-i2pd outbound tunnel/pool whose exact selected hop is i2pr. Count success only
after:

- i2pr receives the stock-created STBM through authenticated `next_inbound`;
- the decoded request role is exactly `OutboundEndpoint`;
- the expected creator/reference identity is bound to the authenticated peer;
- the transformed reply follows the OBEP route;
- i2pd accepts the build as usable;
- exactly one i2pr registration exists for the decoded receive tunnel id.

### C2. IBGW epoch

Configure a stock-i2pd inbound tunnel/pool whose first hop is i2pr. Count success only after the
decoded request role is exactly `InboundGateway`, the creator accepts the reply, and the
registration/state tuple matches that request.

### C3. Participant epoch

Start i2pd-A and i2pd-B at the exact frozen pin. Configure a source-locked explicit path in
which i2pr is neither gateway nor endpoint, for example:

    i2pd-A -> i2pr -> i2pd-B

or the exact inbound analogue supported by the pinned implementation.

Count success only when:

- the decoded request role is exactly `Participant`;
- decoded next-router identity is the expected i2pd-B identity;
- the transformed STBM is delivered to that reference;
- the creator accepts the completed build;
- i2pr state contains only hop-local information.

If stock 2.61.0 cannot deterministically produce the intermediate placement under the
source-locked configuration, stop with a topology boundary. Do not synthesize the build.

Each role gets a unique epoch id in sanitized evidence. A row for one epoch cannot be satisfied
by another epoch.

## 8. Work package D — typed evidence ledger

Replace boolean fan-out with an append-only typed observation ledger. An observation should
carry only non-secret facts needed for qualification, for example:

    epoch
    event kind
    authenticated peer hash
    link id
    decoded role
    receive tunnel id
    next router hash
    next tunnel id
    next message id
    dispatch outcome
    registration count before/after
    delivery count before/after
    sanitized payload digest where appropriate
    logical time
    reference acceptance counter/fact

The final TSV/JSON evidence row generator must consume these observations. It must not accept
arbitrary string keys emitted from a generic branch.

Required invariants:

- each mandatory row has one dedicated predicate over typed observations;
- predicates identify the expected epoch and role;
- a missing observation fails the row;
- an observation from the wrong role/epoch fails the row;
- no single `observed_build`-style boolean can satisfy more than the small set of facts that
  event directly proves;
- role-specific rows require typed decoded role evidence;
- data-plane rows require actual delivery/transform counters or digests;
- cleanup rows require before/after state snapshots;
- static checker rejects a bulk block that appends unrelated success keys after a generic
  build observation.

Add negative unit tests that intentionally provide only one OBEP observation and prove every
IBGW/Participant/rejection/expiry/restart row remains failed.

## 9. Work package E — real external data-plane experiments

After each accepted role is established, execute the relevant data-plane path.

### Participant

Send at least two genuine reference TunnelData cells. Prove:

- receive tunnel id resolves the accepted registration;
- authenticated previous peer matches;
- output goes to the expected i2pd-B router/tunnel;
- a far-side digest/counter changes, proving the transform is not a no-op.

Then replay one already accepted cell and prove there is no second semantic delivery.

### OBEP

Drive an unfragmented message and one fragmented message through the reference-created tunnel.
Record exactly one semantic delivery for each complete message and prove the fragmented case
does not double-deliver.

### IBGW

Send genuine TunnelGateway ingress at the accepted gateway id and prove bounded TunnelData
emission to the reference. Exercise a multi-cell shape and retain emitted/received cell counts.

Do not satisfy these rows by directly calling runtime-neutral role helpers after startup.

## 10. Work package F — real rejection/bandwidth experiment

Create a fresh topology epoch with deterministic i2pr admission rejection.

A pass requires:

1. stock i2pd originates the build;
2. i2pr decodes the expected role;
3. admission returns the normal code-30 policy rejection;
4. the transformed reply follows the role-correct route;
5. the reference observes the failed build and does not use the tunnel;
6. no registration exists for the rejected receive id;
7. pending/per-peer/global counters return to baseline.

Bandwidth evidence is conditional on what stock 2.61.0 actually emits. If `m`/`r` are
present, record sanitized parsed values and prove the reply disposition. If absent, record the
explicit `external-bandwidth-options-not-emitted-by-reference` disposition and retain Plan
250 local wire tests as authority. Never fabricate options.

## 11. Work package G — expiry, cancellation, session close, restart

Use distinct topology epochs and typed state snapshots.

### Expiry

With a live accepted registration, advance the existing injected transit clock beyond
creation + 600 seconds. Then deliver a genuine reference TunnelData cell through authenticated
SSU2. Prove expiry occurs before transform/forward, no next-hop delivery occurs, and active
state/secret ownership returns to baseline.

### Cancellation

With at least one live registration, cancel the outer qualification owner. Prove synchronously:

    active registrations = 0
    pending reservations = 0
    peer index entries = 0
    queued transit work = 0

### Session close

Close the authenticated reference session and prove `note_session_closed` reconciliation
removes the peer mapping without deleting unrelated peers.

### Restart

Stop the complete controlled topology, destroy/recreate fresh reference and i2pr runtime
owners/datadirs as specified by the lane, then prove the new transit owner begins with zero
active/pending/peer state. "Fresh datadir was requested" is not sufficient; record the actual
post-restart state snapshot.

## 12. Work package H — runner/checker/workflow hardening

Retain useful Plan 255 surfaces:

    crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs
    tests/integration/m11-transit/run-i2pd.sh
    scripts/check-m11-transit-qualification-evidence.sh
    .github/workflows/m11-transit-external.yml

Correct them rather than creating a parallel harness.

The checker must additionally reject:

- bulk success-key emission gated only by generic `observed_build`;
- role rows without a decoded typed role;
- Participant evidence when no second reference process/topology is proven;
- RI-loaded rows derived only from a file write;
- fallback NetDB paths derived from the evidence directory;
- an unrelated fresh X25519 transit responder key;
- ownership rows emitted before matching `next_inbound` observation;
- rejection/expiry/replay/cancel/restart rows without dedicated event/state evidence;
- complementary partial runs merged into a pass;
- retained secret/private-key material.

The external Rust test remains `#[ignore]` gated. Explicit execution uses
`--ignored --exact`; missing cache, wrong pin, missing topology prerequisites, or missing
environment must fail nonzero.

The hosted workflow must upload only sanitized evidence and logs. Raw reference datadirs and
private i2pr state are scratch and must be removed before artifact upload.

## 13. Failure, cancellation, and resource semantics

- Every child reference process belongs to one bounded lifecycle owner and is terminated on
  success, test failure, timeout, panic unwind where practical, and cancellation.
- Timeouts are finite and row-specific; no "wait until green" loops.
- Reference startup failure is not converted into a skipped/pass row.
- Queue/resource denial remains a real failure or expected code-30 experiment, never a retry
  loop.
- No public network/reseed fallback.
- No per-cell task spawning or unbounded evidence/event queue.
- Typed observation storage has a hard ceiling appropriate to one qualification epoch.
- Secret wrappers remain move-only/redacted/zeroizing.

## 14. Required focused regressions

At minimum add tests that fail against the Plan 255 implementation and pass after correction:

1. one generic dispatched build cannot satisfy more than its directly observed ownership facts;
2. OBEP evidence cannot satisfy IBGW or Participant rows;
3. Participant row fails without a running/proven i2pd-B topology;
4. reference-known-RI row fails when the RI is written outside the active reference NetDB;
5. reference-known-RI row fails when written after startup without a proven reload/import;
6. tunnel-build responder public key equals RouterIdentity encryption public key;
7. an independently generated responder key is rejected by the qualification composition;
8. `live-next-inbound-observed` cannot be emitted before a matching inbound event;
9. code-30 rows require a real rejection epoch and zero registration;
10. replay row requires delivery-count delta 1 then 0 on replay;
11. expiry row requires logical time beyond 600 seconds and zero forward delta;
12. cancellation row requires a nonzero pre-state and zero post-state;
13. restart row requires an observed zero state from a newly constructed owner;
14. evidence parser rejects duplicate/conflicting epoch ids and cross-epoch row reuse;
15. exact pin/version mismatch fails before network startup.

## 15. Exact verification floor

Run:

    cargo fmt --all --check
    cargo check --locked --workspace --all-targets
    cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
    cargo test --locked --workspace --all-targets -- --test-threads=1
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
    cargo test --locked --workspace --doc
    cargo deny check advisories bans sources
    bash scripts/check-dependency-direction.sh
    bash scripts/check-runtime-boundaries.sh
    bash scripts/check-service-tunnel-boundaries.sh
    bash scripts/check-m11-transit-boundaries.sh
    bash scripts/check-m11-transit-qualification-evidence.sh
    bash scripts/check-m6-mixed-router-acceptance-evidence.sh
    bash scripts/check-exploratory-tunnel-evidence.sh
    git diff --check

Run all new focused negative/positive evidence tests explicitly.

Then execute the **complete** external matrix twice on the same implementation SHA:

    bash tests/integration/m11-transit/run-i2pd.sh
    bash tests/integration/m11-transit/run-i2pd.sh

Each run uses fresh ports and datadirs and independently passes every mandatory row. Do not
merge rows across runs.

Closure also requires ordinary exact-head GitHub Actions green for Ubuntu Quality, macOS
Quality, MSRV, and Dependency policy.

## 16. Acceptance criteria

Plan 256 closes only when all are directly evidenced:

1. exact-pinned unmodified i2pd 2.61.0 / `635b013a...` is used for every reference;
2. public reseed/network remain disabled;
3. i2pr RouterInfo is generated before reference peer selection and loaded by the exact
   reference NetDB owner;
4. the tunnel-build responder private key corresponds to the RouterIdentity encryption public
   key in that RI;
5. the SSU2 static transport key remains a separate key;
6. real authenticated `next_inbound` events feed the enabled `TransitLiveOwner`;
7. OBEP, IBGW, and Participant are proven by typed decoded roles in separate controlled epochs;
8. Participant uses a real second exact-pinned reference and proves next-hop delivery;
9. each accepted role installs exactly one bounded registration;
10. Participant TunnelData transforms/forwards with a far-side digest/counter;
11. OBEP unfragmented/fragmented traffic produces the expected one-shot semantic delivery;
12. IBGW TunnelGateway traffic emits bounded genuine TunnelData, including a multi-cell case;
13. replay produces no second delivery;
14. deterministic stock-i2pd build rejection produces code 30 and zero registration;
15. bandwidth option disposition reflects actual reference bytes only;
16. logical 600-second expiry drops later genuine data and removes state;
17. cancellation, session close, and restart are separately executed and return state to
    baseline;
18. every mandatory row is derived from its own typed event predicate and epoch;
19. evidence checker has regressions proving one generic build cannot fan out into unrelated
    passed rows;
20. complete matrix passes twice on one i2pr SHA with fresh datadirs;
21. full workspace verification and exact-SHA ordinary CI are green;
22. no product default, RouterInfo capability, router.version, public-network, or public transit
    advertisement change lands.

Only then may the unblock audit mark the one-family M11 experimental progression gate passed
under ADR 0026 and register M12 planning.

## 17. Stop conditions

Stop and record the exact boundary rather than expanding the harness if:

- exact-pinned stock i2pd cannot load/select the genuine i2pr RouterInfo through a source-locked
  unmodified mechanism;
- stock i2pd cannot deterministically place i2pr in one of the three required roles;
- the reference requires a false capability/router.version claim;
- the corrected RouterIdentity key coherence exposes a production tunnel-build crypto defect;
- a real role-correct build reaches Plan 250/252/253/254 code and fails there;
- deterministic Participant placement requires patching i2pd;
- public network/reseed is required;
- a new production dependency or public transit toggle is proposed merely to make the lane pass.

If a production defect is localized, preserve the sanitized failing transition and register a
narrow successor rather than weakening Plan 256 evidence.

## 18. Documentation and closure

On closure:

- write `plans/closure/transit-tunnels/256-status.md`;
- preserve the Plan 255 closure evidence as historical infrastructure evidence;
- update registry, roadmap, support ledger, tunnel dossier, and conformance matrix;
- include exact implementation/closure SHAs, reference pins, topology epochs, key-coherence
  proof, per-row typed evidence mapping, both complete same-SHA external runs, ordinary CI,
  resource/secret review, limitations, and M12 unblock audit.

No M11 public capability is claimed by registration or implementation of this corrective.

## 19. Handoff order

Execute in this order:

1. fix RouterIdentity/build-key coherence and add its regression;
2. move identity generation ahead of reference startup and prove exact NetDB ownership/load;
3. provision i2pd-B and establish source-locked explicit role topologies;
4. introduce typed epoch/event evidence and negative anti-fan-out tests;
5. qualify OBEP and IBGW builds;
6. qualify Participant build and next-hop forwarding;
7. execute role data-plane experiments;
8. execute rejection/bandwidth;
9. execute expiry/replay/cancel/session-close/restart;
10. harden checker/workflow against every Plan 255 false-positive shape;
11. run the full local verification floor;
12. run the complete external matrix twice on the same SHA;
13. close only if every row is independently evidenced.

Do not start with repeated hosted dispatches of the Plan 255 lane. Its current evidence semantics
are the defect this plan corrects.
