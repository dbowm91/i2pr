# Plan 221 — M6 Java client-NetDB/OCMOSJ narrowing

Status: **registered-ready-m6-java-client-netdb-ocmosj-narrowing**.

## 1. Objective

Narrow the Plan 220 `P220-OBSERVABILITY-GAP-CLIENT-NETDB` terminal
to the first exactly observed layer at or below the helper
client-message path.

Its only capability outcome is:

> one exact-clean-head destination-only run emits either a
> correctly observed `P221-*` boundary (client lookup vs OCMOSJ
> dispatch vs i2pr inbound) or a narrower typed observability gap
> at the first stage that still cannot be observed without
> violating the test constraints.

Plan 221 MUST NOT implement a topology, bootstrap, NetDB, tunnel,
SAM, or i2pr wire corrective. Only after this plan closes may a
later plan target the resulting boundary.

## 2. Why this narrowing is ready

Plan 220 closed the diagnostic/evidence corrective
(`passed-m6-java-plan219-diagnostic-attribution-corrective`, see
[`220-status.md`](../../closure/mixed-router-interop/220-status.md)):
on the authoritative run every Java main-NetDB stage is
`Known(pass)` at the post-bootstrap epoch (hash cross-check,
exact A-stored-B with current `f` RI, PeerManager indexing, live
selector containing B for the i2pr destination key), the helper
admits the reverse send, and no TunnelData reaches i2pr. The P220
tri-state classifier, the hex-hash identity path, the
same-package selector probe, and the §14 static guards are landed
and green on the routine floor.

Hard dependencies: Plan 220 closed. Interface dependencies: the
P220 evidence contract (`p220-classification` + supporting rows)
is stable and documented in the Plan 220 closure §4–§9.

## 3. Current implementation evidence

- `P220Facts` carries eight client-NetDB/OCMOSJ facts, all
  recorded `Unknown(no-read-only-per-message-observation)`:
  `client_lookup_started`, `client_lookup_peer_selected`,
  `client_lookup_succeeded`, `target_leaseset_present`,
  `target_lease_selected`, `outbound_client_tunnel_selected`,
  `garlic_constructed`, `tunnel_dispatch_submitted`.
- The derivation already orders CLIENT-NETDB before OCMOSJ before
  the dispatch tail, and the reverse-delivery fast path already
  proves the chain on digest-matched recovery — so any newly
  observed `Known` value flows into the existing terminal without
  restructuring.
- Permitted (not yet attempted) evidence per Plan 220 WP F:
  exact-pinned class-specific logs in the bounded send window
  with unambiguous correlation to the reverse send, read-only
  Java stats/counters with unambiguous fresh-run deltas, or
  helper status events. Forbidden: `sendMessage()==true` as
  lookup/dispatch success, forward `reference-received` as
  reverse proof, defaulted booleans, reflection/private fields.

## 4. Invariants

Plan 221 MUST preserve every Plan 220 invariant (exact pins
Java I2P 2.13.0 `9134f808337b401e8e53c73734c81fab04280c9d` +
i2pd 2.61.0 `635b013a612ff47278ef02acf8580a28e10e26c5`, no
public I2P, no Java source patch, no reflection or
private-state mutation, no NetDB/tunnel/LeaseSet injection, no
VMComm, no direct LS2 copying, no SAM helper rewrite, no
topology expansion, no new bootstrap behavior solely to make the
test pass, no i2pr production wire change, no support-claim
expansion, loopback-only bounded diagnostics, Plan 193 i2pd and
Plan 215 M10 authority untouched) plus: **unknown stays
unknown** — a correlated-but-ambiguous signal MUST narrow the
gap, never become a root-cause classification.

## 5. Scope

### In scope

- `crates/i2pr-daemon/tests/java_tunnel_external.rs` (P220 fact
  observation + terminal vocabulary extension);
- `tests/integration/m6-interop/run-java.sh` (bounded
  send-window log/counter correlation, if the narrowing uses
  it);
- `tests/integration/m6-interop/java/` helpers (status-event
  surface only; no wire change, no publication-path change);
- `scripts/check-m6-mixed-router-acceptance-evidence.sh`
  (P221 narrowing invariants);
- Plan 220/221 authority files.

### Explicitly out of scope

- any B→A/A→B bootstrap, floodfill-policy, publication-timing,
  or tunnel-pool change;
- any i2pr NetDB/Garlic/Streaming/SSU2/tunnel/LS2 wire change;
- Streaming requalification; final Java-family qualification;
  Plan 204 convergence; M11.

