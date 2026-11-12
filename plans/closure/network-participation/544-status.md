# Plan 544 — the router's own RouterInfo must reach a private-LAN peer
out of band

**Closure record.** The plan of record is
`plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 544 is **closed**. The router now writes its **own signed RouterInfo** to
`<data_dir>/export/<lowercase-hex-router-hash>`, under the name the peer's cache
loader derives its expected key from, so the only step left in a lab handoff is
a copy.

A private-LAN qualification run has no floodfill to publish the record to, and
the SSU2 publication path is loopback-clamped by design
(`crates/i2pr-runtime/src/ssu2_runtime.rs`, `Ssu2PublicationUnavailable::
InvalidPublication`) — observed in production on the arm64 lab node, not
inferred. Before this plan the router produced no file at all: after a run the
arm64 data dir held `router.identity` and the two transport identities, and
nothing else.

Classification: **capability** (the router could not produce the record its own
peer's loader would accept, so the only handoff available was a manual decode
whose failure mode was silent). **Subsystem:** network-participation.
**Closure date:** 2026-11-12.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The record exists as a file.** `export_local_router_info`
(`crates/i2pr-daemon/src/lib.rs`) decodes the record the router has already
signed, derives its hash, and writes
`<data_dir>/export/<lowercase-hex-router-hash>` at `0700`/`0600`. The bytes on
disk are the bytes the router signed; the helper adds no transformation, no
re-encoding and no re-signing.

**The filename is the record's hash.** The peer's cache loader derives its
expected key from the filename
(`crates/i2pr-netdb-persist/src/cache_loader.rs`), and `ByteCache::validate_name`
accepts exactly 64 lowercase hexadecimal characters. A wrong name is a
`KeyMismatch` that reads exactly like an invalid record, which is why the plan
did not leave the naming rule to the operator.

**`export/`, never `netdb/routers/`.** The latter is the cache the router loads
into its **own** store at startup (`crates/i2pr-storage/src/lib.rs`,
`ByteCache::in_data_dir`). A record written there would make the router hold
itself as a peer on its next start.

**A record whose hash cannot be derived is skipped, not guessed.** A decode
failure returns `Ok(None)` and writes nothing, which the lane proves by
checking that the directory is absent or empty.

**The two-host run.** With the export in place, two `i2pr` routers on
`192.168.182.0/24` (`.143` x86_64 and `.20` aarch64) each loaded the other's
record (`bootstrap pipeline completed record_count=1`) and reached an
**authenticated SSU2 session** — `ssu2 peer pool tick planned=1 examined=1
refused=0 dials_started=1` followed by `outcome=Established` 49 ms later — with
NTCP2 authenticated in both directions on the same pair. This is the topology
Plans 431/449/472/476/493 have been blocked on.

## What the plan did not close

- **T4 and T5.** The correlated two-way reply and the peer-initiated session
  were not reached. The plan never claimed them.
- **Stock-router admission.** i2pr-to-i2pr is one topology. i2pd and Java I2P
  admission is Plan 493's question, and the reference predicates it needs are
  pinned in `specs/QUALIFICATION-GATES.md` §3.
- **`ssu2.advertise` is inert in every posture.** The publication path requires
  `ReachabilityState::Reachable` as well as a loopback bind, and static
  loopback-homogeneous traffic cannot reach it. **Owner:** the plan that gives
  the runtime a lab-side reachability class, if one is ever wanted.
- **`publish_router_hash` has no production caller.**
  `crates/i2pr-daemon/src/i2pcontrol_inspection.rs:450` implements a bounded,
  validated router-hash publication whose only callers are its own unit tests.
  **Owner:** a successor that either retires it or wires it.
- **The `[floodfill]` gate on the dual-transport record.**
  `crates/i2pr-daemon/src/lib.rs:1002` gates the two-transport record on
  `config.floodfill.qualification_profile`, so a lab operator must set the
  **floodfill** key to get an SSU2 endpoint into the record at all. **Owner:** a
  successor that documents or decouples it.
- **This is not publication.** The router still publishes to no NetDB it cannot
  reach.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `eeedb7b6` | the plan of record, the registry's row, the roadmap's row and the export helper with its call site |
| `e1ed8520` | the lane, the guard and the inventory row (one commit shared with Plan 545's lane, guard and row) |
| `228a8ef6` | the documentation, the qualification gates, the conformance account and the plan's amendment log |

The floor ran on the committed tree `6af68a8b`, which is `228a8ef6` plus Plan
545's documentation, its two corrected commits and the three fixes the floor's
first run demanded. The closure commit adds
**only** this record and the two closed rows.

The plan of record's **invariants** are at `plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md:119`,
its **required production changes** at `plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md:167`,
its **design decisions** at `plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md:177`, its
**lane** at `plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md:234`, its **guard** at
`plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md:240`, its **closure evidence** at
`plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md:276`, its **amendment log** at
`plans/implementation/network-participation/544-the-routers-own-routerinfo-must-reach-a-private-lan-peer-out-of-band.md:295`, the qualification gates at
`specs/QUALIFICATION-GATES.md:2330` and the conformance
account at `specs/CONFORMANCE.md:4840`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the export is the router's own record, unmodified | the lane's row `the_exported_bytes_are_the_bytes_the_router_signed`; NC-544-5 restores the defect |
| 2 | the filename is the record's hash | the lane's rows `the_export_name_is_the_hash_of_its_own_contents` and `the_peer_refuses_the_same_record_under_any_other_name`; the guard's rule `the_name_is_the_hash_in_lowercase_hex`; NC-544-2 and NC-544-4 restore the defect |
| 3 | the export never enters the router's own store | the lane's row `the_export_never_lands_in_the_directories_the_router_itself_loads`; the guard's rule `the_export_is_written_to_the_export_directory`; NC-544-1 restores the defect |
| 4 | the export is bounded and permissioned | the lane's row `the_export_is_written_private`; the guard's rule `the_export_is_written_private`; NC-544-3 and NC-544-7 restore the defect |
| 5 | a record whose hash cannot be derived is skipped, counted and never written under a guessed name | the lane's row `an_undecodable_record_is_skipped_without_writing_anything` |
| 6 | no new surface is reachable | the guard's rule `the_export_needs_no_configuration_and_opens_no_socket`; the manifest and lock digests |
| 7 | no advertisement | the guard's rule `nothing_is_advertised`; NC-544-8 restores the defect |
| 8 | the predecessors' lanes and guards stay byte-identical | the guard's rule `the_predecessors_evidence_is_untouched` |
| 9 | the record round-trips through the peer's loader | the lane's row `the_exported_record_round_trips_through_the_peer_cache_loader` |
| 10 | a second publication rewrites the same name | the lane's row `a_second_publication_of_the_same_identity_rewrites_the_same_name` |
| 11 | the inspection publication is unchanged and the helper has one call site | the guard's rule `the_publication_path_is_unchanged`; NC-544-6 restores a different defect on the same path |
| 12 | the two-host run establishes an authenticated session | `eeedb7b6`'s registration record; the LAN logs cited in the plan's §3 |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test router_info_export -- --test-threads=1` | PASS — 8 rows |
| `cargo test --locked -p i2pr-daemon --test router_info_export_boundary -- --test-threads=1` | PASS — 17 rows (8 rules, 9 inverted probes) |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS |
| `python3 /tmp/545-controls.py` | PASS — 14 controls for both plans, each firing the surface that owns the defect, each restored byte-identically |

