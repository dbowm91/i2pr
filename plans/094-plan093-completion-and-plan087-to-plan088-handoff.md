# Plan 094: Plan 093 completion pass and Plan 087 -> Plan 088 handoff

## Status and authority

- Status: planned; narrow successor/completion plan.
- Parent roadmap: Plan 085.
- Immediate predecessor: Plan 093.
- Corrective target: complete Plan 093 without reopening its already-landed NTCP2 data-phase design.
- Closure target: Plan 087 `i2pr -> i2pd` forward direction.
- Next roadmap plan after successful closure: Plan 088.
- Execution lane: existing Plan 086 `host-loopback-development` only.
- Reference implementation: source-locked i2pd 2.60.0 revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Baseline implementation commit reviewed for this plan: `7438a9eda74c46d810d4bf9c9498ca7019655aa4`.
- Blocks: Plan 088, Plan 079, and every claim of two-way NTCP2 development interoperability until this plan closes.
- Plan type: runner/evidence authority correction, clean-head validation, one instrumented forward closure attempt, one control forward closure attempt, evidence sanitation, and status/gate reconciliation.

Active sequence for this handoff:

```text
Plan 085 -> Plan 086 -> Plan 087 -> Plan 090 -> Plan 091
         -> Plan 092 -> Plan 093 (implementation substantially landed)
         -> Plan 094 (completion authority)
         -> Plan 088
         -> conditional Plan 079 or Plan 072
```

Plan 094 does **not** replace the technical design of Plan 093. It exists because the Plan 093 implementation commit landed most of the intended correction surface but did not retain the required passing instrumented/control evidence pair, did not create `plans/093-status.md`, and did not complete the Plan 087 -> Plan 088 gate handoff. It also leaves the canonical Plan 083 event-authority contract insufficiently proven against the exact Plan 093 requirements.

Plan 088 must not execute until Plan 094 records all Plan 093 closure requirements as satisfied.

## Current baseline and exact remaining gap

The following Plan 093 surfaces are already present at baseline commit `7438a9eda74c46d810d4bf9c9498ca7019655aa4` and should be preserved unless a focused regression proves a defect:

```text
pinned i2pd source reclassification       = landed
Plan 092 Branch A supersedure              = landed
i2pd observer generation/reset contract    = landed
i2pd bounded observer rings                = landed
exact DeliveryStatus predicate waits       = landed
i2pr bounded multi-frame receive oracle    = landed
i2pr typed receiver failure surface        = landed
i2pr binary digest wrapper requirement     = landed
Plan 093 focused Python test matrix         = landed
static Plan 093 boundary enforcement        = landed
NTCP2 experimental/non-advertised state     = preserved
```

The repository is **not** yet ready for Plan 088 because these closure requirements remain open:

1. `tests/integration/ntcp2/harness/plan083_runner.py` still needs an authoritative proof/correction for Plan 093 event identity and drain semantics. The current runner stores only `event_name`, `source_side`, and `event_sha256` in its canonical observed-event list and must not silently satisfy the Plan 093 requirement for a genuine process invocation identity by treating `scenario_id` as that identity.
2. Live polling and final drains must be proven to share one event-ingestion authority with one dedup/current-run policy. A late i2pd exact-target send or terminal event must not be dropped or differently classified depending on which drain path sees it.
3. No authoritative Plan 093 instrumented forward pass is retained.
4. No authoritative Plan 093 control forward pass is retained.
5. No `plans/093-status.md` closure record exists.
6. `plans/087-status.md` contains stale/inconsistent authority text, including historical Plan 092 next-executable statements and a stray `plan_093b` token that must not remain part of the canonical gate vocabulary.
7. `plans/088-status.md` contains stale Plan 092 gate text and historical lane wording that must be reconciled before Plan 088 becomes executable.
8. The final passing records must bind nonzero binary/build/source/scenario/placement provenance to the same corrective i2pr source commit and the pinned i2pd revision.

The completion pass is therefore evidence- and runner-focused. Do not reopen Noise, SessionRequest, NTCP2 cryptography, RouterInfo publication, or the bounded multi-frame data-phase oracle unless a focused test or the single post-correction instrumented attempt produces direct contrary evidence.

## Environment contract

All work and retained closure evidence must stay within:

```text
topology_kind                    = host-loopback-development
development_only                 = true
release_qualified                = false
isolation_qualified              = false
bind address                     = 127.0.0.1 only
network_id                       = 99
fresh port                       = required per attempt
fresh state                      = required per attempt
sudo                             = forbidden
privilege escalation             = forbidden
rootless namespace execution     = not required / do not attempt
Multipass execution              = not required / do not attempt
Docker/QEMU                      = not required / do not add
public I2P network               = forbidden
reseed/bootstrap                 = forbidden
DNS                              = forbidden
SAM/I2CP/HTTP/SOCKS              = forbidden
SSU2                             = forbidden
normal daemon NTCP2 enablement   = forbidden
support advertisement            = forbidden
```

The rootless and Multipass scripts remain static boundary checks only. A failure in those static checks is a repository-boundary defect; it is not permission to reopen those execution lanes.

## Objective

Close the Plan 093/Plan 087 forward direction in this exact order:

1. prove or correct the canonical Plan 083 event identity and ingestion contract;
2. add focused regressions for current-run event authority, late-event retention, exact target identity, and provenance;
3. normalize current status authority so Plan 094 is the only completion action and Plan 088 remains blocked;
4. run all required static/unit validation;
5. commit every correction before any closure wire attempt;
6. rebuild and measure the exact clean committed head;
7. execute one fresh instrumented `i2pr -> i2pd` forward attempt;
8. stop immediately if it does not pass;
9. execute one fresh control forward attempt only after the instrumented pass;
10. stop immediately if control behavior is not equivalent;
11. retain only sanitized, digest-bound evidence;
12. create `plans/093-status.md` and close Plan 087;
13. rewrite Plan 088 as `next_executable` without claiming a reverse result;
14. leave Plan 079 blocked and Plan 072 inactive pending the actual Plan 088 decision.

## Non-goals

Plan 094 must not:

- redesign or broaden the Plan 093 data-phase oracle;
- restore the superseded Plan 092 Branch A handshake diagnosis;
- patch pinned i2pd transport semantics to force the test to pass;
- suppress i2pd's automatic RouterInfo behavior;
- weaken RouterInfo, Router Hash, network ID, DeliveryStatus, or authenticated-frame validation;
- permit generic send counters to substitute for the exact target reply write;
- increase deadlines, add retries, or automatically repeat live attempts to hide nondeterminism;
- create a new VM/container/network-namespace lane;
- run Plan 088 as part of this plan;
- activate Plan 079 or Plan 072;
- advertise NTCP2 support or modify production support claims.

## Mandatory invariants

```text
reference revision                       = exact pinned i2pd revision
source commit for each closure attempt   = exact 40-hex and clean
all attempted-live provenance digests    = exact 64-hex and nonzero
RouterInfo signature validation           = unchanged and strict
peer Router Hash binding                  = exact
endpoint binding                          = exact
network ID                                = exact 99
DeliveryStatus envelope ID                = exact target ID
DeliveryStatus payload ID                 = exact target ID
observer generation                       = current listener invocation only
observer drop count                       = 0 for pass
receive deadline                          = one absolute deadline
receive frame/byte/block bounds           = preserved
instrumented/control i2pr source          = same corrective commit
instrumented/control i2pd source          = same pinned revision
instrumented/control external behavior    = equivalent
cleanup                                   = clean and bounded
Plan 088                                  = blocked until both passes retained
NTCP2                                     = experimental and non-advertised
```

## Work package 1: freeze and verify the baseline before changing code

Before implementation:

1. Verify current `main` contains baseline `7438a9eda74c46d810d4bf9c9498ca7019655aa4` or a descendant.
2. Read Plan 093, `plans/087-status.md`, `plans/088-status.md`, `plans/091-status.md`, and `plans/092-status.md`.
3. Inspect the actual Plan 083 real-run path, not only test fakes.
4. Confirm whether an equivalent genuine invocation-ID/current-run mechanism landed somewhere outside `plan083_runner.py` after the baseline.
5. Confirm whether any passing Plan 093 instrumented or control evidence was committed after this plan was authored.
6. If both passing records and a correct gate handoff already exist, stop and reconcile status only; do not duplicate live execution.

Record a short implementation note containing:

```text
baseline_head
plan093_implementation_commit
runner_invocation_identity = proven | missing
shared_event_ingestion      = proven | missing
instrumented_record         = present-passing | absent | nonpassing
control_record              = present-passing | absent | nonpassing
plan093_status              = present-passed | absent | nonpassing
```

Acceptance criteria:

- the executor works from the actual current head rather than assuming `7438a9e...` is still tip;
- no already-closed requirement is reimplemented;
- every remaining change is tied to a specific unsatisfied Plan 093 closure criterion.

## Work package 2: make process invocation identity authoritative

The canonical runner must distinguish process invocations independently of scenario names.

Preferred bounded design:

1. The parent runner allocates an opaque invocation ID per launched child process before `popen()`.
2. The invocation ID is unique within the run and is not equal to `scenario_id`, `direction`, PID alone, or event sequence alone.
3. Inject the ID through the existing test-only launcher/driver configuration or environment boundary.
4. Each child includes that exact ID in every structured event it emits.
5. The runner validates the event ID against the expected child invocation before accepting the event.
6. Inspect-mode i2pd and listen-mode i2pd receive different invocation IDs even when they share a run root.
7. i2pr prepare, i2pr dialer, i2pd inspect, and i2pd listener identities cannot collide.
8. The record retains enough sanitized identity to prove current-run/process ownership without retaining a PID as the sole authority.

A deterministic parent-assigned value such as `run_id + actor + launch_sequence + digest` is acceptable. Randomness is not required. The critical property is that the value represents the concrete launch and is echoed by that launch, rather than being inferred later from a scenario label.

Required event identity fields for accepted live events:

```text
run_id
scenario_id
direction
source_side
invocation_id
event_sequence
event_sha256
binary_sha256
reference_revision   # reference-side events
```

For fields that are side-specific, the validator must have an explicit side-specific rule rather than silently using zero/empty placeholders.

Acceptance criteria:

- `scenario_id` cannot satisfy the invocation-ID check;
- inspect-mode events cannot satisfy listener-mode waits;
- events from an earlier child launch in the same run root are rejected;
- an event carrying the wrong binary digest is rejected;
- an event carrying a zero digest is rejected from any pass path;
- tests prove two launches of the same scenario name remain distinguishable.

## Work package 3: unify event ingestion and final-drain authority

Create one canonical event-ingestion primitive and use it for:

```text
listener readiness polling
live i2pr/i2pd event polling
post-i2pr bounded drain
final pre-reap drain
final post-reap validation drain, if one is retained
```

The primitive must:

1. validate the full current-run/process identity contract from Work package 2;
2. validate allowed event kind and source side;
3. validate `event_sequence` monotonicity per invocation;
4. validate `event_sha256`;
5. deduplicate by at least:

   ```text
   source_side
   invocation_id
   event_sequence
   event_sha256
   ```

6. preserve events in canonical sequence order;
7. never discard a valid later event merely because another event of the same kind was previously seen;
8. reject stale-generation reference events even if their other metadata matches;
9. expose the same accepted-event collection to live classification and final record construction;
10. retain the exact i2pr terminal reason instead of replacing it with `reference-events-missing` when a more specific reason exists.

The post-i2pr sequence must remain:

```text
i2pr terminal observed
-> bounded current-generation i2pd drain
-> i2pr process reap
-> i2pd natural terminal or bounded reap
-> final canonical drain
-> cleanup verification
-> result classification
```

Acceptance criteria:

- a late exact-target i2pd send-completion event is retained;
- a late i2pd terminal event is retained;
- duplicate reads of the same event do not duplicate the record;
- two distinct same-kind events remain distinct;
- no alternate final-drain parser bypasses current-run validation;
- `reference-events-missing` is selected only after all canonical drains and only when no more specific terminal reason applies.

## Work package 4: make forward pass classification exact

The runner must not infer pass from generic event names alone.

Instrumented forward pass requires all of these current-run facts:

```text
i2pd RouterInfo endpoint validated exactly
i2pr tcp_connected
i2pd tcp_accepted
i2pr ntcp2_authenticated
i2pd ntcp2_authenticated
i2pr target DeliveryStatus write completed
i2pd exact target DeliveryStatus received/authenticated/decoded
i2pd exact reply DeliveryStatus write completed after reply baseline
i2pr exact reply DeliveryStatus received/authenticated/decoded
envelope message ID == configured target ID on both target observations
payload message ID == configured target ID on both target observations
peer Router Hash == expected peer on both target observations
observer generation == current listener generation
observer drop count == 0
i2pr terminal == passed
i2pd terminal == clean
cleanup == clean
```

Automatic RouterInfo/DatabaseStore traffic may appear before the target and must remain non-target traffic. It cannot promote pass.

Acceptance criteria:

- generic `i2np_message_decoded` without the exact target metadata does not pass;
- generic `frame_emitted` without exact target send completion does not pass;
- wrong target ID or wrong peer hash fails closed;
- stale-generation exact target fails closed;
- handshake-only success fails closed;
- stage, counters, terminal reason, and retained events cannot contradict one another in a passing record.

## Work package 5: prove build and binary provenance

The wrapper's nonzero i2pr binary digest is necessary but not sufficient. Bind each closure attempt to a reproducible clean source/build identity.

Use the existing build-manifest machinery where possible. Add only the smallest bounded manifest needed for i2pr if the repository does not already provide one.

Required closure provenance:

```text
source_commit
source_tree_clean = true
Cargo.lock_sha256
i2pr_binary_sha256
i2pr_build_manifest_sha256
reference_revision
reference_source_tree_sha256
i2pd_binary_sha256
i2pd_build_manifest_sha256
driver_source_sha256
observer_patch_sha256       # instrumented only
observer_header_sha256      # instrumented only
observer_source_sha256      # instrumented only
placement_record_sha256
scenario_sha256
i2pr_router_info_sha256
i2pd_router_info_sha256
i2pr_router_hash_sha256
i2pd_router_hash_sha256
```

Control records must identify pristine/control observer state rather than copying instrumented observer digests. Use explicit `not-applicable-control` semantics only if the record schema permits a typed non-digest value; otherwise use a dedicated control provenance field. Do not encode absence as a fake nonzero digest.

Acceptance criteria:

- a binary built before the declared source commit is rejected;
- a dirty source tree is rejected before the closure run;
- zero digests cannot enter an attempted-live passing record;
- instrumented and control records bind the same i2pr source commit;
- instrumented and control records bind the same pinned i2pd revision;
- the control record proves it used the control/pristine driver build.

## Work package 6: add the Plan 094 focused regression matrix

Create:

```text
tests/integration/ntcp2/harness/test_plan094.py
```

Minimum required cases:

1. current status identifies Plan 094 as the Plan 093 completion authority while Plan 088 stays blocked;
2. same scenario name with two launch identities cannot cross-satisfy events;
3. `scenario_id` substituted for `invocation_id` is rejected;
4. wrong `run_id` rejected;
5. wrong direction rejected;
6. wrong source side rejected;
7. wrong binary digest rejected;
8. zero event digest rejected;
9. non-monotonic event sequence rejected;
10. exact duplicate event is deduplicated;
11. two same-kind events with different sequences are both retained;
12. inspect-generation event cannot satisfy listener-generation wait;
13. stale listener generation rejected;
14. live poll and final drain use the same ingestion function;
15. late exact-target send event retained;
16. late clean reference terminal retained;
17. automatic RouterInfo event cannot satisfy exact target send;
18. generic I2NP decode without exact target metadata cannot pass;
19. exact target requires envelope ID, payload ID, peer hash, current generation, and post-baseline sequence;
20. i2pr exact terminal reason survives final classification;
21. `reference-events-missing` is not allowed while a more specific current-run terminal exists;
22. attempted-live record rejects zero i2pr digest;
23. attempted-live record rejects zero i2pd digest;
24. attempted-live record rejects missing build-manifest provenance;
25. instrumented/control source-commit mismatch rejects closure;
26. instrumented/control reference-revision mismatch rejects closure;
27. instrumented pass without control pass keeps Plan 088 blocked;
28. both passing records are required to mark Plan 087 passed;
29. Plan 088 handoff cannot claim a reverse result before executing Plan 088;
30. Plan 079 remains blocked after Plan 094 closure until Plan 088 returns `two-way-development-probe-passed`;
31. Plan 072 remains inactive after Plan 094 closure unless Plan 088 later returns `ambiguous-reference-divergence`;
32. NTCP2 remains experimental and non-advertised.

Extend existing Plan 083/093/i2pd tests only where the owning behavior belongs there. Do not clone large fixtures into Plan 094.

Acceptance criteria:

- Plan 094 tests exercise behavior, not only source-string presence;
- every newly introduced runner identity/ingestion branch has a negative test;
- the Plan 093 bounded oracle tests remain green unchanged unless a directly proven defect requires a minimal correction.

## Work package 7: normalize pre-live status authority

Before the closure wire attempt, reconcile current status prose so a smaller executor cannot follow stale Plan 092/093 instructions.

At minimum update:

