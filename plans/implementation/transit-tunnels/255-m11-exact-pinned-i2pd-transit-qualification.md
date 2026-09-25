# Plan 255 — M11 exact-pinned i2pd controlled transit qualification

Status at registration:
**registered-m11-exact-pinned-i2pd-transit-qualification-ready**

Baseline: `27c13c0bd3ab1967d15d2f680ed231a0a0cf090b`

Hard dependency:
`plans/closure/transit-tunnels/254-status.md`
(`passed-m11-live-ingress-body-threading-closure-corrective`; exact-SHA Actions
`36099566339` green on `475c25c9eb20f4e3b954d45044bfba9230c8e8d8`).

## Objective

Qualify M11 controlled transit against unmodified exact-pinned i2pd 2.61.0. Stock i2pd
must create real ECIES ShortTunnelBuild traffic addressed to i2pr; the bytes must arrive
through the actual authenticated SSU2 inbound stream and an enabled Plan-254
`TransitLiveOwner`; then prove accepted build replies, role-correct
TunnelData/TunnelGateway behavior, rejection, expiry/cleanup, and repeated same-revision
stability.

This is a **capability qualification** plan for a controlled non-advertised subsystem. It
is not public-network enablement and not a broad router-conformance claim. Hand-constructed
STBMs, direct calls with fabricated bytes, or synthetic registry insertion never count as
external interoperability evidence.

## Why ready

- Plan 250 closed authenticated previous-peer, admission, code-0/code-30, bandwidth,
  time-window, pending-reservation, secret-ownership, and registry semantics.
- Plan 252 retained the full-message STBM transaction: one local-record open/KDF, own
  reply sealing, all-other-record transform, and typed Participant/IBGW/OBEP routing.
- Plan 253 retained the bounded runtime-neutral data plane, complete build envelopes,
  terminal rollback, bounded peer index, and cancellation drain.
- Plan 254 removed the empty-body shim and provides the single-decode
  `TransitInboundBodies` handoff plus `TransitLiveOwner::handle_inbound` with real
  `Ssu2InboundI2np`, creator/service ownership ordering, OBEP delivery, IBGW ingress,
  outer cancellation, and session-close reconciliation.
- Plan 254 exact-SHA ordinary CI is green.

One boundary remains intentionally unqualified: ordinary product construction keeps
transit disabled and its SSU2 pump consults only `controlled_transit_disabled_probe`.
Therefore Plan 255's controlled qualification runtime must own the real
`Ssu2DaemonHandle::next_inbound()` stream and feed those exact authenticated events into
an enabled `TransitLiveOwner`. The disabled probe is not capability evidence.

## Classification and claim boundary

A pass may satisfy ADR 0026's one-independent-family gate for **experimental,
non-advertised M11 progression** and may unblock M12 planning. It does not authorize:

- public transit or public-network participation;
- RouterInfo capability or router.version changes;
- ordinary/default transit enablement;
- Java-router compatibility;
- full two-independent-router-family transit conformance.

`specs/CONFORMANCE.md` remains authoritative for the higher claim.

## Frozen reference

```text
implementation = i2pd
version        = 2.61.0
commit         = 635b013a612ff47278ef02acf8580a28e10e26c5
source         = unmodified clean checkout / verified existing cache
transport      = SSU2 IPv4 loopback only
public reseed  = disabled
public network = disabled
```

Do not change the pin inside Plan 255.

Before topology work, source-lock the exact 2.61.0 code relied on. Current i2pd
documentation/change history is a design input only: 2.61.0 reports that it always sends
short tunnel-build requests, added handling for `m`/`r` build options, and exposes
client-tunnel `explicitPeers` plus inbound/outbound ordering keys. The counted lane must
verify exact-pin behavior and the actual selected role at runtime.

The current ECIES short-build specification remains wire authority: request lifetime is
600 seconds; flags distinguish IBGW, intermediate Participant, and OBEP; `m/r/l` are
request bandwidth options and `b` is the accepted reply option.

## Controlled topology

Prefer the Plan-193 i2pd cache/provisioning conventions with fresh ephemeral datadirs.

Edge roles:

```text
i2pd-A (creator/reference)
  <---- authenticated SSU2 loopback ---->
i2pr (controlled transit enabled only inside qualification runtime)
```

Intermediate Participant, when needed:

```text
i2pd-A (creator) -> i2pr (Participant) -> i2pd-B (reference edge)
```

Both references must be the same exact pin. No source patch, debugger/object mutation,
LD_PRELOAD hook, Docker/namespace requirement, or public-network fallback is allowed.

Use normal i2pd tunnel-pool/client-tunnel configuration only after source-locking
`explicitPeers` and peer ordering. A genuine signed i2pr RouterInfo may be supplied to
the ephemeral reference NetDB through a bounded bootstrap mechanism. Do not synthesize a
build or patch i2pd's in-memory NetDB/tunnel objects.

