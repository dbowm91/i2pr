# Plan 306 status: stopped

- Plan: `plans/implementation/floodfill/306-m12-loopback-reachability-caps-and-java-requalification.md`
- Closure token: `stopped-m12-java-requires-bandwidth-tier-beyond-reviewed-fR`
- Corrective required via: none registered. The stop is retained; the
  reopen condition is stated in §8 (a future plan with a truthful
  bandwidth-class design plus review — fabricating a tier letter is
  forbidden, so no executable narrow step exists now).
- Stop classification: reference-lane selection boundary under the
  Plan 101 no-false-advertisement posture (as anticipated by Plan 306
  §3/§6/§12 and ADR 0030), not an i2pr wire defect and not a
  reference-unavailability finding. Fresh Java budget (`MAX_ATTEMPTS=3`)
  is spent 2-of-3; the third attempt is deliberately unspent (no
  in-bounds delta exists — the only remaining lever is a fabricated
  tier letter). Normal-daemon caps advertisement stays unavailable
  and ADR 0026 (second independent family) stands unwaived.

## 1. What was executed

Work packages 1–4 of Plan 306, in order, on the exact-pinned
references (i2pd `2.61.0` at `635b013a...`, Java I2P `2.13.0` at
`9134f808...`; pins verified by the lanes before any process starts;
loopback-only, reseed disabled, no public network, no reference
patching/vendoring/rebuilding):

- WP1a — design decision ADR 0030
  (`docs/adr/0030-loopback-controlled-floodfill-reachability-advertisement.md`):
  `R` is TRUE in controlled/loopback scope iff the controlled
  peer-test exchange Confirmed inbound SSU2 acceptance of the exact
  bound address being advertised; tiers and all other letters stay
  forbidden; normal path unchanged; no config surface can inject `R`.
- WP1b — implementation: opaque `LoopbackReachabilityProof`
  (mint confined to the controlled activation owner by the extended
  `scripts/check-m12-floodfill-boundaries.sh`), proof-gated
  `build_floodfill_reachable` (emits exactly `fR`;
  `validate_options` still rejects `R`/tiers in caller bytes),
  activation threading with the `Option` as the evidence switch
  (`Some` on the controlled path beside `Confirmed`, `None` on the
  normal path). Unit tests: exact-`fR` admission, smuggled-letter
  rejection on both builders, plain-`f` pinning. Commit `dc691cb`.
- WP2 — full routine floor on the implementation head (§6).
- WP3 — i2pd requalification, fresh frozen budget 1: **passed** on
  the first attempt. Controlled activation installed a 681-byte `fR`
  record; the full matrix went green; the static checker passes on
  the fresh evidence (`published-routerinfo-caps-f  caps=fR`).
  No regression vs Plan 303; i2pd is address-driven and ignores the
  added letter, as predicted.
- WP4 — Java requalification, fresh frozen budget 3, spent 2:
  attempt 1 stopped at JC SAM establishment (final CREATE failure
  only); recorded delta (commit `01805bc`): per-attempt SAM classes
  to `sam-attempts.log` plus `I2PR_M12_KEEP_SCRATCH_ON_FAIL=1`
  scratch preservation (diagnostic-only, no behavior/budget/row
  change). Attempt 2 stopped at the same gate with full diagnosis
  (§2). Out-of-lane probes (no budget consumed): J219-D3/D4
  read-only classification probes against a diagnostic JC seeded
  with the byte-identical lane RI.

## 2. Where it stopped

Controlled activation **passes** on both Java attempts (phase 0
installs the 681-byte `caps=fR` RouterInfo; P signals `p-live`).
The lane then stops at the SAM holder in both attempts: the 300 s
holder never observes `RESULT=OK`, zero matrix rows are claimed.

Attempt log (fresh budget 3, each with a recorded delta):

1. Stopped at JC SAM establishment; only the final CREATE failure
   recorded. Delta: per-attempt classes + keep-scratch (commit
   `01805bc`).
2. Same gate, fully diagnosed: holder classes `connect-failed` ×24
   (SAM bridge not yet up) + `attempt-failed` ×8 (connected, CREATE
   accepted, tunnel-build wait, holder gave up). JC's preserved
   router log (174 lines) shows SSU2 establishment to both relays
   within 1 s of final start, 8 `Failed to start SAM session`
   errors (`I2PSessionImpl.connect` waiting on client tunnels,
   `3:55:03`–`3:57+`), clean shutdown at trap time — and **zero**
   lines mentioning P (`cWDP...`, `grep -c` = 0) over the whole run.
