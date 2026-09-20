# Plan 227 — M6 Java explicit one-hop client-tunnel corrective

Status: **registered-ready-m6-java-explicit-one-hop-client-tunnel-corrective**

## 1. Objective

Close the exact Plan-226 boundary:

```text
P226-BASELINE-B-ZERO-HOP-UNKNOWN
```

by replacing only the Java raw-reference helper's zero-hop client tunnel with
one genuine stock-Java one-hop client tunnel through controlled Router C,
using Java I2P's own public/debug I2CP `explicitPeers` option.

Then rerun the same frozen destination/reverse-send lane and determine whether
the exact target lookup proceeds past the zero-hop guard and reaches Router B.

This is a reference-harness corrective. It does not authorize an i2pr
production protocol change.

## 2. Research basis

### 2.1 Exact-pinned Java source

Reference authority remains Java I2P 2.13.0 at:

```text
9134f808337b401e8e53c73734c81fab04280c9d
```

Plan 226 proved the selected floodfill candidate is rejected in
`IterativeSearchJob.sendQuery()` before a DatabaseLookupMessage is created:

```java
if (outTunnel != null && outTunnel.getLength() <= 1) {
    if (_facade.lookupLocallyWithoutValidation(peer) == null) {
        failed(peer, false);
        // "not doing zero-hop lookup to unknown ..."
        return;
    }
}
```

The same pinned source establishes why this is structurally expected for the
current helper:

- client NetDBs share the main KBucket set;
- client NetDBs own a separate transient datastore;
- RouterInfo stores into a client NetDB are prohibited;
- the current raw helper explicitly requests
  `inbound.length=0` / `outbound.length=0`.

Therefore Router B can be selected from shared routing state while remaining
unknown to the client facade used by the zero-hop safety check.

### 2.2 Java I2P's own controlled-test mechanisms

Pinned and current Java I2P both expose `explicitPeers` in
`TunnelPeerSelector` for debugging/restricted-route testing.

For a non-exploratory client pool:

```java
Properties opts = settings.getUnknownOptions();
String peers = opts.getProperty("explicitPeers");
...
if (peers != null && ctx.random().nextInt(4) == 0)
    return true;
```

and `selectExplicit()` accepts an explicit peer when
`ProfileOrganizer.isSelectable(peer)` is true. It does **not** require the
peer to have reached Java's fast/high-capacity tier.

This matters because the old Plan-201 one-hop attempt failed specifically on
fresh-router fast/high-capacity promotion, not because Router C was an invalid
RouterInfo or because one-hop client tunnels are unsupported.

The option is carried through the ordinary public I2CP SessionConfig path:

```text
I2CP SessionConfig options
  -> CreateSessionJob
  -> ClientTunnelSettings.readFromProperties()
  -> inbound./outbound. TunnelPoolSettings
  -> unknown option "explicitPeers"
  -> ClientPeerSelector / TunnelPeerSelector
```

The public I2CP documentation also describes `explicitPeers` as a
comma-separated Base64 list of peers to build tunnels through, for debugging.

### 2.3 Upstream local-network testing modality

Java I2P's `MultiRouter` is the closest upstream analogue to the controlled
A/B/C topology. It:

- creates distinct RouterContexts;
- uses loopback addresses;
- sets `i2np.allowLocal=true`;
- notes that the normal transport implementation is preferred over
  `VMCommSystem` when UDP/TCP and related router behavior must actually be
  tested.

The current i2pr `ControlledRouter` already follows those constraints:
real SSU2, `i2np.allowLocal=true`, loopback-only transport, no public reseed,
and no VMComm.

Java I2P also documents local `buildTest` / `LocalClientManager` testing
for reproducible I2CP/Streaming work. That reinforces the design rule for this
plan: use stock client/session configuration and real router machinery instead
of mutating profile, NetDB, or tunnel state.

## 3. Why Plan 227 uses explicitPeers

A real non-zero-hop client tunnel is necessary to bypass the exact Plan-226
guard without changing Java code.

Using Router C through `explicitPeers` is narrower and more faithful than:

- manufacturing fast/high-capacity peer scores;
- copying Router B's RouterInfo into the client NetDB;
- installing a tunnel directly;
- patching the zero-hop guard;
- using `netDb.alwaysQuery`;
- enabling VMComm.

`explicitPeers` still requires:

- an ordinary valid RouterInfo in A's main NetDB;
- normal Java tunnel construction;
- normal SSU2 transport;
- normal tunnel build request/reply handling;
- normal client tunnel installation.

It changes peer choice only, through a supported SessionConfig option.

## 4. Current retained authority