If deterministic role placement requires patched i2pd or false i2pr RouterInfo
capability/version claims, stop and register a narrow topology successor.

## Work package A — hard gate: real SSU2 inbound ownership

Before any interop result counts, the qualification-owned runtime path must be:

```text
Ssu2DaemonHandle::next_inbound()
  -> Ssu2InboundI2np { authenticated peer, link_id, bytes }
  -> TransitLiveOwner::handle_inbound(...)
  -> dispatch_router_i2np_with_transit_bodies(...)
  -> TransitBuildService / i2pr-tunnel
```

Requirements:

1. the same SSU2 runtime owning the reference session supplies `next_inbound()`;
2. one controlled `TransitLiveOwner` is enabled with the Plan-254 service;
3. no second I2NP decoder exists in the driver;
4. no hand-built STBM is substituted after runtime startup;
5. peer/link provenance comes from `Ssu2InboundI2np`;
6. session close invokes `note_session_closed`;
7. the qualification owner shares the outer cancellation token;
8. ordinary product construction remains disabled.

Mandatory rows:

```text
m11-i2pd-live-next-inbound-observed
m11-i2pd-live-owner-enabled
m11-i2pd-authenticated-peer-bound
m11-i2pd-no-direct-build-injection
m11-i2pd-session-close-reconciled
```

A narrow internal qualification composition seam is acceptable. A public transit toggle is
not.

## Work package B — exact-pin source lock and peer placement

Before the full matrix:

1. verify source/cache revision and `i2pd --version`;
2. source-lock short-build creation, `explicitPeers` parsing, peer ordering, build-reply
   handling, and tunnel expiry/testing used by this lane;
3. make the reference know the genuine signed i2pr RouterInfo;
4. construct the smallest loopback-only pool selecting i2pr;
5. prove selected role from i2pr decoded request metadata plus independent reference facts.

Configuration intent alone does not prove role placement.

Mandatory rows:

```text
m11-i2pd-source-pin
m11-i2pd-source-clean
m11-i2pd-short-build-source-lock
m11-i2pd-explicit-peer-source-lock
m11-i2pd-reference-knows-i2pr-ri
m11-i2pd-selected-role-proven
```

## Work package C — accepted build matrix

Count only stock-i2pd builds received over Work Package A's authenticated SSU2 path.

### OBEP

Create a one-hop i2pd outbound tunnel with i2pr as OBEP. Prove genuine STBM receipt,
decoded `OutboundEndpoint`, accepted response, complete OTBRM routing, reference
acceptance/usable tunnel, and exactly one live registration.

### IBGW

Create a one-hop i2pd inbound tunnel with i2pr as IBGW. Prove decoded
`InboundGateway`, accepted role-correct STBM reply path, reference acceptance/usable
tunnel, and hop-local-only registration state.

### Intermediate Participant

Use a source-locked deterministic two-hop path, normally i2pd-A -> i2pr -> i2pd-B or its
inbound analogue. Prove i2pr is neither gateway nor endpoint, decoded role is
`Participant`, transformed STBM reaches the exact next router/tunnel/message-id tuple,
the creator accepts the reply, and no creator path knowledge enters i2pr state.

All three roles are mandatory for an unqualified Plan-255 pass. If exact-pinned stock i2pd
cannot deterministically place the intermediate hop under the controlled constraints, stop
at that topology boundary instead of fabricating a build.

Mandatory rows:

```text
m11-i2pd-obep-build-received
m11-i2pd-obep-build-accepted
m11-i2pd-obep-registration-live
m11-i2pd-ibgw-build-received
m11-i2pd-ibgw-build-accepted
m11-i2pd-ibgw-registration-live
m11-i2pd-participant-build-received
m11-i2pd-participant-build-accepted
m11-i2pd-participant-registration-live
```

## Work package D — role-correct live data plane

An accepted build is insufficient.

Participant: drive at least two genuine reference TunnelData cells through the accepted
registration. Prove authenticated previous peer -> receive id -> replay/expiry check ->
participant transform -> exact next router/tunnel, with a far-side digest/counter so a
no-op transform cannot pass.

OBEP: drive an unfragmented and one fragmented message through the reference-created
outbound tunnel. Prove reassembly emits exactly one semantic delivery and ROUTER or TUNNEL
delivery reaches the controlled consumer. LOCAL may remain local-test authority unless the
external topology naturally provides a reference-owned LOCAL sink.

IBGW: drive a genuine TunnelGateway message at the accepted gateway id. Prove canonical
IBGW processing emits bounded TunnelData cells and stock i2pd receives them. Exercise a
multi-cell shape where the existing bounded builder supports it.

