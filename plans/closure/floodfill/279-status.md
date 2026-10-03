# Plan 279 status: stopped

- Plan: `plans/implementation/floodfill/279-m12-second-family-qualification-and-activation.md`
- Closure token: `stopped-m12-java-never-initiates-to-caps-f-only-controlled-ri`
- Corrective required via: Plan 306 (loopback reachability-caps design + Java
  requalification under a fresh frozen budget), registered `ready` in this commit.
- Stop classification: reference-lane boundary under the Plan 101
  no-false-advertisement posture, not an i2pr wire defect and not a
  reference-unavailability finding. The frozen Java budget (`MAX_ATTEMPTS=3`)
  is spent; no further Java attempt may run under this plan. Normal-daemon
  `caps=f` stays unavailable and ADR 0026 (second independent family) stands
  unwaived.

## 1. What was executed

The Plan 279 lane was built and executed three bounded times against the
exact-pinned stock Java I2P 2.13.0 at
`9134f808337b401e8e53c73734c81fab04280c9d` (cache built by
`scripts/interop/fetch-m6-java.sh`, verified by pin before any process
starts), on an unprivileged loopback-only mesh with reseed disabled and no
public network. No Java source was patched, vendored, or rebuilt; the
out-of-tree test-only `ControlledRouter` launcher (M6 precedent) injects
only loopback properties and per-role `floodfillParticipant` flags.

Landed artifacts (production-clean except the §4 feature below, no guard
weakened):

- `tests/integration/floodfill/run-java-floodfill.sh` — pin-verified lane.
  Frozen attempt budget of 3, per-attempt fresh datadirs, PID-file
  lifecycle, exact file-layout netDb seeding, required-failure exit,
  sanitized evidence schema `i2pr-m12-floodfill-qualification-v1`.
- `crates/i2pr-daemon/tests/floodfill_i2pd_external.rs` — the shared
  `qualify_matrix` (same rows both families face), the lane-neutral
  `FLOODFILL_REF_*` inputs, the stored-record family census, the
  `floodfill_qualify_against_java` / `floodfill_withdraw_against_java`
  entries, the `FLOODFILL_PUBLISHER_SYNC=1`-gated p-live/publisher-ready
  file rendezvous (fail-closed; the proven i2pd lane sets no such env and
  is byte-identical), and the test-only `floodfill_ident_of` printer.
- `scripts/check-m12-floodfill-qualification-evidence.sh` — extended to the
  Java lane (12 guarded rows, frozen budget 3, Java pin/launcher tokens,
  no-patching gate) with a passing `--self-test`.
- Plan 279 §4 normal opt-in activation (`[floodfill].enabled`, default
  false; eligibility-gated `caps=f`; withdrawal/drain semantics) with its
  unit/integration tests. Disposition: retained as tested default-off
  infrastructure (see §7); it is not a capability claim and changes no
  default behavior.

Topology (final): two stock `floodfillParticipant=true` reference peers
(J1/J2, replication targets), one stock transit reference client (JC) whose
netDb holds exactly one floodfill (i2pr) so its NetDB selection is
deterministic, two stock transit relays (JD1/JD2, full-meshed with JC, no
`f` caps so neither can become a NetDB target), and the i2pr controlled
floodfill on a fixed loopback port. JC hosts one lane-local TRANSIENT SAM
destination (key material never logged, only its RESULT) so a
LeaseSet-family publication exercises matrix B.

## 2. Where it stopped

Controlled activation **passes** on all three attempts (phase 0 installs a
680-byte `caps=f` RouterInfo; the qualify driver re-activates to `Active`
and signals `p-live`). The lane then stops at the publisher rendezvous on
every attempt: the driver waits the full 600 s bound and panics
fail-closed (`reference publisher never became ready`); zero matrix rows
are claimed passed.

Attempt log (frozen budget 3, each with a recorded delta — no blind re-run):

1. `i2cp=0` starved the SAM bridge: HELLO answered locally, every SESSION
   CREATE hung (bridge reached `i2cp.tcp.port=0`). Finding recorded in the
   runner; delta: JC binds a real loopback I2CP port.
2. Rendezvous + relay mesh + I2CP port. P signalled live; 300 s of SESSION
   CREATE attempts all hung. Driver TSV proves JC sent zero packets to P.
   Delta: out-of-lane probes (no budget consumed) isolated the recipe —
   one relay is insufficient, two mutually-seeded relays still fail with no
   live floodfill in view, adding a live Java floodfill completes creation
   in 1.4 s — plus the p-live-gated final-start.
