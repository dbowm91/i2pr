# Plan 533 — the router must establish a session with the peer it reached through the relay

**Closure record.** The plan of record is
`plans/implementation/network-participation/533-the-router-must-establish-a-session-with-the-peer-it-reached-through-the-relay.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 533 is **closed**. The requester no longer stops at a hole punch: it
**establishes an authenticated session** with the peer it reached through the
relay, and the session is the runtime's ordinary SSU2 establishment with the
target's own token.

Classification: **corrective** (the machinery existed and nothing used it: the
transport's responder already consumed a presented token, the runtime already
held the responder's token table, and the transport's own documentation already
named the intended design — but the daemon drew a token no table held, refused
an early hole punch instead of holding it, and dialled nothing at all).
Plan 490, 499 and 524 delivered the **introducer** half; Plan 531 delivered the
**requester** half; Plan 532 delivered the **target** half. This plan delivers
the **establishment** that joins them.
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — an exchange that ended at a hole punch.** Plan 532's lane drove
three real routers over real loopback UDP and the relay exchange completed:
Alice asked Bob for Charlie, Bob introduced Charlie, Charlie verified the
introduction and answered it, Bob forwarded the accept, and Charlie's hole
punch arrived out-of-session. But **Alice never opened a session with
Charlie**. The exchange was complete in *shape* and not in *effect*, and three
gaps stood in the way.

1. **The accept's token was inert.** The target minted a fresh nonzero token
   from the daemon's own randomness
   (`draw_nonzero_token`, removed by this plan). The only possible consumer of
   that token is the target's **responder**, whose authority is the runtime's
   `TokenStore` — a table the token was never issued into. The responder would
   have refused its own token as **unknown**. The token's value was opaque to
   the tree.
2. **An early hole punch was refused and lost.** The transport's requester
   table requires the accept to be correlated *before* a hole punch is
   processed, and it refuses the punch with `WrongRole` otherwise
   (`crates/i2pr-transport-ssu2/src/relay.rs:569`, `:582`). The punch travels
   **one** hop and the accept travels **two**, so on loopback the punch reaches
   the requester's handoff **first**. The daemon's `map_relay_error` collapsed
   that refusal into a generic transport refusal, **after** the event had been
   consumed, and the punch was gone.
3. **There was no establishment.** Nothing read the accept's endpoint and
   token, nothing read the target's validated static and introduction keys, and
   nothing dialled the target — while the transport's own documentation
   already named the design: the caller dials via the standard establishment
   path with the HolePunch token and never a relay-specific fake session.

**The division of ownership.** The daemon owns the *policy*; the runtime owns
the *mechanics*.

**The token is issued by the responder, not by the daemon.** The runtime draws
eight random bytes, rejects zero, and issues the token from **its own**
`state.token_store` (`crates/i2pr-runtime/src/ssu2_runtime.rs:2999`, the issue
at `:3016`), bound to the **requester's exact source address** and with **no**
responder connection id — the responder mints a fresh one at SessionCreated,
exactly as it does for a data-phase `NewToken`. The target owner asks the
runtime for the token
(`crates/i2pr-daemon/src/relay_target.rs:494`) and puts the issued value in
the accept. The daemon's own draw is **removed**: a daemon-side draw produces a
token in no table, and a token in no table is refused as unknown. The
randomness and the table are the runtime's; the daemon never sees either.

**The deferral lives in the requester owner, not in the transport.** The
transport's wrong-role rule is **correct** and is **unchanged**: it is the
**deferral signal**. The requester owner intercepts
`i2pr_runtime::RelayError::WrongRole` **before** the generic error mapping
(`crates/i2pr-daemon/src/relay_requester.rs:1064`, the interception at
`:1121`) and **holds** the punch in a bounded table
(`MAX_DEFERRED_PUNCHES = 8`, `crates/i2pr-daemon/src/relay_requester.rs:106`)
rather than refusing it. The hold is bounded, the refusal is counted under its
own reason, and a punch that arrives with no room is refused — never dropped
silently.

**The held punch is presented again.** When the accept is correlated, the
drain calls `owner.redrive_deferred(...)`
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:213`), and the requester owner
presents the held punch through the transport's **own** seam with the
deferral switched off (`crates/i2pr-daemon/src/relay_requester.rs:1093`). The
transport's verification runs again on it and its one-use guarantee is
unchanged. A re-drive that the transport still refuses is a **refusal**, not
another hold: the hold has already had its one chance.

**The establishment is the ordinary establishment path.** The runtime gains a
dial seam that presents an **explicit** token
(`crates/i2pr-runtime/src/ssu2_runtime.rs:4880`), and the ordinary
`dial_ssu2` delegates to it. The explicit token **skips the cached-token
path**: the cache is a FIFO keyed by peer, so an older cached entry would be
presented instead and the responder would refuse it as unknown
(`crates/i2pr-runtime/src/ssu2_runtime.rs:4974`). There is **no** relay-
specific session, **no** synthetic peer and **no** bypass of the responder's
verification — the establishment is the runtime's normal SSU2 handshake with a
token in it.