Mandatory rows:

```text
m11-i2pd-participant-data-forward
m11-i2pd-participant-data-digest
m11-i2pd-obep-delivery
m11-i2pd-obep-fragmented-once
m11-i2pd-ibgw-gateway-ingress
m11-i2pd-ibgw-multicell-bounded
m11-i2pd-replay-no-second-delivery
```

No row may be satisfied by direct `process_tunnel_data`,
`process_tunnel_gateway`, or `deliver_obep_action` calls after runtime startup.

## Work package E — rejection and bandwidth

Run a fresh stock-i2pd build against an i2pr admission policy that deterministically cannot
accept it. Prove stock i2pd originated the build, i2pr returned the normal transformed
code-30 response, no registration was installed, i2pd did not use the tunnel, and all
pending/per-peer/global counters returned to baseline.

If stock 2.61.0 emits `m` or `r`, retain only sanitized option facts and prove accepted
`b >= m` or code-30 when minimum bandwidth cannot be met. If it does not emit these
options in this creator topology, retain Plan-250 local wire tests as authority and record
`external-bandwidth-options-not-emitted-by-reference`. Never patch/fabricate the request.

Mandatory rows:

```text
m11-i2pd-code30-build-rejected
m11-i2pd-code30-no-registration
m11-i2pd-code30-pending-baseline
m11-i2pd-bandwidth-option-disposition
```

## Work package F — expiry, replay, cancellation, restart

After one accepted live tunnel, advance the qualification owner's existing injected
transit clock beyond 600 seconds without sleeping ten wall-clock minutes, then deliver a
genuine reference TunnelData cell through authenticated SSU2. Prove registration expiry
precedes transform, the cell drops, no next-hop delivery occurs, accounting returns to
baseline, and secret-owning state is removed. Do not change production expiry constants.

Stock i2pd need not generate duplicate receive-id builds. That remains mandatory Plan-250
local regression authority unless it naturally occurs. The external duplicate/replay row
is replayed TunnelData producing no second delivery.

Cancel with a live tunnel and prove synchronous drain. Close the reference session and
prove peer-index reconciliation. Restart from fresh datadirs and prove no transit state
survives.

Mandatory rows:

```text
m11-i2pd-expiry-drops-live-data
m11-i2pd-expiry-resource-baseline
m11-i2pd-cancel-drains
m11-i2pd-session-close-peer-baseline
m11-i2pd-restart-clean-baseline
```

## Work package G — fail-closed runner and evidence checker

Preferred surfaces:

```text
crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs
tests/integration/m11-transit/run-i2pd.sh
scripts/check-m11-transit-qualification-evidence.sh
.github/workflows/m11-transit-external.yml
```

Reuse/extract Plan-193 test-only provisioning helpers where practical; do not duplicate
production SSU2, NetDB, build crypto, or tunnel transforms.

The Rust external test is `#[ignore]` gated and explicit execution uses
`--ignored --exact`. Missing reference/cache/source facts fail nonzero.

The runner verifies exact i2pr HEAD and clean i2pd pin(s), uses fresh loopback datadirs,
disables public reseed/network, executes A-F, retains sanitized evidence only, terminates
all child process groups, proves cleanup baselines, and fails on any missing/blocked/red
mandatory row.

The checker must reject literal pass records, missing exact-pin proof, fabricated STBM
ingress, missing `next_inbound` provenance, role inferred only from config, patched i2pd,
public fallback, synthetic registry insertion, bare build bodies counted as complete
envelopes, missing code-30/no-state proof, missing any of the three roles, missing cleanup,
or retained secrets/plaintext/private keys/raw reference datadirs.

Routine CI runs the static checker only; external execution remains manual.

## Work package H — repeated exact-head stability

Execute the complete external matrix **twice on the same i2pr commit** with fresh datadirs
and ports. Both runs independently pass every mandatory row; do not merge complementary
rows from failed runs.

Retain sanitized exact SHAs, OS/toolchain, role/build counts, TunnelData/TunnelGateway
counts, useful digests/lengths, pre/post active/pending/peer/resource baselines, and child
cleanup status.

## Failure, cancellation, restart, contention semantics

- malformed/unauthenticated builds fail before admission with no second-owner fallback;
- valid policy rejection is code 30 and still follows role-correct transformed routing;
- terminal outbound delivery rolls back a just-accepted registration;
- queue/resource denial is bounded; the harness never spins until success;
- previous-peer mismatch and replay fail closed;
- cancellation drains before owner exit;
- session close removes bounded peer mapping;
- restart begins from zero transit state;
- no per-cell task spawning or unbounded retry/queue is introduced.

## Compatibility / migration / dependencies