3. JC final-started into the live mesh (P live from JC's first contact).
   300 s of CREATE attempts all hung again; driver TSV again shows zero
   inbound from JC. Budget spent.

## 3. The exact boundary

Stock Java 2.13.0 never initiates transport to the controlled RouterInfo.
Three independent zeros, all fail-closed:

- attempt-2/3 driver TSVs: `publisher-rendezvous-live` is the last row;
  no `publisher-store-*`, no `lookup-*`, no other inbound rows over 15+
  minutes each with P live;
- a 90 s Python UDP listener on the seeded endpoint observed zero bytes;
- no JC log line references P (no NTP/contact entry), while relay contacts
  appear within 1 s of start.

What Java thinks of P (J219 read-only probe against a diagnostic JC seeded
with the byte-identical lane RI):

```text
J219-EV kind=capabilities ... capabilities="f" has_floodfill_capability=true bandwidth_tier=Unknown ...
P220-EV kind=peers-floodfill count=1 ...
```

P is loaded, signature-verified, and listed as a floodfill peer — but never
dialed. The discriminating content, confirmed by dumping the exact
published bytes (`target/interop` diagstate copy, 680 bytes):

- router-level `caps` = `"f"` only — no `R` (reachable), no bandwidth tier.
  `crates/i2pr-netdb/src/local.rs:236-244` (`validate_options`, Plan 101
  authority) forbids `f B K L M N P R S U X` in controlled options, so a
  controlled record can never carry reachability or tier letters;
  `build_floodfill` appends `f` to whatever the activation supplies, and
  the loopback activation supplies nothing else.
- address-level entry is complete (style `SSU2`, host/port/`v=2`/`s`/`i`,
  address `caps=4`, `mtu=1280`), so the address is dialable — i2pd proves
  it by dialling it in the passing one-family lane. Router options carry
  `netId=2` and `router.version=0.9.62` (the Plan 284/285 gates, satisfied),
  so reachability/tier letters are the only missing content.

Reference-side gates that consume these letters (exact-pinned source):

- `TunnelPeerSelector.allowAsIBGW` requires the `R` capability
  (`router/java/src/net/i2p/router/tunnel/pool/TunnelPeerSelector.java:301-309`);
- bandwidth tier derives from the same forbidden cap letters (hence
  `Unknown`, where the working Java peer reports `L`).

i2pd is address-driven and ignores the missing letters; stock Java selects
on caps and withholds all initiation. The i2pd lane passing with the
identical publication proves the publication is conformant and dialable —
this is a caps-policy interaction under the Plan 101 posture, not an i2pr
wire defect.

## 4. What was proven and what was ruled out

1. **The seeding mechanism is correct.** NTP/contact lines prove JC loads
   seeded RIs and establishes SSU2 to seeded relays within 1 s; J219 shows
   the seeded P record stored, verified, and floodfill-listed. The
   discriminator is inside selection, not netDb handling.
2. **I2CP starvation is fixed and closed.** Attempt 1's `i2cp=0` root cause
   (bridge answers HELLO locally, CREATE needs the router I2CP server) is
   fixed by a real loopback I2CP port; HELLO answers instantly on all later
   attempts.
3. **The tunnel recipe is known.** Probes proved stock tunnel building
   needs working exploration via a live floodfill plus mutually-seeded
   relays (multi-hop build records must forward through hops that know the
   next hop). The lane encodes the full recipe.
4. **Profile poisoning is ruled out.** A fresh-datadir diagnostic JC (never
   started before, no profiles) with P seeded still sends zero bytes in
   90 s. First-contact timing (attempt 3) changes nothing.
5. **The signature is not the discriminator.** Seeded records verify
   (J219 `stored=true`); a deliberately caps-tampered copy is rejected at
   load (`stored=false`), confirming Java verifies seeded files and the
   lane's files are genuine.
6. **No i2pr defect is evidenced.** P received zero packets from JC, so no
   i2pr wire behavior toward Java was exercised. P's acceptance of inbound
   SSU2 and its lookup/store answers are proven by the passing i2pd lane
   on the identical publication path.

## 5. Security and resource review

- No secret, private key, SSU2 static/intro key, or `router.keys` material
  reached evidence. The Java DESTINATION reply (key material) is never
  logged; only its RESULT is recorded. Attempt state is owner-only;
  reference datadirs stay in ephemeral scratch and are removed by the EXIT
  trap (verified: all lane ports free after each attempt).
- Evidence rows carry counts, digests, lengths, and categorical outcomes
  only. Reference logs are diagnostic-only, never matrix evidence.
- No production wire change was made to go green. No guard, boundary
  script, budget, or test-selection rule was weakened. `check-*-evidence`
  passes including `--self-test`; the i2pd harness still passes it
  unchanged.
- The §4 `floodfill.enabled` surface is default-false, permit-gated, and
  loopback-scoped; it cannot advertise `f` without eligibility and is
  covered by its own tests. It authorizes no advertisement.
- Bounded throughout: frozen budget 3 (spent, see §2), per-step deadlines
  (600 s rendezvous, 300 s holder, 900 s driver timeout), owned PIDs with
  trap teardown. One residual: a JVM once outlived the trap (~1 min, port
  held); the next lane start would have failed closed on bind. No
  recurrence after `stop_java` discipline.
- Earlier this evening the host OOM-killed two processes (21:25/21:29, five
  JVMs plus a concurrent cargo build). Lane attempts ran one JVM set at a
  time afterwards with 10 GB available; no OOM during attempts 2–3.

## 6. Local verification on this head

- `cargo fmt --all --check` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` —
  110 suites, 3271 passed, 0 failed, 34 ignored (ordinary `#[ignore]`-gated
  external lanes skip), on the closing head.