```text
plans/087-status.md
plans/088-status.md
plans/091-status.md       # only if it still claims active authority
plans/092-status.md       # only if it still claims active authority
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Required pre-live authority:

```text
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed
plan_091 = historical-partial-correction
plan_092 = superseded-by-plan093
plan_093 = implementation-landed-closure-incomplete
plan_094 = active-single-next-executable-completion-pass
plan_087 = open-pending-plan094-forward-evidence-pair
plan_088 = blocked-pending-plan094-completion
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

Specific cleanup:

- remove the stray `plan_093b` current-authority token;
- remove stale tables/paragraphs that still name Plan 092 as next executable;
- remove stale Plan 088 gate text that says Plan 092 owns forward closure;
- do not erase historical Plan 090/091/092 diagnostics; label them historical;
- do not claim Plan 093 passed before the live evidence pair exists;
- do not claim Plan 088 ran.

Acceptance criteria:

- each current status file names exactly one next executable action: Plan 094;
- no current-authority section contradicts another;
- historical sections cannot be mistaken for current gate authority;
- Plan 088 remains explicitly blocked.

## Work package 8: run static/unit validation and commit before live execution

Run at minimum:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan094.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan092.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan091.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan090.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
git diff --check
```

Also run the repository's live-binary test command if it is distinct from the above. Record executed/passed/failed/skipped counts. A required closure test that is skipped does not satisfy this plan.

Then:

1. require `git status --porcelain` to be empty;
2. commit all runner/provenance/status/test corrections;
3. record the exact 40-hex correction commit;
4. do not modify code after this commit before the instrumented attempt.

Acceptance criteria:

- every required command passes;
- no required live-capable validation is silently skipped;
- the correction commit is clean and immutable for both closure attempts;
- the evidence records do not bind uncommitted source.

## Work package 9: freeze closure builds and manifests

From the exact correction commit:

1. build `i2pr-interop`;
2. build the instrumented i2pd direct driver from the pinned source revision;
3. build the control/pristine i2pd direct driver from the same pinned revision;
4. generate/validate all build manifests;
5. measure binary and source/manifests immediately before execution;
6. verify the measured digests match the values passed to the runner;
7. verify no source mutation occurred after build.

Do not rebuild between instrumented and control attempts unless the build artifacts are proven reproducible and the new binary digest is explicitly recorded. Prefer freezing both binaries before the first live attempt.

Acceptance criteria:

- correction commit is exact and clean;
- all required binary/manifests exist as regular non-symlink files;
- all required digests are nonzero and schema-valid;
- both i2pd binaries prove the same reference revision;
- instrumented/control distinction is explicit.

## Work package 10: execute exactly one instrumented forward closure attempt

Use:

```text
direction       = i2pr-to-i2pd-ipv4
topology_kind   = host-loopback-development
network_id      = 99
address         = 127.0.0.1
fresh port      = yes
fresh run root  = yes
fresh identities = yes
fresh invocation IDs = yes
fresh observer generation = yes
fresh nonzero DeliveryStatus ID = yes
```

The attempt must use the exact correction commit and the frozen instrumented binaries from Work package 9.

Required instrumented pass evidence:

```text
RouterInfo exact endpoint verified
nonzero i2pr/i2pd binary provenance
i2pr tcp_connected
i2pd tcp_accepted
i2pr ntcp2_authenticated
i2pd ntcp2_authenticated
valid bounded pre-target traffic handled
i2pr exact target DeliveryStatus write complete
i2pd exact target DeliveryStatus receive/decode matched
i2pd exact target reply write complete after baseline
i2pr exact target reply receive/decode matched
exact peer Router Hash correlation
exact envelope + payload DeliveryStatus ID correlation
current listener generation only
observer drop count = 0
i2pr terminal = passed
i2pd terminal = clean
cleanup = clean
all owned processes terminated
record digest = exact nonzero 64-hex
all attempted-live provenance = exact and nonzero
```

Stop rule:

- If the instrumented attempt does not pass, **do not run control**.
- Preserve one sanitized diagnostic record and exact reason.
- Do not retry automatically.
- Do not change timeouts or add fallback behavior.
- Do not execute Plan 088.
- A new protocol/harness defect discovered here requires a new narrowly scoped successor plan unless the failure is a trivial evidence-rendering typo that does not require re-running the wire attempt.

Acceptance criteria:

- one and only one post-correction instrumented closure attempt is retained as authoritative for this execution cycle;
- every pass predicate above is directly evidenced;
- there is no generic-event or prose inference in the pass decision.

## Work package 11: execute exactly one control forward closure attempt

Run only after Work package 10 passes.

Control requirements:

```text
same i2pr correction commit
same pinned i2pd revision
control/pristine i2pd driver
fresh run root
fresh identities
fresh ports
fresh invocation IDs
fresh nonzero DeliveryStatus ID
same topology/network ID
same timeout classes
same bounded oracle configuration
same externally visible target exchange
```

The control path must not require instrumented observer events. Equivalent external success requires:

```text
i2pr terminal = passed
exact returned DeliveryStatus envelope ID = configured target
exact returned DeliveryStatus payload ID = configured target
expected peer Router Hash = exact
control reference process = clean terminal/exit
cleanup = clean
all owned processes terminated
control record digest = exact nonzero 64-hex
all required control provenance = exact and nonzero
```

Stop rule:

- If control fails, Plan 094 remains open and Plan 088 remains blocked.
- Classify the disagreement as observer-induced behavior, control-driver lifecycle divergence, or another exact bounded reason.
- Do not accept the instrumented pass as closure by itself.
- Do not run another control attempt automatically.

Acceptance criteria:

- control produces the same external protocol success as instrumented;
- control does not depend on observer-only evidence;
- source/revision equivalence is proven by provenance, not prose.

## Work package 12: sanitize and retain the evidence pair

After both attempts pass:

1. validate both records before deleting raw run state;
2. calculate final record SHA-256 values;
3. retain only the repository-approved sanitized compact evidence/index;
4. remove secret-bearing run roots;
5. verify no RouterInfo bytes, identities, private keys, NTCP2 static keys, raw frames, ciphertext, decrypted payloads, socket captures, or private filesystem paths were retained;
6. preserve exact digest-level provenance needed for reproducibility.

Required closure evidence summary:

```text
correction_commit
reference_revision
forward_instrumented_record_sha256
forward_control_record_sha256
i2pr_binary_sha256
instrumented_i2pd_binary_sha256
control_i2pd_binary_sha256
i2pr_build_manifest_sha256
instrumented_i2pd_build_manifest_sha256
control_i2pd_build_manifest_sha256
reference_source_tree_sha256
scenario_sha256 values
placement_record_sha256 values
RouterInfo digest values
Router Hash digest values
DeliveryStatus message IDs
cleanup result for each attempt
```

Acceptance criteria:

- both record digests are exact nonzero 64-hex values;
- neither passing record references zero placeholder provenance;
- secret-bearing raw state is deleted after evidence extraction;
- sanitized evidence is sufficient to prove the gate without exposing sensitive runtime material.

## Work package 13: close Plan 093 and Plan 087, then hand off Plan 088

Create `plans/093-status.md` only after both passes exist.

Required Plan 093 closure tokens:

```text
status = passed
classification = post-auth-data-phase-sequencing-corrected
completion_plan = plan094
correction_commit = <exact 40-hex>
forward_instrumented_record_sha256 = <exact 64-hex>
forward_control_record_sha256 = <exact 64-hex>
reference_revision = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
cleanup = clean
```

Rewrite `plans/087-status.md` current authority to:

```text
status = passed
plan_093 = passed
plan_094 = passed
forward_instrumented_record_sha256 = <exact 64-hex>
forward_control_record_sha256 = <exact 64-hex>
```

Rewrite `plans/088-status.md` current authority to:

```text
decision = not-yet-run
plan_087 = passed
plan_093 = passed
plan_094 = passed
plan_088 = next_executable
```

Then update README, AGENTS, and the i2pr NTCP2 interop skill so the single next executable plan is Plan 088.

Keep:

```text
plan_079 = blocked_pending_plan_088_two_way_pass
plan_072 = inactive_pending_plan_088_ambiguity
ntcp2 = experimental_non_advertised
```

Do not write any reverse result into Plan 088 status during this plan.

Acceptance criteria:

- Plan 093 closure exists and references both exact forward records;
- Plan 087 is unambiguously passed;
- Plan 088 is unambiguously `next_executable` and `not-yet-run`;
- no stale current-authority section names Plan 092, Plan 093, or Plan 094 as next executable after closure;
- Plan 079 and Plan 072 gates remain unchanged;
- NTCP2 support state remains experimental/non-advertised.

## Work package 14: final consistency and no-regression pass

After the closure/status commit:

1. rerun `test_plan094.py`, `test_plan093.py`, `test_plan088.py`, and the static NTCP2 interoperability checker;
2. search current authority docs for stale tokens:

   ```text
   plan_093b
   Plan 092 ... next executable
   Plan 093 ... next executable
   Plan 094 ... next executable
   blocked_pending_plan_092
   blocked_pending_plan_093
   open_pending_plan_092_forward_closure
   ```

   Historical sections may contain old plan names only when explicitly labeled historical and not parsed as current authority.
3. confirm no code changed between the correction commit and the two closure runs;
4. confirm the closure/status-only commit does not alter protocol behavior;
5. confirm `git diff --check` and the worktree are clean.

Acceptance criteria:

- all current authority is coherent;
- Plan 088 is the only next executable plan;
- closure evidence still validates after the status commit;
- no protocol or runtime code is changed by the closure/status commit.

## Explicit Plan 094 closure criteria

Plan 094 closes only when **every** item below is true:

- [ ] Current head was inspected and remaining work was revalidated before implementation.
- [ ] Plan 093's already-landed data-phase/oracle/observer corrections were not unnecessarily reopened.
- [ ] Every accepted live event binds the correct run and concrete process invocation.
- [ ] `scenario_id` is not used as a substitute for process invocation identity.
- [ ] Inspect-mode and listener-mode i2pd events cannot cross-satisfy waits.
- [ ] Event sequence and event digest are validated.
- [ ] Zero-digest events cannot contribute to a pass.
- [ ] Live polling and final drains share one canonical ingestion authority.
- [ ] A late exact-target send-completion event is retained.
- [ ] A late clean i2pd terminal event is retained.
- [ ] Exact duplicates are deduplicated without discarding distinct same-kind events.
- [ ] Exact target pass classification checks type, envelope ID, payload ID, peer hash, generation, and post-baseline sequence.
- [ ] Generic I2NP/frame events cannot substitute for exact target evidence.
- [ ] The exact i2pr terminal reason is preserved when a more specific reason exists.
- [ ] Attempted-live evidence rejects zero binary/build/source/scenario/placement provenance.
- [ ] i2pr build provenance proves the declared clean correction commit.
- [ ] Instrumented/control i2pd provenance proves the same pinned reference revision.
- [ ] Pre-live current status authority names Plan 094 as the completion action and keeps Plan 088 blocked.
- [ ] All required Plan 094/093/083/i2pd/088 tests pass.
- [ ] All required Rust/static boundary checks pass.
- [ ] All corrections are committed before live execution.
- [ ] The worktree is clean before both closure attempts.
- [ ] Exactly one authoritative post-correction instrumented forward attempt passes.
- [ ] The instrumented pass proves exact bidirectional DeliveryStatus exchange and clean teardown.
- [ ] Exactly one authoritative control forward attempt passes after the instrumented pass.
- [ ] The control pass proves equivalent external behavior without observer-only evidence.
- [ ] Instrumented and control records bind the same i2pr correction commit.
- [ ] Instrumented and control records bind the same pinned i2pd revision.
- [ ] Both record digests are exact nonzero 64-hex values.
- [ ] All required provenance digests are exact and nonzero.
- [ ] Observer drop count is zero in the instrumented pass.
- [ ] Both attempts terminate all owned processes and record `cleanup = clean`.
- [ ] Secret-bearing run roots are deleted after evidence sanitation.
- [ ] `plans/093-status.md` records `status = passed` and both record digests.
- [ ] `plans/087-status.md` records `status = passed`.
- [ ] `plans/088-status.md` records `decision = not-yet-run` and `plan_088 = next_executable`.
- [ ] No reverse Plan 088 result is fabricated during Plan 094.
- [ ] Plan 079 remains blocked pending a Plan 088 two-way pass.
- [ ] Plan 072 remains inactive pending a Plan 088 ambiguity decision.
- [ ] NTCP2 remains experimental, disabled by default, and non-advertised.

If any checkbox is false, Plan 088 remains blocked.

## Failure ownership and stop conditions

### Runner/event-authority validation fails before live execution

Fix only the smallest runner/schema/test surface necessary, rerun validation, and create the correction commit. Do not touch NTCP2 wire behavior unless a focused regression proves it is involved.

### Instrumented attempt fails before authentication

Retain sanitized evidence. Do not restore Plan 092 Branch A automatically. Use the existing metadata-only handshake observer to classify the exact stage. Stop Plan 094 and create a new narrowly scoped diagnostic/corrective plan.

### Instrumented authenticates but target exchange fails

Use the exact typed oracle/observer reason and current-run events to assign ownership. Do not retry, broaden timeouts, or run control. Stop and create a successor corrective plan.

### Instrumented passes but control fails

Treat the instrumented/control divergence as the blocker. Do not close Plan 093 or Plan 087. Do not run Plan 088. Create a narrow control-parity corrective successor.

### Both pass but status/evidence sanitation fails

Do not rerun the wire attempts merely to fix documentation or evidence-index rendering. Correct only the sanitation/status layer if the retained cryptographic digests and validated compact records are intact and the correction does not alter protocol/harness behavior.

### Both pass and all closure criteria pass

Close Plan 093 and Plan 087, commit the status/evidence reconciliation, and hand off Plan 088. Do not execute Plan 088 in the same plan.

## Smaller-model execution sequence

Execute in this exact order and stop on the first failed gate:

1. Read Plan 093 and current 087/088/091/092 status files.
2. Record current HEAD and verify it descends from the Plan 093 implementation baseline.
3. Inspect the Plan 083 real-run event path for genuine invocation identity and shared ingestion.
4. Check for already-retained passing Plan 093 instrumented/control records; do not duplicate them if valid.
5. Implement genuine per-process invocation identity if missing.
6. Implement one canonical event-ingestion/drain function if missing.
7. Make forward pass classification require exact target metadata.
8. Complete clean-source/build provenance binding.
9. Add `test_plan094.py` and focused owning-surface regressions.
10. Normalize pre-live status authority to Plan 094 and remove stale current-gate tokens.
11. Run every required validation command.
12. Stop and fix only bounded validation defects until green.
13. Verify clean worktree.
14. Commit all code/test/status corrections.
15. Record the exact correction commit.
16. Build and freeze i2pr, instrumented i2pd, and control i2pd binaries/manifests from that commit/pinned revision.
17. Measure and validate all provenance.
18. Create a fresh instrumented run root, identities, port, invocation IDs, generation, and target message ID.
19. Run exactly one instrumented forward closure attempt.
20. If it fails, retain sanitized diagnostic evidence and stop.
21. If it passes, validate every instrumented pass predicate before proceeding.
22. Create fresh control state/ports/IDs while keeping the same source/revision contract.
23. Run exactly one control forward closure attempt.
24. If it fails or diverges, retain sanitized diagnostic evidence and stop.
25. Validate both compact records and exact digests.
26. Delete secret-bearing run roots.
27. Create `plans/093-status.md` with both passing record digests.
28. Rewrite Plan 087 status to passed.
29. Rewrite Plan 088 status to not-yet-run / next-executable.
30. Update README, AGENTS, and interop skill to Plan 088 as the only next action.
31. Keep Plan 079 blocked and Plan 072 inactive.
32. Rerun focused status/gate/static consistency checks.
33. Commit closure/status reconciliation.
34. Report Plan 088 as ready for separate execution.

## Executor final report requirements

The handoff executor must report:

```text
implementation/correction commit
closure/status commit
exact files changed
runner invocation-ID design
shared ingestion/dedup design
exact pass predicate used
validation commands and pass/fail/skipped counts
instrumented attempt run ID
instrumented record SHA-256
instrumented i2pr/i2pd binary SHA-256 values
instrumented terminal + cleanup result
control attempt run ID
control record SHA-256
control i2pr/i2pd binary SHA-256 values
control terminal + cleanup result
reference revision
source/build manifest digests
whether raw secret-bearing state was deleted
Plan 093 final status
Plan 087 final status
Plan 088 gate status
Plan 079 gate status
Plan 072 gate status
NTCP2 advertisement/default-enable state
```

A final report that says only "tests pass" or "Plan 093 complete" without the exact record/provenance/gate values is insufficient.

## Handoff state before execution

```text
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed
plan_091 = historical-partial-correction
plan_092 = superseded-by-plan093
plan_093 = implementation-substantially-landed-closure-incomplete
plan_094 = planned-next-executable-completion-pass
plan_087 = open-pending-plan094-forward-evidence-pair
plan_088 = blocked-pending-plan094-completion
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

## Handoff state after successful execution

```text
plan_093 = passed
plan_094 = passed
plan_087 = passed
plan_088 = next-executable-not-yet-run
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```