Retain unchanged:

- Plan 223 Destination identity / LS2 separation;
- Plan 224 proof that Router B holds a current, validated,
  `receivedAsPublished=true`, query-answerable target LS2;
- Plan 225 exact helper-search attribution;
- Plan 226 exact target-job trace and
  `P226-BASELINE-B-ZERO-HOP-UNKNOWN`;
- Plan-222 production-equivalent selector evidence that B is a candidate;
- Java and i2pd reference pins;
- helper X25519/type-4 LS2 behavior;
- tracked reverse-send nonce semantics;
- frozen 45-second reverse-payload window.

Plan 226's distinct-loopback `127.0.1.1/127.0.2.1/127.0.3.1` branch remains
unadmitted and MUST NOT be enabled by Plan 227. Plan 226 proved IP-close
filtering was not the active boundary.

## 5. Invariants

1. No Java source patch.
2. No reflection or private-state mutation.
3. No profile score/tier mutation.
4. No RouterInfo insertion into a client NetDB.
5. No direct tunnel installation or fake tunnel object.
6. No VMComm.
7. No `netDb.alwaysQuery`.
8. No public I2P.
9. Java/i2pd pins unchanged.
10. No i2pr production protocol or crypto change.
11. Router A/B/C SSU2 hosts remain the Plan-226 baseline loopback topology.
12. `i2np.allowLocal=true` remains the upstream-style local-test setting.
13. Router B remains the target LS2 publication/floodfill role.
14. Router C is the sole explicit raw-helper tunnel peer.
15. SAM/I2CP/diagnostic endpoints remain loopback-only.
16. The raw helper is the only helper changed in this plan.
17. The Streaming helper remains frozen and is not executed for Plan 227.
18. The target Destination, LS2 publication path, crypto, search limits, and
    reverse-send payload stay frozen.
19. The reverse delivery window remains 45 seconds.
20. Raw Java logs remain scratch-only; durable evidence is whitelist-sanitized.
21. An explicit-peer tunnel is not considered proven merely because the option
    was configured. Installed live tunnel state must prove it.

## 6. Work package A — Router C eligibility preflight

Before starting the raw helper, prove Router A can legitimately use Router C
as a Java tunnel peer.

Add a bounded read-only diagnostic:

```text
P227-PEER-ELIGIBILITY <router-c-hex>
```

Return only typed facts:

```text
main_raw_present=<bool>
main_valid_present=<bool>
selectable=<bool>
established=<bool>
banlisted=<bool>
```

Implementation may use public/read-only accessors only:

- main NetDB local lookup;
- `ProfileOrganizer.isSelectable()`;
- comm-system established state;
- banlist read.

No profile creation, tier promotion, connection forcing, or NetDB store.

Required gate:

```text
main_raw_present=true
main_valid_present=true
selectable=true
```

`established` may be either value and is diagnostic only.

If this gate fails, stop:

```text
P227-C-NOT-SELECTABLE
```

Do not change the helper profile or compensate with another peer.

## 7. Work package B — exact Router-C I2P Base64 identity

`explicitPeers` consumes Java/I2P Base64 RouterHash text.

Do not reimplement the I2P Base64 alphabet in shell.

Reuse existing authoritative read-only diagnostics:

1. obtain Router C's exact 32-byte hash from its P220 self snapshot;
2. validate it as exactly 64 lowercase hex characters;
3. use the existing `P224-HASH-B64 <hash-hex>` renderer to obtain Java's
   exact I2P Base64 representation;
4. pass that exact value to the raw helper.

The value may appear in scratch process arguments/session options but durable
evidence should store only the Router-C hex hash and booleans proving option
identity/match.

## 8. Work package C — raw helper one-hop profile

Change only `ReferenceRawDestination.java` for the Plan-227 destination lane.

Add an explicit Router-C argument or environment input with strict validation.

The raw helper SessionConfig becomes:

```text
inbound.length=1
outbound.length=1
inbound.quantity=1
outbound.quantity=1
inbound.backupQuantity=0
outbound.backupQuantity=0
inbound.allowZeroHop=false
outbound.allowZeroHop=false
inbound.explicitPeers=<Router-C I2P Base64 hash>
outbound.explicitPeers=<Router-C I2P Base64 hash>

i2cp.leaseSetType=3
i2cp.leaseSetEncType=4
i2cp.dontPublishLeaseSet=false
i2cp.fastReceive=true
i2cp.messageReliability=BestEffort
```

Do not set router-global `explicitPeers`; scope it to this raw client's
inbound/outbound pools.

Do not alter `ReferenceStreamingService.java` in this plan.

### Upstream randomness