- `cargo clippy --locked --workspace --all-targets --all-features --
  -D warnings` — clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`,
  `cargo test --locked --workspace --doc` — clean, including one fix found
  by this floor: a broken intra-doc link to `FloodfillEligibilitySnapshot`
  in `config.rs` (left over from the §4 commit) now cites the full
  `i2pr_netdb::` path.
- Boundary/evidence scripts — all pass: dependency-direction,
  runtime-boundaries, service-tunnel-boundaries, fixture-manifest,
  ntcp2/ssu2/i2cp vectors, ntcp2-interoperability,
  constrained-host-lane-boundary, m11-transit-boundaries,
  m11-transit-qualification-evidence, sam/ssu2/i2cp/service-tunnel/
  exploratory/netdb/destination/streaming/m6 acceptance-evidence,
  m12-floodfill-boundaries, m12-floodfill-qualification-evidence
  (plus `--self-test`).
- `cargo deny check advisories bans sources` — clean; `python3 -m unittest
  discover -s tests/integration/ntcp2/harness` — 18 tests OK.
- Not run (recorded, not waived): the i2pd/Java external lanes (frozen
  budgets belong to Plan 306), macOS CI, exact-head hosted CI.
- `bash scripts/check-dependency-direction.sh`,
  `bash scripts/check-runtime-boundaries.sh`,
  `bash scripts/check-m12-floodfill-boundaries.sh`,
  `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test`
  — all pass.
- Lane commands (local, loopback-only; budgets in §2):
  `bash tests/integration/floodfill/run-java-floodfill.sh` × 3 — all stop
  at the publisher rendezvous as recorded; `REFERENCE` pins verified
  before every attempt.
- The Plan 303 i2pd requalification on the final head (Plan 279 §12
  criterion 1, frozen budget 1) is NOT run here: this head adds production
  code (Plan 279 §4) after the 303 head, so the one-family pass is stale
  by the §13 material-difference rule. The requal is owned by Plan 306 on
  its closing head.

## 7. Requirement disposition (Plan 279 §12)

1. i2pd Plan 278/303 remains passed — STALE (see §6, owned by Plan 306).
2. Independent Java-family matrix — UNMET (this record).
3. Normal config default-off, cannot bypass eligibility — CODE PRESENT and
   unit-tested, but UNPROVEN against a reference (§4 retained as
   infrastructure, not capability).
4. `caps=f` follows Active/withdrawn — same as 3.
5. Private mixed-router acceptance — UNMET (blocked on 2).
6–10. Validation/bounds/persistence/CI/criticals — UNMET as a milestone;
   the components that exist are covered by their own suites (see §6).

Roadmap disposition: **stopped**. M12 stays open; Plans 270–276, 281, 283,
302, 303 keep their tokens; 277/278/280/282 stay stopped; 284 stays ready;
285 stays retained. No historical record rewritten.

## 8. Unblock audit

- Plan 306 (`306-m12-loopback-reachability-caps-and-java-requalification`)
  is registered `ready` in this commit and owns: the design decision of
  what a loopback controlled RI may truthfully advertise (reachability
  letter and nothing else; tiers stay forbidden), its implementation
  behind the existing eligibility/permit gates with review, the i2pd
  requalification on the closing head, and the Java requalification under
  a fresh frozen budget. It must not widen this plan's spent budget.
- Broad normal-daemon `caps=f` and M12 closure stay blocked on Plan 306
  (via ADR 0026). Nothing else unblocks: no plan lists 279 as its sole
  remaining dependency.
- Plan 278 stays stopped (its line is independent of this record).

## 9. Findings by severity

- **Blocking:** stock Java 2.13.0 withholds all initiation toward a
  Plan-101-compliant controlled RI (caps `f`, no `R`/tier). Owned by Plan
  306. No workaround exists inside lane bounds: changing production caps
  policy to go green is forbidden without a defect finding and review,
  and seeding caps the publication does not carry would misrepresent it.
- **Medium:** JVM teardown can briefly outlive the EXIT trap (one stale
  router held a fixed port ~1 min). Next-lane bind fails closed, so this
  is operational, not evidence, risk. Plan 306 should poll ports free
  before phase 0.
- **Low:** the runner's holder logs only the final CREATE failure, not
  per-attempt outcomes; diagnosis needed the reference log. Plan 306
  should record per-attempt classes.
- No critical/high i2pr defect found. No anonymity/privacy claim made or
  implied.