**The target's material is read from validated material only.** The
establishment reads the target's static and introduction keys from the
authoritative store through the transport's strict `Ssu2RouterAddress::parse`
(`crates/i2pr-daemon/src/relay_establishment.rs:243`), and builds the dial
target under the **qualification profile**, so the relay establishment cannot
widen the router's reach. Unvalidated bytes are never parsed.

**The establishment is a bounded, owned task.** A handshake is an `await` and
the daemon's `select!` loop must never block on one. The establishment runs on
a task of its own under the service's child scope
(`crates/i2pr-daemon/src/relay_establishment_task.rs:35`, the run at `:79`),
spawned only when the operator has opted in
(`crates/i2pr-daemon/src/lib.rs:4897`). The handoff between the requester
owner and the task is a **bounded** channel
(`RELAY_ESTABLISHMENT_QUEUE = 8`,
`crates/i2pr-daemon/src/relay_establishment.rs:54`); a full handoff **returns
the material to the owner** under a counted reason
(`crates/i2pr-daemon/src/lib.rs:5685`, `:5687`) rather than dropping it
silently. The task reads the router's authoritative store under the bootstrap's
lock and **releases it before the dial**.

**The transport's relay state machines and token table are used, not edited.**
**Plan 533 changes no transport file at all.** The responder's own table
decides whether a token is accepted; this plan only issues into it.

**No reachability signal is written by the daemon.** A peer reached through a
relay is reachable **through the relay**, not directly. The requester writes
**no** reachability signal of its own, the corroboration count is **unchanged**,
and the recorded state stays `Firewalled` — the relay-firewalled signal
**contradicts** reachability and continues to.

**One opt-in per role, and no new key.** The establishment rides the
**existing** `ssu2.relay_requester` opt-in and the token issuance rides the
**existing** `ssu2.relay_target` opt-in. **No configuration key is added**,
and both stay default **off** and refused at parse time without
`ssu2.enabled`.

**Every refusal is typed and counted, and no refusal writes into a peer's
record.** The requester gained `DeferralFull`, `EstablishmentUnavailable` and
`EstablishmentQueueFull`; the target gained `EstablishmentRefused`; the
establishment carries eight reasons of its own
(`crates/i2pr-daemon/src/relay_establishment.rs:58`). A peer that is unknown,
absent or unparsable is refused under its own reason — a **local** condition,
never a peer's profile.

**The exchange completes.** The lane drives three **real** routers over real
loopback UDP, runs the **production** establishment task against a real
`Bootstrap`, and asserts that the requester holds a session with the target
after the exchange and that the target's runtime holds the matching session.

## What the plan did not close

- The **route** by which a peer reached through a relay is then used for
  tunnel building. This plan establishes the session; it does not route traffic
  over it. **Owner:** a successor in the transit line.
- The **relayed peer's** presence in the peer-profile store. The establishment
  records a **local** fact about this router's session and writes nothing into
  the peer's record. **Owner:** a successor may add a local session
  observation.
- The target role still needs the **authoritative store**, which the
  composition root takes from the bootstrap. The establishment's task has the
  same dependency and reads the same store. Plan 532's **Low** finding 2 is
  therefore **unchanged**.
- The **production** SSU2 peer-test driver stays unbuilt; the `B` advertisement
  and `caps=f` stay with their owners; and the **stock-peer** half stays
  blocked on Plan 493's authorized two-host topology.
- The delivery is confined to the **controlled loopback profile**: the router's
  SSU2 dial target is loopback-only and the private-LAN variant accepts only
  RFC1918 IPv4, so the establishment can only reach a peer on a local or
  private network. That bound is Plan 431's.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `bfe80fd8` | the registration: the plan of record, the registry row and the roadmap row |
