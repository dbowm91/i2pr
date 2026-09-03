# Plan 159 — Milestone 8 SSU2 path validation, publication, and transport selection

Status: **registered; execute after Plan 158 passes**.

Depends on Plan 158. Blocks Plan 160.

## 1. Goal

Add safe endpoint/path change handling, conservative external-address/reachability state, SSU2 RouterInfo publication snapshots, and deterministic transport selection/fallback across NTCP2 and SSU2 without conflating an observed UDP source with proven reachability.

## 2. Path-change threat model

An authenticated-looking packet arriving from a new endpoint is not sufficient by itself to migrate an SSU2 session.

Required state:

```text
current validated path
optional candidate path
path-challenge value/deadline
candidate MTU/congestion restriction
validation result
```

On a packet from a new endpoint:

- authenticate it against the existing session first;
- reject/replay-filter as normal;
- create at most one/few bounded candidate paths according to policy;
- issue the exact spec-defined PathChallenge;
- keep normal traffic on the current validated path while possible;
- only promote after the matching authenticated PathResponse/current spec proof;
- expire candidate state on deadline;
- never migrate based solely on source-IP change.

## 3. Path challenge/response

Implement typed Plan 157 block events into a runtime-neutral path-validation state machine driven by the runtime.

Requirements:

- challenge values generated with OS CSPRNG in production;
- challenge count per session and globally bounded;
- stale/replayed/wrong-value responses ignored/rejected safely;
- response to a challenge is sent only under the current spec's amplification/path rules;
- concurrent candidates cannot overwrite one another ambiguously;
- endpoint key includes family/address/port as required;
- candidate path sends are constrained to the protocol's minimum MTU and conservative congestion window until validation;
- current path remains usable during validation unless independently failed.

## 4. MTU/path state

Maintain explicit per-validated-path MTU state.

- enforce SSU2 minimum 1280;
- cap to protocol/UDP safe maximum;
- never increase MTU from an unauthenticated packet claim;
- candidate paths start conservative;
- path migration cannot retain stale in-flight byte accounting from a different endpoint incorrectly;
- fragmentation Plan 157 must use the current validated path's effective MTU.

No PMTU-discovery complexity beyond what current SSU2 requires for ordinary operation is necessary here.

## 5. Reachability observations

Reuse/extend the transport-neutral `ReachabilityObservation` model rather than storing publication policy inside SSU2 packet code.

SSU2 runtime may emit privacy-safe typed observations such as:

```text
local-configured-bind
authenticated-peer-observed-external-address
validated-path
peer-test-result        # Plan 160 later
relay/firewalled signal # Plan 160 later
```

Observation fields must be bounded and retain only endpoint information actually needed for operational/publication decisions. Snapshots exposed to general diagnostics should remain redacted/aggregate according to repository policy.

## 6. Reachability/publication state machine

Implement a conservative router-level policy owner outside the SSU2 codec.

At minimum distinguish:

```text
Unknown
ObservedUnconfirmed
CandidateReachable
Reachable
Firewalled
Unreachable
```

Exact names may differ.

Before Plan 160 peer-test evidence exists:

- one peer's external-address observation must not transition directly to `Reachable`;
- locally configured public endpoint may be treated according to explicit configuration policy, not inferred;
- contradictory observations reduce confidence/keep state unconfirmed;
- state transitions have expiry and minimum corroboration rules;
- publication consumes a snapshot, not mutable packet/session objects.

Plan 160 will feed peer-test/introducer evidence into this same state machine.

## 7. SSU2 RouterInfo publication snapshot

Create a deterministic validated SSU2 address-publication builder from:

```text
static SSU2 public key
intro key
version=2
validated local/configured endpoint
reachability state
MTU/caps
introducer list (empty until Plan160 unless configured evidence exists)
```

Rules:

- do not expose private static/intro key material;
- direct host/port only when policy says the router may claim direct reachability;
- firewalled form does not fabricate a direct address;
- introducers only included after Plan 160 creates validated live introducer records;
- output options canonical/deterministic;
- address expires/withdraws when underlying reachability evidence expires;
- publishing SSU2 must remain disabled unless daemon configuration explicitly opts in after the relevant milestone gate.

This plan may build/test RouterAddress snapshots without enabling actual network publication in production.

## 8. Shared transport selection policy

Extend the existing transport-neutral manager/policy layer so NTCP2 and SSU2 are selected deterministically.

Do not make the daemon ask protocol crates directly which transport to use.

Required selection order/policy inputs:

1. existing authenticated usable link to the peer;
2. candidate RouterAddresses structurally validated per transport;
3. per-transport dial backoff/health;
4. direct/reachable vs introducer-only status;
5. configured transport enablement;
6. deterministic tie-break policy;
7. peer-wide limits/duplicate-link policy.

Policy should support outcomes like:

```text
Reuse(link)
Dial { transport, address }
DialFallback { primary, secondary }
NoCompatibleAddress
BackedOff
ResourceDenied
```

Use concrete bounded data types consistent with current `i2pr-transport` style; do not add an async trait framework solely for selection.

## 9. Fallback semantics

Tests must prove:

- an active authenticated SSU2 link is reused for SSU2-capable peer delivery;
- an active authenticated NTCP2 link is reused rather than needlessly dialing SSU2;
- SSU2 dial failure/backoff may fall back to a valid NTCP2 candidate;
- NTCP2 failure may select SSU2 where valid;
- one transport's address-specific failure does not poison all peer transports unless a peer-wide authentication/policy reason justifies it;
- duplicate simultaneous NTCP2/SSU2 links resolve through existing generic duplicate/link policy rather than races in separate managers;
- deterministic inputs always choose the same candidate.

The historical NTCP2 transport remains experimental/non-advertised. This selection work must not silently activate it in the production daemon.

## 10. IPv4/IPv6 behavior

Structurally support separate IPv4/IPv6 path/address candidates.

Required tests:

- v4 current -> spoofed v4 candidate rejected without challenge completion;
- v4 current -> legitimate new v4 port/address validated and migrated;
- v6 structures round-trip and family mismatch fails;
- v4 packet cannot validate a v6 candidate or vice versa;
- candidate tables are bounded independently enough to resist family-based slot exhaustion.

Independent IPv6 router interop is not required to close this plan.

## 11. Spoof/migration fault matrix

Through real localhost UDP where feasible, prove:

- unauthenticated datagram from new endpoint does nothing;
- authenticated replay from old packet at new endpoint does not migrate;
- authenticated new packet creates only bounded candidate state;
- wrong PathResponse does not migrate;
- correct challenge/response migrates exactly once;
- candidate timeout retains/returns to old path;
- multiple spoofed candidate sources hit quotas without unbounded crypto/state;
- after valid migration, old path packets are handled according to current SSU2 rules rather than silently reopening migration.

Use a test UDP proxy/multiple localhost sockets if necessary; no Linux namespaces are needed.

## 12. Documentation/support

Update:

```text
docs/architecture/i2pr-transport.md
docs/architecture/i2pr-transport-ssu2.md
docs/architecture/i2pr-runtime.md
specs/protocols/09-ssu2.md
specs/support.toml
plans/159-status.md
```

Clearly distinguish:

```text
address parsed
path authenticated
reachability observed
reachability confirmed
RouterInfo address built
RouterInfo publicly advertised
```

Passing one state must not be documented as passing all later states.

## 13. Non-goals

No:

- PeerTest three-party protocol (Plan 160);
- relay/introducer negotiation (Plan 160);
- public-network RouterInfo publication;
- external-router SSU2 interop (Plan 161);
- exotic multipath/multihoming beyond ordinary spec-required migration.

## 14. Validation

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
```

## 15. Acceptance criteria

Plan 159 passes only when:

1. endpoint changes require authenticated spec-defined path validation.
2. candidate path state/count/time/bytes are bounded.
3. wrong/replayed/stale path responses cannot migrate a session.
4. legitimate real-UDP path migration is proven.
5. candidate path MTU/cwnd stay conservative until validation.
6. external-address observations are typed and separate from publication authority.
7. one unauthenticated/single-peer observation cannot create `Reachable` publication state.
8. SSU2 RouterAddress publication snapshots are deterministic and policy-gated.
9. direct addresses withdraw when supporting evidence expires.
10. generic transport selection includes SSU2 without introducing a second manager.
11. existing authenticated link reuse takes precedence over needless redial.
12. NTCP2/SSU2 failure/backoff/fallback semantics are deterministic and tested.
13. one transport failure does not incorrectly poison the other.
14. IPv4/IPv6 candidate states are structurally separated.
15. no public advertising/interop claim is introduced.
16. full workspace/SSU2 quality floor passes.
17. `plans/159-status.md` advances only to Plan 160.

## 16. Stop conditions

Stop and narrow if:

- correct migration semantics are ambiguous between the current spec and deployed references;
- selection requires changing generic duplicate-link policy in a way that invalidates NTCP2 evidence;
- publication state would require direct NetDB mutation from the SSU2 runtime;
- spoof tests show candidate-path state/crypto work grows with source count beyond explicit quotas.

Do not solve reachability uncertainty by publishing optimistically.