3. Unspent: no in-bounds delta exists (see §3).

J219-D3/D4 (diagnostic JC, byte-identical fR RI, no budget):

```text
J219-EV kind=stored-ri ... present=true ...
J219-EV kind=peers-floodfill count=1 peer_0=cWDP...= truncated=false
J219-EV kind=capabilities ... stored=true ... capabilities="fR"
  has_floodfill_capability=true bandwidth_tier=Unknown ...
```

## 3. The exact boundary

Stock Java 2.13.0 loads, verifies, **lists P as its sole floodfill
peer**, parses `fR` (`has_floodfill_capability=true`) — and still
never initiates transport to P. Three independent zeros:

- attempt-2 lane: zero P mentions in JC's 174-line log (relays
  appear in second 1); holder `attempt-failed` ×8 all on the
  tunnel-build wait, never on a P-contact error;
- the lane driver TSV never advances past `publisher-rendezvous-live`
  (no `publisher-store-*`, no lookups);
- JC builds no client tunnels at all: with P its only known
  floodfill and P never selected, exploration has nothing to use,
  so every SESSION CREATE waits forever.

The discriminator is now exactly one letter: the bandwidth tier
(`Unknown`, where the working Java peer reports `L`). R unlocked
everything R can unlock (listing + floodfill-capability parsing);
selection still withholds on the tier, which derives from the same
forbidden cap letters. Advertising any tier (`L/M/N/...`) would
assert a measured capacity class with no measurement behind it —
a fabrication, explicitly forbidden by Plan 306 §3 and ADR 0030.
Per Plan 306 §6/§12, the plan STOPS here instead of iterating
letters.

## 4. What was proven and what was ruled out

1. **The R mechanism works as designed.** The `fR` record is
   truthful (peer-test-Confirmed inbound acceptance of the exact
   advertised address), i2pd-accepted (WP3 requal green), and
   Java-parsed (`has_floodfill_capability=true`, floodfill-listed).
   The proof/permit gates hold: no config path can mint either
   token (boundary-script-confined), withdrawal still emits no caps
   letters, the normal path still builds `f`.
2. **R is insufficient and the insufficiency is localized.** The
   before/after pair is exact: caps `f` → zero initiation (Plan
   279); caps `fR` → listed + capability-parsed, still zero
   initiation, tier still `Unknown`. No other content is missing
   from the record (address entry complete and i2pd-dialable;
   `netId`/`router.version` gates satisfied per Plans 284/285).
3. **No i2pr defect is evidenced.** P received zero packets from
   JC, so no i2pr wire behavior toward Java was exercised. P's
   acceptance of inbound SSU2 and its lookup/store answers are
   proven by the passing i2pd requal on the identical record.
4. **The holder classes are explained, not anomalous.**
   `connect-failed` ×24 = SAM bridge still starting across JC's
   final-start window; `attempt-failed` ×8 = CREATE accepted but
   tunnel construction impossible without a selected floodfill
   (8 JC-side `Failed to start SAM session` errors match).
5. **A tier letter cannot be supplied truthfully in lane scope.**
   Java tiers encode configured bandwidth class, not a measurement
   the lane could take; any letter would be invented. This is a
   policy boundary, not a measurement gap.

## 5. Security and resource review

- No secret, private key, SSU2 static/intro key, or destination key
  material reached evidence. Evidence rows carry counts, digests,
  lengths, and categorical outcomes only; the J219 replies recorded
  above carry hashes, caps strings, and tier labels only.
  Reference logs were diagnostic-only; the preserved attempt-2
  scratch (owner-only `/tmp`, reference keys inside) is removed at
  closure — nothing in this record depends on it.
- No production wire change was made to go green. No guard,
  boundary script, budget, or test-selection rule was weakened:
  the boundary script was extended with strictly stronger gates
  (proof-mint confinement, dual-token builder signature, normal-path
  `None` pin).
- The `fR` surface is controlled-path-only, permit+proof-gated,
  and loopback-scoped; eligibility loss still withdraws the whole
  advertisement. It authorizes no normal-daemon advertisement.
- Bounded throughout: frozen budgets (i2pd 1 spent-passed, Java 3
  spent 2 with 1 deliberately unspent), per-step deadlines (300 s
  holder, 600 s rendezvous, 900 s driver timeout), owned PIDs with
  trap teardown (verified: all lane ports free after each attempt;
  JC shut down cleanly at the trap in attempt 2). No OOM events
  this session (10 GB available; one JVM set at a time).