| `4f87a040` | the production change: the runtime's token issuance and token-bearing dial, the target's real token, the requester's deferral and re-drive, the establishment owner and task, the composition, and the drain's re-drive |
| `45ce73ae` | the daemon lane and the daemon guard |
| `4fc969d3` | the documentation, the inventory and the plan's amendment log |
| `19abb0e0` | the floor's caught defects: the relay-firewalled recorder returns to the caller's body, and the two lint findings in this plan's own test files are cleared |

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that satisfies
it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | the target's accept carries a token the target's own responder consumes, and the lane proves the consumption rather than inferring it | `crates/i2pr-runtime/src/ssu2_runtime.rs:2999` (the issuance), `:3016` (the issue into the responder's own table), `crates/i2pr-daemon/src/relay_target.rs:494` (the target's call), `:503` (the token counted) | guard rule 2 `the_token_is_issued_by_the_runtimes_responder_table`; the lane rows `the_accept_carries_a_token_the_targets_own_responder_issued` and `the_requester_establishes_a_session_with_the_peer_it_reached` |
| 2 | a hole punch that arrives before its accept is held and re-driven, and the exchange completes on the **production** path with no lane-side scaffolding | `crates/i2pr-daemon/src/relay_requester.rs:1064` (the deferral), `:1093` (the re-drive), `:1121` (the interception before the generic mapping), `crates/i2pr-daemon/src/ssu2_relay_drain.rs:213` (the drain's re-drive) | guard rule 7 `the_deferred_punch_is_bounded_and_re_driven`; the lane row `the_requester_establishes_a_session_with_the_peer_it_reached` |
| 3 | the requester holds an authenticated session with the target after the establishment, and the target's runtime holds the matching session | `crates/i2pr-runtime/src/ssu2_runtime.rs:4880` (the token-bearing dial), `:4899` (the shared dial step), `:4974` (the explicit token skips the cache), `crates/i2pr-daemon/src/relay_establishment.rs:292` (the drive step), `crates/i2pr-daemon/src/relay_establishment_task.rs:79` (the task) | guard rule 4 `the_establishment_is_the_ordinary_establishment_path`; the lane row `the_requester_establishes_a_session_with_the_peer_it_reached` |
| 4 | the token is one-use and source-bound, and both properties are proven by a negative row | `crates/i2pr-runtime/src/ssu2_runtime.rs:2999` (the binding to the requester's address), `crates/i2pr-daemon/src/relay_target.rs:101` (the refusal reason) | the lane rows `the_token_is_one_use_and_the_responder_refuses_the_second` and `a_token_presented_from_another_address_is_refused` |
| 5 | every refusal is typed and counted, and no refusal writes into a peer's record | `crates/i2pr-daemon/src/relay_establishment.rs:58` (the eight reasons), `:109` (one counter per reason), `crates/i2pr-daemon/src/relay_target.rs:62` (the target's reasons), `:187` (the counter), `crates/i2pr-daemon/src/relay_requester.rs:323` (the requester's reasons) | the lane row `every_refusal_is_typed_and_counted_and_writes_nothing` |
| 6 | no configuration key is added, `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | `crates/i2pr-daemon/src/relay_establishment.rs` (no config surface); the establishment rides `ssu2.relay_requester` and the issuance rides `ssu2.relay_target`; neither manifest is touched | guard rule 10 `no_configuration_key_is_added`; `python3 scripts/check-tooling-inventory.py`; the counts are 126 surfaces, 121 experimental, 4 deferred, 1 ready, **0 advertised** |
| 7 | the full floor is green, with the closure record's table complete | the floor table below | the floor run |
| 8 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | `crates/i2pr-daemon/tests/ssu2_relay_establishment_boundary.rs` (12 rules, 15 inverted probes, 9 fail-closed probes, 36 rows) | the negative controls below |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_establishment -- --test-threads=1` | PASS — 7 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_establishment_boundary -- --test-threads=1` | PASS — 36 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester --test ssu2_relay_requester_boundary -- --test-threads=1` | PASS — 8 rows and 26 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target -- --test-threads=1` | PASS — 8 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS |
| `python3 /tmp/533-negative.py` | PASS — 7 controls, each caught by the surface that owns the defect, each restored byte-identically |

## The routine floor

The floor ran on the fixed tree `19abb0e0` and it is **green**:
**105 PASS, 0 FAIL**. Every step is recorded in `/tmp/533-floor.log`, with each
step's own log under `/tmp/533f-<step>.log`. The floor is a plain sequence of
`step` runs that records each step's exit status; no step is skipped, no
`|| true`, no `continue-on-error` and no filename filtering. The workspace step
runs with `--no-fail-fast`, so every test binary's result is reported rather
than the run stopping at the first failure.

Plan 533's floor is Plan 532's floor plus two steps — the daemon lane and the
daemon guard for the relay establishment. **No other step changed, and no other
plan's guard file was touched.**

| Step | Outcome |
| --- | --- |
| `floor-tree-clean` | PASS |
| `fmt` | PASS |
| `check` | PASS |
| `app-siblings` | PASS |
| `app-fixture-static` | PASS |
| `app-fixture-copy` | PASS |
| `app-fixture-rebuild` | PASS |
| `plan522-peer-pool-lane` | PASS |
| `plan522-peer-pool-boundary` | PASS |
| `plan522-peer-pool-unit` | PASS |
| `transport-ssu2-unit` | PASS |
| `runtime-unit` | PASS |
| `netdb-unit` | PASS |
| `plan523-introducer-lane` | PASS |
| `plan523-introducer-boundary` | PASS |
| `plan524-introducer-request-lane` | PASS |
| `plan524-introducer-request-boundary` | PASS |
| `plan520-publication-lane` | PASS |
| `plan520-publication-boundary` | PASS |
| `plan525-introducer-publication-lane` | PASS |
| `plan525-introducer-publication-boundary` | PASS |
| `plan526-introducer-advertisement-lane` | PASS |
| `plan526-introducer-advertisement-boundary` | PASS |
| `plan521-peer-test-role-boundary` | PASS |
| `plan527-reachability-contradiction-lane` | PASS |
| `plan527-reachability-contradiction-boundary` | PASS |
| `plan528-peer-test-verdict-lane` | PASS |
| `plan528-peer-test-verdict-boundary` | PASS |
| `plan529-observed-address-lane` | PASS |
| `plan529-observed-address-boundary` | PASS |
| `plan530-explicit-bind-lane` | PASS |
| `plan530-explicit-bind-boundary` | PASS |
| `plan531-relay-requester-lane` | PASS |
| `plan531-relay-requester-boundary` | PASS |
| `plan532-relay-target-lane` | PASS |
| `plan532-relay-target-boundary` | PASS |
| `plan533-relay-establishment-lane` | PASS |
| `plan533-relay-establishment-boundary` | PASS |
| `test-workspace` | PASS |
| `clippy` | PASS |
| `doc` | PASS |
| `test-doc` | PASS |
| `dependency-direction` | PASS |
| `plan-number-uniqueness` | PASS |
| `adr-number-uniqueness` | PASS |
| `portable-service-tunnel-api` | PASS |
| `portable-service-tunnel-consumer` | PASS |
| `planning-tests` | PASS |
| `runtime-boundaries` | PASS |
| `console-boundaries` | PASS |
| `console-browser-security` | PASS |
| `tooling-inventory` | PASS |
| `license-metadata` | PASS |
| `managed-app-private-client-seams` | PASS |
| `router-readiness-contract` | PASS |
| `router-readiness-self-test` | PASS |
| `core-router-recovery-contract` | PASS |
| `core-router-recovery-self-test` | PASS |
| `plan449-ssu2-preflight` | PASS |
| `plan495-peer-eligibility` | PASS |
| `plan476-gate-binding` | PASS |
| `plan476-gate-binding-self-test` | PASS |
| `managed-app-process-boundary` | PASS |
| `managed-app-process-boundary-self-test` | PASS |
| `managed-app-package-boundary` | PASS |
| `managed-app-package-boundary-self-test` | PASS |
| `managed-app-policy-boundary` | PASS |
| `managed-app-policy-boundary-self-test` | PASS |
| `managed-app-secured-sandbox` | PASS |
| `managed-app-secured-sandbox-self-test` | PASS |
| `service-tunnel-boundaries` | PASS |
| `managed-app-gateway-boundary` | PASS |
| `managed-app-manager-boundary` | PASS |
| `m11-per-epoch-composition` | PASS |
| `service-anonymity-boundaries` | PASS |
| `fixture-manifest` | PASS |
| `ntcp2-vectors` | PASS |
| `ssu2-vectors` | PASS |
| `i2cp-vectors` | PASS |
| `ntcp2-interoperability` | PASS |
| `constrained-host-lane-boundary` | PASS |
| `m11-transit-boundaries` | PASS |
| `m11-transit-qualification-evidence` | PASS |
| `sam-acceptance-evidence` | PASS |
| `ssu2-acceptance-evidence` | PASS |
| `i2cp-acceptance-evidence` | PASS |
| `i2pcontrol-acceptance-evidence` | PASS |
| `els2-type11-transcript-boundary` | PASS |
| `els2-live-lane-evidence` | PASS |
| `encrypted-service-consumer-caller` | PASS |
| `outproxy-request-path` | PASS |
| `outproxy-wire-lane-evidence` | PASS |
| `config-secret-hygiene` | PASS |
| `workflow-validity` | PASS |
| `floodfill-type5-serve` | PASS |
| `service-tunnel-acceptance-evidence` | PASS |
| `exploratory-tunnel-evidence` | PASS |
| `netdb-tunnel-evidence` | PASS |
| `destination-tunnel-evidence` | PASS |
| `streaming-tunnel-evidence` | PASS |
| `m6-mixed-router-acceptance-evidence` | PASS |
| `m12-floodfill-qualification-evidence` | PASS |
| `m12-floodfill-boundaries` | PASS |
| `ntcp2-harness` | PASS |
| `cargo-deny` | PASS |


**A floor-caught defect, and what it was.** The floor's first run was **not**
green: three steps failed, from **two** distinct defects, both in this plan's
own work.

**The first defect was in the production code.** Plan 531's boundary guard
failed its rule `the_relay_firewalled_fact_is_a_contradiction` with the reason
*"a completed exchange must record the relay-firewalled fact"*. The cause was
in **this plan's own production change**, not in Plan 531's guard: the
correlation step had been split into a shared `handle_hole_punch`, which moved
the recorder call `self.runtime.note_relay_firewalled(entry.context.family);`
one level down, out of the body Plan 531's rule reads. The recording itself
was never lost — the shared step still performed it — but the rule's **shape**
no longer held. **The production code was fixed, not the guard.** The shared
step now hands the completed request back to its caller, and each caller
records the relay's success against it — so the literal is back in
`on_hole_punch`'s body, Plan 531's inverted probe still fires (it replaces
that exact literal), and **Plan 531's guard file is byte-identical**. Plan
533's own guard was unaffected throughout: the shared step still carries the
wrong-role interception and the deferral, and the re-drive still presents the
held punch with the deferral switched off.

**The second defect was a lint finding in this plan's own test files.** The
`clippy` step failed on two helper functions in this plan's guard
(`item_text` and `arm_body`) that no rule uses, and then, once those were
removed, on a `clone_on_copy` in this plan's lane's counter read. The two
helpers are removed and the copy is a copy. **Neither finding was in a
production file.**

The fix is commit `19abb0e0`. The floor's **second** run, on the fixed tree,
is the one recorded in the table above.

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be reported.
Every probe mutates an **in-memory copy** of the source, so **no file on disk
is written**. Every rule **fails closed** when the item it names is absent —
the guard reports the missing item rather than passing.

### Plan 533's guard — `crates/i2pr-daemon/tests/ssu2_relay_establishment_boundary.rs`

Twelve rules, fifteen inverted probes, nine fail-closed probes. All 36 rows
green.

| Rule | What it forbids | Its inverted probes |
| --- | --- | --- |
| 1 | an owner that does not use the runtime's seam, or a daemon-side copy of the transport's machinery | `the_guard_reports_a_daemon_side_token_table` |
| 2 | a token that is not issued by the runtime's responder table | `the_guard_reports_a_runtime_that_issues_from_another_table`, `the_guard_reports_a_daemon_that_draws_its_own_token` |
| 3 | a token that is not bound to the requester's source | `the_guard_reports_a_token_bound_to_another_source` |
| 4 | an establishment that is not the ordinary establishment path | `the_guard_reports_an_establishment_that_skips_the_token` |
| 5 | a dial target that does not come from the validated store | `the_guard_reports_a_dial_target_that_parses_unvalidated_bytes` |
| 6 | a dial target that ignores the qualification profile | `the_guard_reports_a_dial_target_that_ignores_the_profile` |
| 7 | an early punch that is refused instead of held, or a hold that is never re-driven | `the_guard_reports_a_punch_that_is_refused_instead_of_held`, `the_guard_reports_a_deferral_that_is_never_re_driven` |
| 8 | an unbounded handoff, or a silent handoff drop | `the_guard_reports_an_unbounded_handoff`, `the_guard_reports_a_silent_handoff_drop` |
| 9 | a tick that awaits the handshake | `the_guard_reports_a_tick_that_awaits_the_handshake` |
| 10 | a configuration key of the establishment's own | `the_guard_reports_a_configuration_key_for_the_establishment` |
| 11 | a daemon-side reachability write | `the_guard_reports_a_daemon_side_reachability_write` |
| 12 | an edited transport rule | `the_guard_reports_an_edited_transport_rule` |

Nine further probes remove the item each rule names — the owner, the issuance
seam, the drive step, the preparation step, the profile builder, the re-drive,
the handoff bound, the counters and the transport's token table — and every
rule reports the absence rather than passing. They are the
`the_guard_fails_closed_when_*` rows.

### The negative control against the **real** tree

The guard's probes are in-memory. The tree was therefore also mutated **on
disk** — one mutation at a time, each restored byte-identically — and the guard
and the lane were re-run. The script is `/tmp/533-negative.py`.

| Mutation | The guard | The lane |
| --- | --- | --- |
| NC-1 — the target draws its own token instead of issuing one | **FAILED**, as it must (`the_token_is_issued_by_the_runtimes_responder_table`) | green — the lane is a behaviour lane and does not read the owner's source |
| NC-2 — the runtime issues the token against another source address | **FAILED**, as it must (`the_token_is_bound_to_the_requesters_source`) | green — the lane's token is bound by the source address it observes |
| NC-3 — the establishment dials without the accept's token | **FAILED**, as it must (`the_establishment_is_the_ordinary_establishment_path`) | green — the lane's session is established with the token the responder consumed |
| NC-4 — the dial target ignores the qualification profile | **FAILED**, as it must (`the_dial_target_obeys_the_qualification_profile`) | green — the lane's targets are inside the profile |
| NC-5 — the early hole punch is refused instead of held | **FAILED**, as it must (`the_deferred_punch_is_bounded_and_re_driven`) | **FAILED**, as it must — four rows: `the_requester_establishes_a_session_with_the_peer_it_reached`, `the_accept_carries_a_token_the_targets_own_responder_issued`, `the_token_is_one_use_and_the_responder_refuses_the_second`, `a_token_presented_from_another_address_is_refused` |
| NC-6 — the runtime's token-bearing dial ignores the explicit token | green — the guard is the authority for the shape of the seam | **FAILED**, as it must — three rows: `the_requester_establishes_a_session_with_the_peer_it_reached`, `the_token_is_one_use_and_the_responder_refuses_the_second`, `a_token_presented_from_another_address_is_refused` |
| NC-7 — the establishment's task is not spawned under the service's scope | **FAILED**, as it must (`the_owner_is_the_daemons_and_the_mechanics_the_runtimes`) | green — the lane drives the task directly |

The **NC-6** result is the sharpest of the seven, and it is worth stating
plainly: the guard alone would not have caught it. When the explicit token is
dropped, the runtime falls back to the tokenless path, obtains a fresh Retry
and **still establishes a session** — so the session's existence proves
nothing about the token. The lane catches it because the **responder's own
`token_rejections` counter** and the **`establishments_with_token`** counter
are the observables: a session established *without* the token leaves both at
zero. This is why the lane asserts the **counters** and not merely the dial's
outcome, and why §17.2's fifth finding is load-bearing rather than
incidental.

The split is honest and is recorded rather than hidden. The **lane** is the
authority for the **behaviour**: the session is established, the token is
consumed by the responder, the token is one-use and source-bound, and the
early punch is held. The **guard** is the authority for the **shape**: the
token's issuer, the token's binding, the ordinary establishment path, the
validated store, the qualification profile, the bounded deferral, the bounded
handoff, the tick that never awaits, the absence of a configuration key, the
absence of a daemon-side reachability write, and the untouched transport.

Every mutation was restored **byte-identically** — the restore is verified by
a SHA-256 comparison of the file's bytes — and the working tree was verified
to be exactly as it was before each mutation.

## Acceptance criteria

The plan's §14 lists eight criteria and four stop conditions.

1. the target's accept carries a token the target's own responder consumes, and
   the lane proves the consumption rather than inferring it — **met**;
2. a hole punch that arrives before its accept is held and re-driven, and the
   exchange completes on the **production** path with no lane-side scaffolding
   — **met**;
3. the requester holds an authenticated session with the target after the
   establishment, and the target's runtime holds the matching session —
   **met**;
4. the token is one-use and source-bound, and both properties are proven by a
   negative row — **met**;
5. every refusal is typed and counted, and no refusal writes into a peer's
   record — **met**;
6. no configuration key is added, `router.version` stays `0.9.62`, and the
   inventory's advertised count stays **zero** — **met**;
7. the full floor is green, with the closure record's table complete —
   **met**;
8. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each — **met**.

**No stop condition fired.** The transport's responder **could** consume a
token the runtime issued from its own store, bound to the requester's address,
so no transport defect was found and the transport's table was not touched.
The establishment **could** be made to obey the qualification profile. The
floor's `cargo deny check advisories bans sources` passed. **No stock peer was
required** to prove any of the above: the stock-peer half stays blocked on
Plan 493 and was not attempted.

## Invariant review

The plan's §4 lists eleven invariants. Each is reviewed against the closing
tree.

1. **The token's issuer is the responder.** The runtime issues the token from
   its own `state.token_store`
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:1505`, the issuance at `:2999`),
   bound to the requester's exact source address, with the runtime's own
   randomness. The daemon's `draw_nonzero_token` is **removed**; the guard
   fails closed if a daemon-side draw appears.
2. **The token is one-use and source-bound.** It is consumed once, by the
   responder that issued it, and only from the exact address it was issued to.
   The transport's own table decides all three refusals — unknown, expired,
   reused and wrong-source — and the lane proves the one-use and source-bound
   properties through the responder's own `token_rejections` counter, not
   through a failed dial.
3. **The establishment is the ordinary establishment path.** The requester
   reaches the target with the runtime's normal SSU2 handshake
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:4880`). No relay-specific
   session, no synthetic peer, no bypass of the responder's verification.
4. **The requester reads only validated material.** The target's static and
   introduction keys come from the target's **validated** RouterInfo through
   the transport's strict address parser
   (`crates/i2pr-daemon/src/relay_establishment.rs:243`). The guard fails
   closed if unvalidated bytes are parsed.
5. **A local condition is never written into a peer's profile.** Every refusal
   in this plan is a typed, counted reason about **this router's** state, and
   the establishment writes no reachability signal and touches no tracker.
6. **The reachability model is untouched.** No corroborating class is claimed
   for a relayed peer; the corroboration count is unchanged and the recorded
   state stays `Firewalled`, because the relay-firewalled signal contradicts
   reachability.
7. **The transport's relay state machines are used, not edited.** The plan
   changes **no file** under `crates/i2pr-transport-ssu2/`. The responder's
   token table, the requester's wrong-role rule and the target's verification
   are all the transport's own.
8. **Every bound is explicit.** The deferred-punch buffer
   (`MAX_DEFERRED_PUNCHES = 8`), the pending establishment table
   (`MAX_PENDING_ESTABLISHMENTS = 8`) and the establishment handoff
   (`RELAY_ESTABLISHMENT_QUEUE = 8`) each have a stated capacity, and each
   refusal under a full bound is counted under its own reason.
9. **No advertisement.** `router.version` stays `0.9.62`, the published
   RouterInfo and `caps` are unchanged, and the inventory's advertised count
   stays **zero**.
10. **One opt-in per role.** The establishment rides the existing
    `ssu2.relay_requester` opt-in and the token issuance rides the existing
    `ssu2.relay_target` opt-in. **No configuration key is added**; the guard
    fails closed if one appears.
11. **No boundary script is weakened.** No `|| true`, no
    `continue-on-error`, no filename filtering, no fake peer environment, no
    broad exclusion, no early-return-success, and no production wire change
    made to go green.

## Failure, cancellation, restart, and contention review

**Failure.** Every refusal is typed and counted by reason, and leaves no state
behind: no establishment, no session, no held punch, no pending entry. A
deferral that cannot be held is refused under `DeferralFull`; a handoff that
cannot be made is refused under `EstablishmentQueueFull` **and the material is
returned to the owner** rather than dropped. The transport's own requester
table is the authority on a correlated punch, and the transport's own token
table is the authority on a token.

**Cancellation.** The establishment is a task under the service's child scope,
so it is cancelled with the service. The task reads the router's
authoritative store under the bootstrap's lock and **releases it before the
dial**, so a cancellation cannot hold the lock across an await. The requester
owner holds no task of its own: its work is a single synchronous step on the
product's existing tick.

**Restart.** Nothing is persisted. The deferred punches and the pending
establishments are live state and are rebuilt empty; a restart holds no punch,
establishes no session, and dials nothing.

**Contention.** The task takes the bootstrap's lock, and the requester owner
takes the runtime's own state lock — the same locks, in the same order, that
the daemon's other owners use. The establishment introduces **no new lock**
beyond the runtime's existing state lock. The handoff to the task is a
**bounded** channel, so a slow task cannot make the owner's table grow without
bound.

## Migration and compatibility review

**No wire change.** The relay blocks, their types, their lengths and their
signatures are exactly what the tree already sent and received. The accept
carries a token the responder can actually consume; the hole punch is built by
the transport's own codec; the establishment is the runtime's ordinary
handshake.

**No configuration migration.** **No key is added.** The establishment rides
the existing `ssu2.relay_requester` opt-in and the token issuance rides the
existing `ssu2.relay_target` opt-in. A configuration that named neither behaves
exactly as before.

**A new reading for the router.** A router that has asked for the requester
role may now hold a session with a peer it reached through a relay. It
publishes nothing new and advertises nothing new.

**No change to the transport's model.** The relay-firewalled signal already
exists and already contradicts; a peer reached through a relay gains no
corroborating class and the corroboration count is unchanged.

## Security review

**No secret is copied, logged or serialized.** The establishment material is a
target hash, a target endpoint and a token — the token is a **bearer** for one
session and is carried in the accept, which the introducer forwards. The
`RelayEstablishment` struct has a **redacted** `Debug`, so the token never
appears in a log line or a panic message. The counters carry counts only: no
peer hash, no address, no key, no nonce, no tag and no payload.

**No unvalidated bytes are parsed.** The target's static and introduction keys
come from the target's **validated** RouterInfo, read out of the authoritative
store and parsed with the transport's strict `Ssu2RouterAddress::parse`. The
guard fails closed if the establishment parses unvalidated bytes.

**The token cannot be replayed or stolen.** The token is issued by the
responder, bound to the requester's **exact source address**, consumed once,
and expires on the transport's own `TOKEN_LIFETIME_SECONDS`. A token
presented from another address, presented twice, or presented after expiry is
refused by the responder's own table — not by a rule this plan writes — and
each refusal is counted by the responder that refused it.

**No reachability is fabricated.** The daemon writes **no** reachability
signal of its own and touches no tracker. A router that reaches a peer
through a relay is reachable **through the relay**, not directly, and the tree
records it that way.

**The establishment cannot widen the reach.** The dial target is checked
against the qualification profile before the dial, so the relay establishment
can only reach where the router already dials.

**The transport's verification is used, not re-implemented.** The daemon adds
no second token table and no second decision, so there is no second place
where a token could be admitted.

**No listener, no port, no discovery endpoint.** The establishment owns no
socket of its own; it uses the runtime's ordinary dial path.

## Limitations

- The **route** by which a peer reached through a relay is then used for
  tunnel building is **not** in this tree. This plan establishes the session;
  it does not route traffic over it. **Owner:** a successor in the transit
  line.
- The **relayed peer's** presence in the peer-profile store is **not** in this
  tree. The establishment records a **local** fact about this router's
  session; it writes nothing into the peer's record. **Owner:** a successor may
  add a local session observation.
- The target role still needs the **authoritative store**, which the
  composition root takes from the bootstrap; the establishment's task has the
  same dependency and reads the same store. Plan 532's **Low** finding 2 is
  **unchanged**.
- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4. That bound is Plan 431's.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology, and no stock router implements a router-level relay target.
- The **production** SSU2 peer-test driver stays unbuilt; the `B` advertisement
  and `caps=f` stay with their owners.
- Plan 530's Low finding 1 (the explicit-bind class lapses with the
  30-minute observation TTL, and no re-record timer exists) remains open and
  is **not** this plan's subject.

## Findings by severity

### None — nothing was found that blocks the plan

No High, Medium or Low finding remains open in the plan's own production
change. The guard's twelve rules and the lane's seven rows are green, the
floor is green, and the negative controls all fire the surface that owns the
defect.

### Medium — discharged

**Plan 532's Medium finding — the requester owner had no deferred-punch
buffer — is discharged by this plan.** The requester owner now holds an early
hole punch in a bounded table and presents it again through the transport's own
seam once the accept is correlated
(`crates/i2pr-daemon/src/relay_requester.rs:1064`, `:1093`). The correction
lives in this record and in the registry's row; **Plan 532's closed closure
record is not rewritten.**

### Low — discharged, and unchanged

**Plan 532's Low finding 1 — the accept's token had no consumer — is
discharged by this plan.** The accept's token is issued by the target's own
responder table and consumed by it. The correction lives in this record and in
the registry's row; **Plan 532's closed closure record is not rewritten.**

**Plan 532's Low finding 2 — the target role needs the bootstrap's store — is
unchanged.** The establishment's task has the same dependency and reads the
same store. It is the composition root's own line, and the requester has had
the same dependency since Plan 531.

### Low — recorded by this plan

1. **The requester's establishment is a task of its own.** The daemon's
   `select!` loop must never block on a handshake, so the establishment runs
   under the service's child scope. The handoff is bounded and a full handoff
   returns the material to the owner under a counted reason. **Owner:** the
   composition root's own line; the peer pool's task is the pattern.

## Roadmap disposition

The `network-participation` roadmap gains one closed row, **533**, with this
record as its closure. The row states what the plan closed, what it did not,
and that it advertises nothing.

The roadmap's SSU2 relay phase now reaches the clause it was written for. The
three roles of the relay protocol — introducer, requester and target — each
have a daemon-owned producer behind its own default-off opt-in, and the
requester now **establishes a session** with the peer it reached through the
relay. The roadmap's completion clause — *"so that a firewalled peer is
reachable through it"* — is discharged **on the controlled loopback profile**.
What remains is **not** a producer gap: the route by which a relayed peer is
then used for tunnel building, the production peer-test driver, the `B` and
`caps=f` advertisements, the public dial scope, and the stock-peer half on
Plan 493's topology.

## Unblock audit

| What was blocked | Disposition |
| --- | --- |
| Plan 532's **Medium** finding (the requester owner had no deferred-punch buffer) | **discharged** — the requester owner holds an early punch and presents it again; the correction lives in this record and in the registry row, never in the closed record |
| Plan 532's **Low** finding 1 (the accept's token had no consumer) | **discharged** — the token is issued by the target's own responder table and consumed by it |
| Plan 532's **Low** finding 2 (the target role needs the bootstrap's store) | **open** — the establishment's task has the same dependency; the composition root's line |
| The accept's **inert** token | **closed** — the daemon's own draw is removed; a token in no table is no longer produced |
| The **refused and lost** early hole punch | **closed** — the wrong-role refusal is the deferral signal, and the punch is held and re-driven |
| The **absent establishment** | **closed** — the requester dials the target through the runtime's ordinary establishment path, presenting the accept's token |
| The transport's responder consuming a presented token | **present and unchanged** — the transport's own table decides; the plan changes no transport file |
| The transport's requester wrong-role rule | **present and unchanged** — it is the deferral signal, and the transport's file is untouched |
| The **route** by which a relayed peer is used for tunnel building | **open** — a successor in the transit line |
| The **relayed peer's** presence in the peer-profile store | **open** — a successor may add a local session observation |
| The production SSU2 peer-test driver | **unbuilt** — the model, the recorder and the verdict path exist; the driver is a successor's subject |
| The `B` advertisement and `caps=f` | **out of scope**, with their owners named |
| The public dial scope | **out of scope** — Plan 431 owns it |
| The stock-peer half | **blocked externally** on Plan 493's authorized two-host topology |
| The delivery's scope | **the controlled loopback profile** — Plan 431's bound |

## Handoff notes

- The plan's subject is the **requester's establishment path**: the router must
  be able to establish a session with the peer it reached through a relay.
- The plan adds **no** capability to the transport. It composes machinery the
  tree already owns — the responder's token table, the transport's wrong-role
  rule, the transport's hole-punch codec and the runtime's ordinary dial path
  — and gives them a caller. **No transport file is changed.**
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **public dial scope**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.
- The **route** by which a relayed peer is then used for tunnel building is the
  next gap, and it belongs to the transit line.
- The floor's first run caught a real defect in this plan's own work — the
  relay-firewalled recorder had moved out of the body Plan 531's rule reads.
  The **production code** was fixed and Plan 531's guard file stayed
  **byte-identical**.