Pinned Java intentionally selects the explicit branch on only one in four
selection passes. Do not patch or seed Java randomness to defeat this.

Allow normal Java tunnel build retries.

The raw-helper readiness wait may be extended only to cover Java's own
five-minute `I2PSession.connect()` LeaseSet/tunnel ceiling. This readiness
budget is not the reverse-delivery acceptance window and MUST NOT alter the
frozen 45-second payload wait.

Record:

```text
helper_connect_elapsed_ms
helper_ready
helper_connect_timeout_seen
```

Do not retry the helper inside one counted run after Java returns its
five-minute failure.

## 9. Work package D — prove the installed client tunnels

Add a bounded read-only command:

```text
P227-CLIENT-TUNNELS <client-dbid-hex> <router-c-hex>
```

Resolve the live inbound and outbound client pools through public tunnel
manager accessors and inspect their installed tunnels read-only.

Return only:

```text
client_resolved=<bool>
inbound_pool_present=<bool>
outbound_pool_present=<bool>
inbound_tunnel_count=<n>
outbound_tunnel_count=<n>
inbound_exact_one_remote_hop_via_c=<bool>
outbound_exact_one_remote_hop_via_c=<bool>
inbound_zero_hop_present=<bool>
outbound_zero_hop_present=<bool>
```

For Java's TunnelInfo representation, local router membership counts in
`getLength()`; therefore one remote hop must be checked semantically as the
exact local+Router-C path, not by a hard-coded name alone.

Authoritative gate before reverse send:

```text
inbound_exact_one_remote_hop_via_c=true
outbound_exact_one_remote_hop_via_c=true
inbound_zero_hop_present=false
outbound_zero_hop_present=false
```

If helper connects but this gate does not hold:

```text
P227-EXPLICIT-ONE-HOP-NOT-BUILT
```

Stop before interpreting lookup behavior.

## 10. Work package E — scratch-log corroboration

Extend the targeted scratch logger only as needed for:

- `TunnelPeerSelector`;
- `ClientPeerSelector`;
- existing Plan-225/226 lookup classes.

Raw lines remain scratch-only.

Sanitize only bounded facts such as:

```text
explicit_peer_option_present
explicit_c_selection_observed
explicit_c_not_selectable_seen
client_build_success_seen
client_build_reject_seen
client_build_timeout_seen
```

Installed tunnel-pool state from Work Package D is authoritative. Log-derived
selection facts are corroborative only.

Do not promote peer lists, keys, tags, raw SessionConfig contents, or raw log
text to durable evidence.

## 11. Work package F — rerun the exact destination lookup

Execute destination only:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

Before tracked reverse send, retain:

1. Plan-227 Router-C eligibility;
2. exact one-hop client tunnel proof;
3. Plan-224 Router-B main-LS answerability;
4. Plan-222 selector includes B;
5. Plan-223 Destination/LS2 identity facts.

Then send the exact tracked reverse payload.

Required observation sequence:

```text
b_zero_hop_unknown_rejected=false
        ↓
query_to_b
        ↓
b_lookup_received
        ↓
b_published_ls_answered
        ↓
a_client_tunnel_ls_received
        ↓
A_CLIENT_AFTER_SEND.validated_present
        ↓
ordered_statuses
        ↓
frozen_payload_45s
```

No later stage may be used to infer an earlier one.

## 12. Terminal taxonomy

Exactly one final P227 terminal.

### Preflight/build stops

```text
P227-C-NOT-SELECTABLE
P227-EXPLICIT-ONE-HOP-NOT-BUILT
P227-OBSERVABILITY-GAP
```

### Contradictions

If one-hop tunnels are proven but the same zero-hop branch fires:

```text
P227-EVIDENCE-CONTRADICTION-ONE-HOP-BUT-ZERO-HOP-UNKNOWN
```

If A receives the target LS DSM on the helper tunnel but it is not installed:

```text
P227-EVIDENCE-CONTRADICTION-CLIENT-DSM-NOT-STORED
```

### New behavioral boundaries

One-hop is proven, zero-hop guard is absent, but B is never queried:

```text
P227-NEXT-BOUNDARY-B-NOT-QUERIED
```

A dispatches toward B but B never receives:

```text
P227-NEXT-BOUNDARY-A-TO-B-LOOKUP-DELIVERY
```

B answers but A does not observe the client-tunnel DSM:

```text
P227-NEXT-BOUNDARY-B-REPLY-TO-A-CLIENT-TUNNEL
```

Client DB becomes usable and status 21 is replaced by a new send terminal:

```text
P227-NEXT-BOUNDARY ordered_statuses=<...>
```