- Standing operational note (from Plan 279, not re-observed):
  a JVM once outlived the EXIT trap; the keep-scratch flag aids
  any recurrence.

## 6. Local verification on this head

- `cargo fmt --all --check` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` —
  110 suites, 3274 passed, 0 failed, 34 ignored (ordinary
  `#[ignore]`-gated external lanes skip), on the implementation
  head `dc691cb` (re-verified `cargo check` + `i2pr-daemon --lib`
  after the doc-only registry/roadmap text that followed).
- `cargo clippy --locked --workspace --all-targets --all-features --
  -D warnings` — clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`,
  `cargo test --locked --workspace --doc` — clean.
- Boundary/evidence scripts — all pass, including the extended
  `check-m12-floodfill-boundaries.sh` and
  `check-m12-floodfill-qualification-evidence.sh` (plus
  `--self-test`).
- `cargo deny check advisories bans sources` — clean; `python3 -m
  unittest discover -s tests/integration/ntcp2/harness` — 18 OK.
- Lane commands (loopback-only; budgets in §§1–2):
  `bash tests/integration/floodfill/run-i2pd.sh` — passed, first
  attempt, 681-byte `fR` record, evidence at
  `target/interop/m12-floodfill-evidence`;
  `bash tests/integration/floodfill/run-java-floodfill.sh` × 2 —
  both stop at the SAM holder as recorded (logs
  `/tmp/opencode/plan306-java-attempt{1,2}.log`).
- Not run (recorded, not waived): the third Java attempt
  (deliberately unspent, see §2), macOS CI, exact-head hosted CI.
- `specs/support.toml` refreshed exactly to what was qualified
  (§7): one-family line now cites the 306 fR requal; second-family
  stays unmet; `m12_caps_f_advertised` stays false. No
  `CONFORMANCE.md` change (no new conformance tier reached).

## 7. Requirement disposition (Plan 306 §11)

1. i2pd requal passes — MET (first fresh-budget attempt, fR
   record, checker green).
2. Java matrix passes all guarded rows — UNMET (this record; zero
   rows reachable, tier-selection boundary).
3. Caps policy reviewed and minimal — MET (ADR 0030: R-only,
   proof-gated, tiers forbidden; boundary script confines both
   mint sites; normal path pinned `f`).
4. No critical/high finding remains — MET (no i2pr defect found;
   the blocking finding is a reference selection boundary).

Roadmap disposition: **stopped**. M12 stays open; Plans 270–276,
281, 283, 302, 303 keep their tokens (303's one-family line is
additionally confirmed by the 306 requal); 277/278/280/282 stay
stopped; 284 stays ready; 285 stays retained; ADR 0030 stands as
reviewed controlled-scope policy. No historical record rewritten.

## 8. Unblock audit

- No new plan is registered with this closure: no bounded,
  executable next step exists that stays inside the
  no-false-advertisement posture. A future plan may reopen the
  second-family gate ONLY with a truthful bandwidth-class design
  (a genuine measurement or configured-class semantic plus
  review) — inventing a tier letter is not a plan, it is a
  violation. Until then the stop is retained and M12 stays open.
- Broad normal-daemon caps advertisement stays blocked (via
  ADR 0026). Nothing else unblocks: no plan lists 306 as its sole
  remaining dependency.
- The unspent third Java budget (1-of-3) is released, not banked:
  spent budgets are per-plan and frozen; a future plan brings its
  own fresh budget.

## 9. Findings by severity

- **Blocking:** stock Java 2.13.0 withholds all initiation toward
  the controlled RI until it carries a bandwidth-tier letter, and
  every tier letter is forbidden as fabrication under the standing
  posture. Owned by a future truthful-bandwidth-class plan, if any.
  No workaround exists inside lane bounds.
- **Medium:** JC's SAM bridge is unavailable for much of the
  final-start window (`connect-failed` ×24 before first accept);
  diagnosis needed the new per-attempt log. Any future lane should
  gate the holder on SAM-bridge readiness, not only on router-info
  publication.
- **Low:** the J219 diagnostic server answers one command per
  connection (later commands on the same socket see a closed
  stream); the D4 probe script documents the one-connection-per-
  command pattern.
- No critical/high i2pr defect found. No anonymity/privacy claim
  made or implied.