No persisted-state migration or new production dependency is expected. Reuse existing
Rust/shell/Python harness machinery and the exact-pinned i2pd cache.

No product default changes: transit remains disabled, RouterInfo/router.version unchanged,
no public listener/network or user-facing transit config, and no M12 floodfill behavior.

If reference selection needs a product-visible capability/version claim that is not already
truthful for the controlled runtime, stop rather than spoof it.

## Required focused regressions

Add/retain tests proving: real runtime `Ssu2InboundI2np` consumption; disabled ordinary
behavior; direct-build injection cannot satisfy external evidence; role-to-route mapping;
authenticated previous-peer Participant forwarding; one-shot fragmented OBEP delivery;
bounded IBGW multi-cell output; code-30 zero state; expiry secret/state removal; duplicate
receive-id fail-closed; TunnelData replay suppression; zero cancellation/session/restart
baselines; evidence parser rejection of forged/missing rows; exact-pin mismatch failure
before network startup.

## Exact verification floor

```text
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
```

Invoke all new focused tests explicitly. Counted external execution:

```text
bash tests/integration/m11-transit/run-i2pd.sh   # pass 1
bash tests/integration/m11-transit/run-i2pd.sh   # pass 2, same i2pr SHA
```

Closure additionally requires exact-implementation-SHA ordinary Actions green for Ubuntu
Quality, macOS Quality, MSRV, and Dependency policy, plus sanitized artifacts from both
manual external passes.

## Documentation updates on closure

Write `plans/closure/transit-tunnels/255-status.md`, update the transit roadmap/registry,
`specs/support.toml`, `specs/protocols/05-tunnels.md`, and the exact i2pd subset in
`specs/CONFORMANCE.md`. Do not broaden product advertisement claims.

## Acceptance criteria

Plan 255 closes `passed` only when all are directly evidenced:

1. clean exact i2pd 2.61.0 / `635b013a...` is unmodified;
2. no public reseed/network occurs;
3. counted builds enter through real `next_inbound()` -> enabled
   `TransitLiveOwner::handle_inbound`;
4. peer/link provenance is SSU2-authenticated;
5. stock i2pd creates and accepts i2pr OBEP, IBGW, and intermediate Participant tunnels;
6. each accepted role installs exactly one bounded registration;
7. Participant TunnelData transforms/forwards and reaches the next reference hop;
8. OBEP data produces exactly one role-correct semantic delivery;
9. IBGW TunnelGateway ingress emits bounded role-correct TunnelData;
10. replay produces no second delivery;
11. genuine stock-i2pd build receives code 30 under deterministic rejection with zero
    registration;
12. bandwidth-option disposition is truthful and not fabricated;
13. logical 600-second expiry removes state and drops later genuine data;
14. cancellation/session-close/restart return active/pending/peer/resource state to zero;
15. complete matrix passes twice on the same i2pr SHA with fresh datadirs;
16. fail-closed evidence checker is green in routine CI;
17. full workspace verification and exact-SHA ordinary CI are green;
18. no product default/capability/router.version/public-network change lands;
19. closure language is limited to exact-pinned i2pd experimental progression.

Only then may the unblock audit move M12 planning forward. Full two-family transit
conformance remains open.

## Stop conditions

Stop and register a narrow corrective/topology plan if: actual SSU2 `next_inbound`
cannot feed the controlled owner without ordinary product enablement; stock exact-pinned
i2pd cannot know/select the genuine signed i2pr RI through an unmodified bootstrap;
deterministic role placement needs patched i2pd or false RI claims; i2pd emits a current
short-build shape i2pr cannot parse; failure localizes to Plan-250/252 crypto/admission or
Plan-253/254 owner/data-plane semantics; role-correct delivery needs a second ad hoc
protocol stack; a new production dependency/public config/public network is proposed; or
source-lock evidence cannot identify the failing reference boundary.

Preserve the first failing message/state transition, exact topology, and sanitized
source-lock facts. Do not grow a new diagnostic harness around an unidentified reference
behavior.

## Closure evidence required

The closure record must include implementation/closure SHAs, exact reference proof,
topology/role-placement proof, requirement-to-evidence matrix, both same-SHA external
runs, local/full command outcomes, exact-head ordinary CI, resource/secret/privacy review,
deviations/limitations, findings by severity, M12 unblock audit, and explicit non-claims
for public transit/full two-family conformance. Raw reference logs/datadirs remain scratch;
retained evidence is sanitized counts/hashes/digests/state transitions.

## Handoff

Start with the real `next_inbound` gate and exact-pin peer-placement source lock, not with
TunnelData. Get one genuine accepted edge-role build before expanding to all three roles.

Reuse Plan-193 provisioning and extract test-only helpers instead of copying its large
Streaming driver. Ordinary product transit remaining disabled is intentional.