Digest-matched reverse payload arrives inside the unchanged 45-second window:

```text
P227-REVERSE-DELIVERY-PASSED
```

## 13. Static guards and focused tests

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` to require:

1. Plan-227 raw helper uses length 1 inbound/outbound;
2. Plan-227 raw helper disables zero-hop;
3. both explicit-peer options are scoped to the raw client SessionConfig;
4. explicit peer derives from Router C identity;
5. Streaming helper remains unchanged;
6. no router-global `explicitPeers`;
7. no `netDb.alwaysQuery`;
8. no profile/tier mutation;
9. no client-NetDB RI store;
10. no direct tunnel install;
11. no VMComm;
12. no Plan-226 distinct-loopback topology in a P227 counted run;
13. P227 eligibility diagnostic is read-only;
14. P227 tunnel snapshot is read-only;
15. installed one-hop tunnel proof is required before reverse-send
    interpretation;
16. 45-second reverse result remains frozen;
17. exactly one P227 terminal is emitted.

Focused Rust/test-driver coverage must include:

- C selectable / non-selectable classification;
- exact-C identity mismatch rejection;
- inbound-only tunnel is insufficient;
- outbound-only tunnel is insufficient;
- zero-hop fallback is insufficient;
- exact one-hop both directions via C passes the tunnel gate;
- one-hop + zero-hop warning classifies contradiction;
- one-hop + no B query classifies new boundary;
- B query/reply/client-store downstream stages;
- changed send status;
- digest-matched reverse delivery;
- secret/raw-log rejection.

## 14. Execution and retry policy

Commit all implementation before counted execution.

Require clean exact head:

```bash
git status --porcelain=v1
git rev-parse HEAD
```

Maximum three counted attempts per implementation SHA.

Each attempt must use:

- fresh disposable Java RouterContexts;
- the same A/B/C identities for that attempt;
- the same raw helper profile;
- the same Router-C explicit peer derivation;
- the same target/publication path;
- no tuning between retries.

If code/configuration changes, commit a new SHA before another counted
attempt.

The one-in-four explicit selection behavior is a valid reason for repeated
build attempts *inside Java's normal pool machinery*, not for mutating random
state or looping helper restarts.

## 15. Verification floor

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh

cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p225_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
```

## 16. Acceptance criteria

Plan 227 closes only if:

1. exact Java/i2pd pins remain frozen;
2. Router C is proven present/valid/selectable in A main NetDB before helper
   start;
3. the raw helper receives Router C through ordinary SessionConfig
   `explicitPeers`;
4. no profile/tier state is manufactured;
5. a real Java inbound one-hop client tunnel through C is installed;
6. a real Java outbound one-hop client tunnel through C is installed;
7. no helper zero-hop tunnel is accepted as qualification;
8. Router B remains answerable for the exact target LS2;
9. the exact Plan-226 target search is observable;
10. whether the zero-hop rejection disappears is recorded;
11. whether A sends the lookup to B is recorded;
12. B receive/answer and A client-tunnel/store stages are independently
    recorded when reached;
13. tracked send statuses are recorded;
14. the unchanged 45-second reverse digest result is recorded;
15. exactly one P227 terminal is emitted;
16. focused and routine verification passes;
17. closure status, registry, roadmap, and unblock audit land.

## 17. Closure disposition

Create/update:

```text
plans/closure/mixed-router-interop/227-status.md
```

Record:

- implementation SHA(s);
- Java/i2pd pins;
- Router-C eligibility facts;
- exact explicit-peer identity derivation;
- helper connect duration/result;
- installed inbound/outbound tunnel facts;
- B answerability;
- target-job trace;
- downstream lookup/reply/store stages;
- ordered statuses;
- 45-second digest result;
- attempt history;
- verification;
- final terminal;
- unblock audit.

If:

```text
P227-REVERSE-DELIVERY-PASSED
```

do not automatically claim full Java-family closure. Plan 201 becomes the
likely qualification owner for the remaining destination/Streaming matrix.

If a `NEXT-BOUNDARY` terminal occurs, register a new narrow successor based
only on that boundary.

If `P227-EXPLICIT-ONE-HOP-NOT-BUILT` occurs while C is selectable, the next
investigation is the real stock-Java tunnel build request/reply path—not peer
profile scoring.

## 18. Registration disposition

```text
plan_226 = passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary
plan_227 = registered-ready-m6-java-explicit-one-hop-client-tunnel-corrective

plan_201 = blocked-pending-plan227-explicit-one-hop-client-tunnel-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan227
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 227-m6-java-explicit-one-hop-client-tunnel-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```