## 6. Ordered work packages

- **A — client-NetDB correlation.** Attempt the WP-F-permitted
  sources in order: (1) helper status events for
  lookup-started/peer/LS outcome; (2) read-only Java
  stats/counters with fresh-run deltas inverifiably tied to the
  reverse send window; (3) exact-pinned class-specific log lines
  in the bounded send window with unambiguous reverse-send
  correlation (helper ID + destination key + timestamp window
  all matching). First unambiguous source wins per fact; on
  conflict, the fact stays Unknown.
- **B — OCMOSJ correlation.** Same ladder for lease-selected /
  tunnel-selected / garlic-constructed / dispatch-submitted. If
  dispatch-submitted becomes `Known(true)` while i2pr observes
  no TunnelData, the narrowed boundary is the i2pr inbound leg;
  if dispatch stays Unknown while lookup is `Known(pass)`, the
  terminal stays `GAP-OCMOSJ` with the narrowed reason.
- **C — terminal vocabulary.** Extend `P220Terminal` only with
  `P221-*` tokens of the same two shapes
  (`P221-CORRECTED-ATTRIBUTION <boundary>`,
  `P221-OBSERVABILITY-GAP-<stage>`); the P220 tokens stay frozen
  history. A root cause fires only on every-earlier-`Known(pass)`
  + current-`Known(fail)`.
- **D — regression guards.** Extend the checker §14 block:
  require the new observation call sites, forbid correlation by
  coarse substring (exact class + window + key binding required),
  forbid promoting an ambiguous signal to `Known`.
- **E — exact-clean-head rerun.** Commit implementation first;
  record `git rev-parse HEAD` + empty porcelain; run
  `I2PR_M6_JAVA_DRIVER=destination bash
  tests/integration/m6-interop/run-java.sh`; exactly one
  terminal per run. No Streaming unless reverse delivery passes.

## 7. Failure / cancellation / restart semantics

Plan 220 §17 verbatim, plus: the known pre-epoch helper-LS2
publication race (Plan 220 closure §12 runs 2–4) may consume
repeats; budget them explicitly in the closure and never tune
windows/topology to avoid them. Missing correlation = Unknown;
ambiguous correlation = Unknown with the ambiguity recorded;
router exit = environment failure, never a protocol
classification.

## 8. Compatibility and migration

No user-facing or support change. Evidence authority only: Plan
221 governs the narrowed boundary; downstream work consumes Plan
221, never the Plan 220 gap directly.

## 9. Required tests

Focused rows must cover: unambiguous correlation flips
Unknown→Known; ambiguous correlation stays Unknown; conflicting
sources stay Unknown; earliest-`Known(fail)` still wins; forward
evidence still cannot satisfy reverse; the new `P221-*` tokens
are canonical. If a narrowed `Known(fail)` is observed, a row
must lock that its entire prefix is `Known(pass)`.

## 10. Verification commands

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh

cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run

test -z "$(git status --porcelain=v1)"
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

## 11. Acceptance criteria

Plan 221 closes only when the §6 diagnostic ladder yields one
`P221-*` terminal on an exact-clean-head run with every earlier
stage `Known(pass)` (or a narrower documented gap), guards cover
the new correlation paths, implementation precedes the run, the
run SHA is recorded exactly, no out-of-scope corrective leaked
in, and Plans 193/215 are untouched.

## 12. Stop conditions

Stop with the narrowed gap if no WP-F-permitted source yields
an unambiguous per-message observation. If the narrowing proves
OCMOSJ dispatch submitted with no i2pr TunnelData, stop and
register the i2pr-inbound corrective. If OCMOSJ never dispatches
despite a `Known(pass)` lookup, stop and register the Java
client-delivery corrective. If reverse delivery passes, stop and
register a fresh final-qualification plan.

## 13. Closure evidence required

`plans/closure/mixed-router-interop/221-status.md` must include:
implementation commits, clean-worktree proof, pins, the
correlation ladder with per-fact source-vs-ambiguity
disposition, the narrowed terminal, full verification
outcomes, findings by severity, and the unblock audit for Plans
201, 204, 205, 218, 220.

## 14. Handoff

```text
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective
plan_221 = registered-ready-m6-java-client-netdb-ocmosj-narrowing

plan_201 = blocked-pending-plan221-client-netdb-narrowing
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective

next_executable_plan = 221-m6-java-client-netdb-ocmosj-narrowing
```