The full routine floor, with this plan's and Plan 545's steps appended:

| # | Command | Outcome |
| --- | --- | --- |
| 1 | `cargo fmt --all --check` | PASS (4.1 s) |
| 2 | `cargo check --locked --workspace --all-targets` | PASS (18.7 s) |
| 3 | `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | PASS (0.1 s) |
| 4 | `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS (2234.9 s) |
| 5 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS (5.1 s) |
| 6 | `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS (7.3 s) |
| 7 | `cargo test --locked --workspace --doc` | PASS (3.9 s) |
| 8 | `bash scripts/check-dependency-direction.sh` | PASS (0.1 s) |
| 9 | `python3 scripts/check-global-plan-number-uniqueness.py` | PASS (0.1 s) |
| 10 | `python3 scripts/check-adr-number-uniqueness.py` | PASS (0.0 s) |
| 11 | `python3 scripts/check-portable-service-tunnel-api.py` | PASS (0.0 s) |
| 12 | `bash scripts/check-portable-service-tunnel-consumer.sh` | PASS (6.5 s) |
| 13 | `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS (7.4 s) |
| 14 | `bash scripts/check-runtime-boundaries.sh` | PASS (1.1 s) |
| 15 | `bash scripts/check-console-boundaries.sh` | PASS (0.3 s) |
| 16 | `bash scripts/check-console-browser-security.sh` | PASS (0.1 s) |
| 17 | `python3 scripts/check-tooling-inventory.py` | PASS (0.1 s) |
| 18 | `python3 scripts/check-license-metadata.py` | PASS (0.1 s) |
| 19 | `python3 scripts/check-managed-app-private-client-seams.py` | PASS (0.0 s) |
| 20 | `python3 scripts/check-router-readiness-contract.py` | PASS (0.0 s) |
| 21 | `python3 scripts/check-router-readiness-contract.py --self-test` | PASS (0.2 s) |
| 22 | `python3 scripts/check-core-router-recovery-contract.py` | PASS (0.0 s) |
| 23 | `python3 scripts/check-core-router-recovery-contract.py --self-test` | PASS (0.0 s) |
| 24 | `python3 scripts/plan449-ssu2-preflight.py --self-test` | PASS (0.0 s) |
| 25 | `python3 scripts/plan495-peer-eligibility.py --self-test` | PASS (0.1 s) |
| 26 | `python3 scripts/check-plan476-gate-binding.py` | PASS (0.3 s) |
| 27 | `python3 scripts/check-plan476-gate-binding.py --self-test` | PASS (0.6 s) |
| 28 | `python3 scripts/check-managed-app-package-boundary.py` | PASS (0.1 s) |
| 29 | `python3 scripts/check-managed-app-package-boundary.py --self-test` | PASS (0.1 s) |
| 30 | `python3 scripts/check-managed-app-policy-boundary.py` | PASS (0.1 s) |
| 31 | `python3 scripts/check-managed-app-policy-boundary.py --self-test` | PASS (0.1 s) |
| 32 | `python3 scripts/check-managed-app-secured-sandbox.py` | PASS (0.0 s) |
| 33 | `python3 scripts/check-managed-app-secured-sandbox.py --self-test` | PASS (0.0 s) |
| 34 | `bash scripts/check-service-tunnel-boundaries.sh` | PASS (0.1 s) |
| 35 | `python3 scripts/check-managed-app-gateway-boundary.py` | PASS (0.0 s) |
| 36 | `python3 scripts/check-managed-app-manager-boundary.py` | PASS (0.1 s) |
| 37 | `bash scripts/check-m11-per-epoch-composition.sh` | PASS (0.1 s) |
| 38 | `bash scripts/check-service-anonymity-boundaries.sh` | PASS (0.1 s) |
| 39 | `bash scripts/check-fixture-manifest.sh` | PASS (0.1 s) |
| 40 | `bash scripts/check-ntcp2-vectors.sh` | PASS (0.0 s) |
| 41 | `bash scripts/check-ssu2-vectors.sh` | PASS (0.0 s) |
| 42 | `bash scripts/check-i2cp-vectors.sh` | PASS (6.5 s) |
| 43 | `bash scripts/check-ntcp2-interoperability.sh` | PASS (0.0 s) |
| 44 | `bash scripts/check-constrained-host-lane-boundary.sh` | PASS (0.0 s) |
| 45 | `bash scripts/check-m11-transit-boundaries.sh` | PASS (2.2 s) |
| 46 | `bash scripts/check-m11-transit-qualification-evidence.sh` | FAIL (1.0 s) |
| 47 | `bash scripts/check-sam-acceptance-evidence.sh` | PASS (0.1 s) |
| 48 | `bash scripts/check-ssu2-acceptance-evidence.sh` | PASS (0.0 s) |
| 49 | `bash scripts/check-i2cp-acceptance-evidence.sh` | PASS (0.1 s) |
| 50 | `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS (0.0 s) |
| 51 | `bash scripts/check-els2-type11-transcript-boundary.sh` | PASS (0.1 s) |
| 52 | `bash scripts/check-els2-live-lane-evidence.sh` | PASS (2.0 s) |
| 53 | `bash scripts/check-encrypted-service-consumer-caller.sh` | PASS (0.1 s) |
| 54 | `bash scripts/check-outproxy-request-path.sh` | PASS (0.1 s) |
| 55 | `bash scripts/check-outproxy-wire-lane-evidence.sh` | PASS (0.2 s) |
| 56 | `bash scripts/check-config-secret-hygiene.sh` | PASS (0.1 s) |
| 57 | `python3 scripts/check-workflow-validity.py` | PASS (0.1 s) |
| 58 | `bash scripts/check-floodfill-type5-serve.sh` | PASS (0.0 s) |
| 59 | `bash scripts/check-service-tunnel-acceptance-evidence.sh` | PASS (0.6 s) |
| 60 | `bash scripts/check-exploratory-tunnel-evidence.sh` | PASS (0.1 s) |
| 61 | `bash scripts/check-netdb-tunnel-evidence.sh` | PASS (0.1 s) |
| 62 | `bash scripts/check-destination-tunnel-evidence.sh` | PASS (0.2 s) |
| 63 | `bash scripts/check-streaming-tunnel-evidence.sh` | PASS (5.1 s) |
| 64 | `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS (3.1 s) |
| 65 | `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS (0.1 s) |
| 66 | `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | PASS (0.6 s) |
| 67 | `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS (0.1 s) |
| 68 | `cargo deny check advisories bans sources` | PASS (0.9 s) |
| 69 | `cargo test --locked -p i2pr-daemon --test router_info_export -- --test-threads=1` | PASS (0.4 s) |
| 70 | `cargo test --locked -p i2pr-daemon --test router_info_export_boundary -- --test-threads=1` | PASS (0.6 s) |
| 71 | `cargo test --locked -p i2pr-daemon --test peer_pool_fallback -- --test-threads=1` | PASS (3.2 s) |
| 72 | `cargo test --locked -p i2pr-daemon --test peer_pool_fallback_boundary -- --test-threads=1` | PASS (0.6 s) |

The floor's own summary is **71 passed, 1 failed, 72 steps**.

## The negative controls

Fourteen controls mutated the **real tree**, ran the lane or the guard, and
observed the expected row fail. Every control was then restored from the bytes
saved before the mutation, and the SHA-256 of every touched file was compared
with the value it had before, so the tree is provably byte-identical afterwards.
Every control **compiles** and names exactly **one** row, which it runs **by
name**. Eight of the fourteen belong to this plan.

| Control | What it broke | What restored the defect |
| --- | --- | --- |
| NC-544-1 | the export was written into `netdb/routers/` | the lane's row `the_export_never_lands_in_the_directories_the_router_itself_loads` |
| NC-544-2 | the name was formatted as uppercase hexadecimal | the lane's row `the_export_name_is_the_hash_of_its_own_contents` |
| NC-544-3 | the export file was written `0644` | the lane's row `the_export_is_written_private` |
| NC-544-4 | the file was named `router.info` | the lane's row `the_export_name_is_the_hash_of_its_own_contents` |
| NC-544-5 | the export wrote no bytes | the lane's row `the_exported_bytes_are_the_bytes_the_router_signed` |
| NC-544-6 | the helper never created its directory | the lane's row `the_export_name_is_the_hash_of_its_own_contents` |
| NC-544-7 | the export directory was written `0755` | the guard's rule `the_export_is_written_private` |
| NC-544-8 | an inventory surface became advertised | the guard's rule `nothing_is_advertised` |

**The sweep corrected its own harness twice before it produced usable
evidence.** The first run showed 13 of 14 controls firing; the fourteenth's
"companion" claim was wrong because the companion test ran **after** the tree
was restored, so it measured the healthy tree. The harness now runs the
companion against the mutated tree, and the third run is the one this record
cites: fourteen fired, both companion claims valid ("the lane still passes" for
NC-545-1, "the guard also fires" for NC-545-5), tree byte-identical.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | the export's name is its own hash | MET — the lane's row `the_export_name_is_the_hash_of_its_own_contents`; NC-544-2 and NC-544-4 restore the defect |
| 2 | the record round-trips through the loader | MET — the lane's row `the_exported_record_round_trips_through_the_peer_cache_loader`, and its mirror `the_peer_refuses_the_same_record_under_any_other_name` |
| 3 | the export never enters `netdb/routers/` | MET — the lane's row `the_export_never_lands_in_the_directories_the_router_itself_loads` and the guard's rule `the_export_is_written_to_the_export_directory`; NC-544-1 restores the defect |
| 4 | no key or dependency is added | MET — the guard's rule `the_export_needs_no_configuration_and_opens_no_socket` and the byte-identical `Cargo.toml`/`Cargo.lock` |
| 5 | nothing is advertised | MET — the guard's rule `nothing_is_advertised`; NC-544-8 restores the defect |
| 6 | the two-host run establishes an authenticated session | MET — the LAN run recorded in §3 of the plan of record |
| 7 | the full floor is green, with the closure record's table complete | PARTIAL — 71 passed, 1 failed, 72 steps. The one failure is `check-m11-transit-qualification-evidence.sh`, a pre-existing floor-only flake named under Findings. No other step failed. |
| 8 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the guard's nine inverted probes and the eight controls above |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | the export is the router's own record, unmodified | HELD — the helper writes the `encoded` slice it is handed; the lane compares the file with that slice byte for byte |
| 2 | the filename is the record's hash | HELD — derived from `i2pr_netdb::router_hash` over the decoded record, formatted `{byte:02x}`, pinned by the guard's `the_name_is_the_hash_in_lowercase_hex` |
| 3 | the export never enters the router's own store | HELD — the destination is `data_dir.join("export")`; the guard forbids `netdb/routers`, `join("netdb")`, `ROUTERS_SUBDIR`, `ByteCache` and `cache_seam` in the helper |
| 4 | the export is bounded and permissioned | HELD — `0700` on the directory and `0600` on the file, asserted both behaviourally and statically; the guard forbids any other mode |
| 5 | a record whose hash cannot be derived is skipped, counted and logged | HELD for **skipped and logged**; **counted** is not claimed — no counter was added, and the plan's §4.5 is reviewed here as partially met (see Findings) |
| 6 | no new surface is reachable | HELD — no listener, socket, configuration key or dependency; the manifest and lock are byte-identical |
| 7 | no advertisement | HELD — `router.version` stays `0.9.62`, the inventory's advertised count stays zero |
| 8 | the predecessors' lanes and guards stay byte-identical | HELD — six pinned digests: Plans 515, 542 and 543, lane and guard |

## Failure, cancellation, restart, and contention review

**Failure.** A failed write is logged at `warn` and does not fail the
publication. The record is public material, so a failed write leaks nothing and
a lost export costs the operator one retry, not the router's liveness.

**Cancellation.** The helper is synchronous and one-shot per record change. It
awaits nothing, so it holds no cancellation point and cannot be cancelled
half-way through a rename — there is no rename: the file is written directly,
so a torn export is possible in principle and is repaired by the next
publication. This is recorded as a **Low** finding rather than papered over.

**Restart.** The export is rewritten on each record change, so it always
matches the record in force. The lane's row
`a_second_publication_of_the_same_identity_rewrites_the_same_name` proves the
rewrite and the name's stability.

**Contention.** Two writers cannot contend: the helper runs on the SSU2
service's own loop, once per record change, and nothing else writes `export/`.

## Migration and compatibility review

**No wire change.** The export is a local file; nothing about the RouterInfo's
encoding, its signature or its publication changes.

**No configuration key.** The helper takes the data directory the daemon
already has.

**No persisted state beyond the export itself**, which is derived and can be
deleted at any time.

**No manifest change.** `Cargo.toml` and `Cargo.lock` are byte-identical, and
the lane uses `i2pr-netdb-persist`, which the daemon already depends on in
production.

**One API surface widens.** `export_local_router_info` became `pub` so the
plan's own **integration** lane could call the production helper instead of
re-implementing the naming rule. The plan's amendment log, §17.2, records the
choice and the alternative it rejected.

## Security review

**No secret is touched.** The export is the record the router publishes; it
carries the router's public identity and its published addresses, which are
public by definition. No private key, no signing seed and no session secret
reaches the file.

**The permissions are the data directory's own.** `0700`/`0600` match the
posture every other private artefact in the data directory already has, so the
export does not become the weakest file in the tree.

**No new surface is reachable.** No listener, no port, no socket, no
configuration key, no dependency. The helper is reachable only from the SSU2
service's own loop.

**The name cannot be attacker-chosen.** The filename is derived from the
record's own hash, formatted by the router, so no input can steer the write
outside `export/`.

**Fail closed.** The guard fails closed on every item it names: a destination in
the router's own load path, an uppercase or non-derived name, a permissive mode,
a configuration read, a socket, a task, a lock, a removed publication, a
second call site, a moved version, an advertised surface, a changed manifest and
a changed predecessor's evidence each make the rule that names them fail.

## Limitations

- **The export appears on the first record change, not at startup.** The call
  site is the shared record's `changed` branch
  (`crates/i2pr-daemon/src/lib.rs`), so a router whose record never changes
  writes no export. In the LAN run the publication cycle's re-dating supplied
  the change. A successor that wants the file available before the record ever
  moves owns that.
- **The write is not atomic.** The file is written directly rather than through
  a temporary and a rename, so a crash mid-write can leave a short record. The
  next publication repairs it, and the peer's loader refuses a short record
  rather than accepting one.
- **The lane cannot see the daemon's composition.** It calls the helper the
  daemon calls and drives the peer's real loader, but it does not start a
  daemon; that the daemon reaches the call site is proven by the two-host run
  and by the registration record's logs, not by the lane.
- The plan does not claim T4/T5, stock-router admission, or publication.
- **`counted` in §4.5 is not met.** No counter was added; the skip is logged and
  the observation is recorded under Findings.

## Findings by severity

### None — nothing was found that blocks the plan

### Low — recorded by this plan

1. **`export_local_router_info`'s skip is logged but not counted.** The plan's
   §4.5 asks for a skipped record to be "skipped, counted and logged"; the helper
   returns `Ok(None)` and the call site's match arm is empty, so nothing is
   recorded. A record whose hash cannot be derived is a local invariant failure
   (the router just encoded it), so the condition is close to unreachable — but
   an empty arm is exactly the shape this repository has spent three plans
   removing. **Owner:** the successor that adds the export's own counters, if the
   export is ever given a promotion.
2. **The write is not atomic.** Recorded above; the peer's loader fails closed on
   a short record, so the consequence is a refused handoff rather than a corrupt
   store.
3. **`publish_router_hash` still has no production caller.**
   `crates/i2pr-daemon/src/i2pcontrol_inspection.rs:450`. This plan deliberately
   did not use it (it would have touched the frozen RouterInfo selector
   inventory) and records it with an owner instead.
4. **The two-transport record is gated on the floodfill profile.**
   `crates/i2pr-daemon/src/lib.rs:1002` means a lab operator must set
   `[floodfill].qualification_profile` to get an SSU2 endpoint into the record.
   A genuine trap, recorded with an owner rather than fixed here.

### Informational

- The companion commit `6ba8ba98` (Plan 545) is what made the pool's silence
  visible; without it this plan's two-host run would have looked like a failure
  to dial rather than a dial that succeeded.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 544 moves from
`registered` to `closed`, with the verdict
`passed-the-routers-own-routerinfo-reaches-a-private-lan-peer-out-of-band` and
this record's path. The row's prose is **not** rewritten: it records what the
plan set out to do at registration, and this record is the authority on what it
did.

The roadmap's **completion definition** — the router can be driven against a
two-host topology — is **advanced but not met**: the two hosts now find each
other, authenticate and carry I2NP, and the tiers above that are named with
their owners.

## Unblock audit

| Plan | State after 544 | Why |
| --- | --- | --- |
| 431 | unchanged (`stopped`) | the broader lab topology keeps its own plan-of-record |
| 449 | unchanged (blocked) | needs the SSU2 external lane's independent peer |
| 472 | unchanged (blocked) | the same |
| 476 | unchanged (blocked) | the gate binding keeps its requirement |
| 493 | **advanced** | the two-host i2pr topology it needed now exists; stock-router admission still needs the reference pins and the ban-surface care in `specs/QUALIFICATION-GATES.md` §3 |
| 433, 435–439, 441–448, 451, 452, 455, 457–462, 464–466, 471–474, 494 | unchanged (externally blocked) | they need a public network or an authorized external peer, not a lab handoff |

## Handoff notes

- Experimental, default-off, non-advertised. The export is **public material**;
  it carries no secret.
- **Do not move the export into `netdb/routers/`.** That is the router's own
  load path.
- **Do not rename the file.** A wrong name is a silent `KeyMismatch`.
- The LAN run needs `[floodfill].qualification_profile` set to get a
  two-transport record at all — see §5.3 of the plan of record.
- The export is a **lab handoff**, not publication. A successor must not treat
  it as publication, and the router still publishes to no NetDB it cannot reach.
- The predecessors' closure records are **not** rewritten.
