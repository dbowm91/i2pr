# Plan 197 status — M6 PQ SSU2 option support corrective (tolerant parse only)

Status: **`implementation-landed-parser-tolerance-static-floor-green-pending-plan196-external-re-run`**.

Plan of record:
[`plans/implementation/mixed-router-interop/197-m6-pq-ssu2-option-support-corrective.md`](../../implementation/mixed-router-interop/197-m6-pq-ssu2-option-support-corrective.md).

Plan 197 is the only registered follow-up that can lift Plan 196 to
`passed-m6-java-controlled-first-run-topology-corrective`. Plan 196
landed the controlled Java first-run topology corrective but stopped
at §10.B on the first counted external run:
`Ssu2RouterAddress::parse` rejected the exact-pinned Java I2P 2.13.0
`pq=4,3` KEM-scheme option as `UnknownOption` and the
`destination_message_plane_against_java` driver panicked before any
authenticated SSU2 transcript began. Plan 197 owns a narrow
parser-only tolerance for the `pq` option with a typed
`Ssu2PqKem`/`PqCapabilities` surface; it does not implement
ML-KEM, does not publish `pq`, and does not claim a third PQ
milestone.

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187_local_product         = retained-passed
plan_188_short_build_corrective = retained-passed
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-boundary-diagnosis-retained
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective

m6_second_family_java              = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
m6_ssu2_pq_option_tolerance        = landed-via-plan197-typed-parser-surface
milestone6_i2pd_streaming_interop  = passed-via-plan193
milestone6_java_mixed_router_interop = in-progress-resume-194
milestone6_interoperable           = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance       = not-yet-closed

next_executable_plan    = 194 (resume §5.3 tunnel-over-tunnels + §5.4(b)/(c) bidirectional destination delivery + §5.5 Streaming qualification against the proven controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge)
resume_after_plan196   = 194 (Java second-family qualification) -> 195
remaining_sequence      = resume-194 -> 195
```

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187_local_product         = retained-passed
plan_188_short_build_corrective = retained-passed
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-boundary-diagnosis-retained
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective

m6_second_family_java              = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
milestone6_i2pd_streaming_interop  = passed-via-plan193
milestone6_java_mixed_router_interop = in-progress-resume-194
milestone6_interoperable           = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance       = not-yet-closed

next_executable_plan    = 194 (resume §5.3 tunnel-over-tunnels + §5.4(b)/(c) bidirectional destination delivery + §5.5 Streaming qualification against the proven controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge)
resume_after_plan197   = 196 (re-run external lane; session-established-java row must flip)
remaining_sequence      = 196-execute -> resume-194 -> 195
```

## Why Plan 197 exists

Plan 193 closed the i2pd first-family Streaming qualification
(33/33 rows passing twice on exact head
`3687189de651ba2b2d3483cbfded2c8e4a7278ef`). Plan 194 then landed
the Java second-family fetch/build, runner, external driver,
cross-family aggregator, static checker, and hosted workflow
scaffolding, but its first real Java run exposed a reference-startup
topology blocker (random UDP port, public reseed, mutated cache).
Plan 196 owns the controlled-topology corrective:

1. Out-of-tree `tests/integration/m6-interop/java/ControlledRouter.java`
   test-only launcher invokes the stock
   `net.i2p.router.Router(Properties)` + `setKillVMOnEnd(false)` +
   `runRouter()` lifecycle (the exact-pinned upstream `MultiRouter`
   precedent).
2. `tests/integration/m6-interop/run-java.sh` rewritten at the
   topology/startup boundary so reserved fixed loopback Java
   SSU2/SAM/I2CP ports and the controlled `Properties` set drive
   the JVM with exact-pinned upstream keys (`router.reseedDisable=true`,
   `router.floodfillParticipant=true`, `i2np.ntcp.enable=false`,
   etc.).
3. `scripts/check-m6-mixed-router-acceptance-evidence.sh`
   extended with Plan 196 §7 controlled-topology invariants
   (`i2p.vmCommSystem=true` rejected, obsolete Plan 194 keys
   rejected, cache mutation rejected, non-loopback `i2p.reseedURL`
   rejected, `|| true` forgiveness rejected).

The first counted external run cleared every Plan 196 §5
topology invariant and then drove
`crates/i2pr-daemon/tests/java_tunnel_external.rs::destination_message_plane_against_java`
through its `--ignored --exact` invocation. The driver failed at
`verify_reference_router_info` with
`Ssu2ServiceError::InvalidIdentity` because the exact-pinned Java
I2P 2.13.0 `UDPTransport.addSSU2Options` unconditionally emits
`pq=4,3` (ML-KEM-768 + ML-KEM-512) and `Ssu2RouterAddress::parse`'s
default arm rejects unknown options at
`crates/i2pr-transport-ssu2/src/address.rs:885`.

Plan 196 stops here per §10.B and retains the topology
evidence. Plan 197 is the narrow follow-up.

Pinned reference contracts:

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
role   = mandatory Plan 161 first-family SSU2 reference (no pq option)
        → must remain green under Plan 197

Java I2P 2.13.0
commit = 9134f808337b401e8e53c73734c81fab04280c9d
role   = mandatory Plan 194 second-family reference (unconditionally publishes pq=4,3)
        → must be tolerated by the parser under Plan 197
```

## Why a typed `PqCapabilities` and not just `continue`

The NTCP2 parser precedent at
`crates/i2pr-transport-ntcp2/src/address.rs:714-718` accepts `pq`
with `continue,` (type-free ignore). The SSU2 crate already names
that precedent explicitly in its module docs
(`crates/i2pr-transport-ssu2/src/address.rs:11-19`) and Plan 196
status §6 names the eventual interface:
"type-safe (`PqScheme` enum with at least `MlKem512`, `MlKem768`,
and a sentinel for unknown PQ schemes), bounds-checked (fixed
ML-KEM-512 / ML-KEM-768 public-key lengths), and never re-derives
the canonical wire bytes."

Plan 197 keeps the accept-and-do-not-negotiate semantics but
takes the typed step so a future PQ crypto plan of record can
read the parsed schemes without re-parsing the wire string.
i2pr's session layer remains classical X25519 only by the Plan
156/160/161 contract; Plan 197 does not propose, add, or otherwise
alter a single PQ key-exchange path.

## Scope (locked)

```text
i2pr SSU2 v2 session layer    = classical X25519 only (Plan 156/160/161 unchanged)
i2pr SSU2 address parser      = tolerant of pq=X,Y,Z option; surfaces typed PqCapabilities
i2pr SSU2 publication path    = never emits pq
i2pr RouterInfo RouterAddress = no pq today; never adds pq under Plan 197
ML-KEM-512 / ML-KEM-768 crypto = NOT implemented; NOT claimed; NOT silently enabled
Ssu2RouterAddress::address_class semantics = unchanged
```

## Required implementation (preview)

Primary file:

```text
crates/i2pr-transport-ssu2/src/address.rs
crates/i2pr-transport-ssu2/src/publication.rs
crates/i2pr-transport-ssu2/src/lib.rs              (pub use)
crates/i2pr-daemon/tests/ssu2_daemon_preflight.rs   (Plan 197 row 20)
crates/i2pr-daemon/tests/java_tunnel_external.rs   (Plan 197 row 21)
scripts/check-m6-mixed-router-acceptance-evidence.sh (Plan 197 §5.6 §8)
```

Plan 197 §5.1-§5.7 detail the typed surface and the wire-form
invariants. Plan 197 §5.5 lists 21 required test rows. Plan 197
§5.6 specifies four additions to the existing
`scripts/check-m6-mixed-router-acceptance-evidence.sh` and zero
additions to `scripts/check-ssu2-acceptance-evidence.sh`. Plan
197 §6 names the first-family regression matrix that must remain
green without modification. Plan 197 §7 names the
second-family external proof: the existing Plan 196 lane with
the existing driver, with the `external-session-established-java`
row transitioning from `failed` (Plan 194-§11 stop provenance)
to `passed` (Plan 197 `session-established` evidence key).

## Implementation record

Implementation landed on the `plan197-m6-pq-ssu2-option-support-corrective`
branch off the Plan 196 implementation commit. The following
artefacts are in place on the closing head:

- `crates/i2pr-transport-ssu2/src/address.rs`:
  - `PQ_OPTION: &str = "pq"` constant at lines ~58-60.
  - `MAX_SSU2_PQ_SCHEMES: usize = 8` constant at lines ~62-68.
  - `Ssu2PqKem` enum (`MlKem512`, `MlKem768`, `Unknown(u8)`) with
    `from_code(value: u8) -> Self` at lines ~158-200.
  - `PqCapabilities` value type with `empty()`, `from_parts()`,
    `schemes()`, `as_wire()`, `is_supported()` at lines ~202-260.
  - `Ssu2RouterAddress::pq_capabilities: PqCapabilities` field
    + `pq_capabilities()` accessor at lines ~470-540.
  - `ParsedOptions::pq` field + `PQ_OPTION => store(...)` parser
    arm at lines ~890-940.
  - `parse_pq_capabilities(value: &str) -> Result<PqCapabilities, Ssu2AddressError>`
    function next to `parse_capabilities`, with the wire-form
    invariants from §5.2 (empty, comma-split, ASCII-digit-only,
    bounded at `MAX_SSU2_PQ_SCHEMES`, wire string retained
    verbatim).
  - `from_parsed` initialises `pq_capabilities` via
    `parse_pq_capabilities` or `PqCapabilities::empty()` for the
    `options.pq == None` i2pd-first-family path.
  - 18 new in-file `#[cfg(test)]` unit rows (rows 1-18 per §5.5):
    `parses_java_high_mtu_pq_options`,
    `parses_java_low_mtu_pq_option`,
    `parses_mlkem768_only_pq_option`,
    `parses_empty_pq_option_as_empty_capabilities`,
    `parses_absent_pq_option_as_empty_capabilities`,
    `parses_unknown_pq_scheme_as_unknown`,
    `parses_mixed_known_and_unknown_pq_schemes`,
    `rejects_too_many_pq_schemes`,
    `rejects_pq_option_with_leading_comma`,
    `rejects_pq_option_with_trailing_comma`,
    `rejects_pq_option_with_whitespace`,
    `rejects_pq_option_with_non_digit_chars`,
    `rejects_pq_option_with_negative_sign`,
    `rejects_pq_option_with_decimal_point`,
    `parses_pq_alongside_full_direct_options`,
    `rejects_duplicate_pq_option`,
    `i2pd_style_address_without_pq_parses_to_empty_caps`,
    `pq_capabilities_canonical_field_never_re_derived`.
- `crates/i2pr-transport-ssu2/src/publication.rs`:
  - `publication_never_emits_pq` regression (row 19) asserting
    both `PublicationOutcome::Direct` and
    `PublicationOutcome::Firewalled` snapshots carry no `pq`
    option. The publication path is otherwise unchanged.
- `crates/i2pr-transport-ssu2/src/lib.rs`:
  - `pub use` re-exports `PqCapabilities`, `Ssu2PqKem`,
    `MAX_SSU2_PQ_SCHEMES` alongside the existing
    `Ssu2RouterAddress` surface.
- `crates/i2pr-runtime/src/lib.rs`:
  - The existing `pub use i2pr_transport_ssu2::{IntroKey,
    Ssu2PublicKey, Ssu2RouterAddress, constants}` line is
    extended to also re-export `Ssu2PqKem` and `PqCapabilities`
    so the daemon can inspect the typed value through the
    Plan 184 daemon-owned runtime surface without pulling in
    `i2pr-transport-ssu2` directly.
- `crates/i2pr-daemon/tests/ssu2_daemon_preflight.rs`:
  - `reference_i2pd_routerinfo_has_no_pq_capabilities` (row 20)
    CI-fast regression that constructs an i2pd-style RouterInfo
    via `Mapping::from_entries` (no `pq` key) and asserts
    `pq_capabilities().schemes().is_empty()` through the
    `verify_reference_router_info` consumer. The external lane
    stays `#[ignore]`d.
- `crates/i2pr-daemon/tests/java_tunnel_external.rs`:
  - `java_pq_capabilities_surfaced` (row 21) CI-fast regression
    that constructs a Java 2.13.0-style RouterInfo via
    `Mapping::from_entries` with the canonical `pq=4,3` option
    and asserts the parsed `pq_capabilities().schemes()` is
    `[Ssu2PqKem::MlKem768, Ssu2PqKem::MlKem512]`. The external
    lane stays `#[ignore]`d.
- `scripts/check-m6-mixed-router-acceptance-evidence.sh`:
  - New §9 block enforces the four §5.6 invariants: PQ_OPTION
    parser arm present, positive `pq=4,3` unit-test row,
    `pq_capabilities()` accessor present, `publication.rs`
    never emits `pq`, and the `lib.rs` re-exports include
    `Ssu2PqKem`/`PqCapabilities`/`MAX_SSU2_PQ_SCHEMES`.

## Plan 196/197 interaction

Plan 197 is the only registered follow-up that can flip Plan 196
to `passed-m6-java-controlled-first-run-topology-corrective`.
Plan 196 retains ownership of the controlled-topology harness
(`ControlledRouter.java`, `run-java.sh`, the §5 invariants, the
§7 static checker). Plan 197 owns the SSU2 `pq` parser tolerance.
On Plan 197 pass, Plan 196 closure records the counted external
lane result by appending the Java external evidence to the
existing §6 row set, no source file is rewritten beyond a
single §11 status append.

## Registration source floor

Plan 197 was registered from:

```text
i2pr main = de4b390 (Plan 196 implementation commit)
routine CI = 34767155352 (success)
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-by-plan196-topology-corrective
plan_195 = registered-blocked-by-plan194
plan_196 = in-progress-corrective-implementation-landed-static-checks-green-stopped-at-§10B-authenticated-ssu2-pq-option-rejection
workspace floor recorded by Plan 196 = 2312 passed / 8 ignored
plan_196_first_counted_external_run = failed at
  java_tunnel_external.rs:183:54 verify java RouterInfo: InvalidIdentity
plan_196_first_counted_external_evidence = target/interop/m6-java-evidence/external-driver.log
```

Workspace quality floor recorded by Plan 196 on the registration
commit:

```text
cargo fmt --all --check                              OK
cargo check --locked --workspace --all-targets       OK
cargo test --locked --workspace --all-targets \
  -- --test-threads=1                               2312 passed / 8 ignored
cargo clippy --locked --workspace --all-targets \
  --all-features -- -D warnings                     no issues
RUSTDOCFLAGS="-D warnings" cargo doc --locked \
  --workspace --no-deps                             OK
cargo test --locked --workspace --doc               0 passed (16 suites)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh  OK (11 guarded labels, two-family pins verified)
bash scripts/check-ssu2-acceptance-evidence.sh       OK
cargo deny check advisories bans sources             OK
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'   Ran 153 tests OK
```

The Plan 197 implementation must add the parser-tolerance rows
to the same floor without shrinking the existing tests.

### Plan 197 implementation floor

On the `plan197-m6-pq-ssu2-option-support-corrective` closing head:

```text
cargo fmt --all --check                              OK
cargo check --locked --workspace --all-targets       OK
cargo test --locked --workspace --all-targets \
  -- --test-threads=1                               2333 passed / 8 ignored
                                              (Plan 197 adds 21 rows:
                                               18 in address.rs,
                                                1 in publication.rs,
                                                1 in ssu2_daemon_preflight.rs,
                                                1 in java_tunnel_external.rs;
                                               no existing rows regressed)
cargo test --locked -p i2pr-transport-ssu2 --all-targets        188 passed / 0 ignored
cargo test --locked -p i2pr-runtime --lib                        70 passed
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight \
  -- --test-threads=1                                            6 passed / 1 ignored
cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  -- --test-threads=1                                            1 passed / 1 ignored
cargo clippy --locked --workspace --all-targets \
  --all-features -- -D warnings                                  no issues
RUSTDOCFLAGS="-D warnings" cargo doc --locked \
  --workspace --no-deps                                          OK
cargo test --locked --workspace --doc                            0 passed (16 suites)
bash scripts/check-dependency-direction.sh                       ok
bash scripts/check-runtime-boundaries.sh                         OK
bash scripts/check-service-tunnel-boundaries.sh                  OK
bash scripts/check-fixture-manifest.sh                            OK
bash scripts/check-ntcp2-vectors.sh                              OK
bash scripts/check-ssu2-vectors.sh                               OK
bash scripts/check-i2cp-vectors.sh                               OK
bash scripts/check-ntcp2-interoperability.sh                     OK
bash scripts/check-constrained-host-lane-boundary.sh             OK
bash scripts/check-sam-acceptance-evidence.sh                    22 rows command-derived
bash scripts/check-ssu2-acceptance-evidence.sh                   15 rows command-derived
bash scripts/check-i2cp-acceptance-evidence.sh                   24 rows command-derived
bash scripts/check-service-tunnel-acceptance-evidence.sh         29 rows command-derived / 2 blocked
bash scripts/check-m6-mixed-router-acceptance-evidence.sh        11 guarded labels + Plan 197 §8 pq parser tolerance invariants
bash scripts/check-streaming-tunnel-evidence.sh                  33 guarded labels
bash scripts/check-exploratory-tunnel-evidence.sh                12 guarded labels
bash scripts/check-netdb-tunnel-evidence.sh                      12 guarded labels
bash scripts/check-destination-tunnel-evidence.sh                21 guarded labels
cargo deny check advisories bans sources                         OK
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'   Ran 153 tests OK
```

No existing row regressed. The Plan 161 i2pd 2.61.0 first-family
SSU2 matrix continues to pass locally; the
`reference_i2pd_routerinfo_has_no_pq_capabilities` row exercises
the i2pd-style (no `pq`) path and surfaces empty schemes. The
`java_pq_capabilities_surfaced` row exercises the Java 2.13.0
(`pq=4,3`) path and surfaces `[MlKem768, MlKem512]` through the
`verify_reference_router_info` consumer — proving the §10.B
defect is fixed at the parser layer.

## Handoff rule

The parser tolerance is implemented and the local floor is
green. Run the exact-pinned Java external lane through the
existing `tests/integration/m6-interop/run-java.sh` harness.
On the counted run:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs::destination_message_plane_against_java
  panic site: java_tunnel_external.rs:183 (verify_reference_router_info)
  panic message: verify java RouterInfo: InvalidIdentity
  → must transition to: session-established
  → record_stop plan194-java-stop must clear
```

The cross-family `external-session-established-java` row bound
by `scripts/check-m6-mixed-router-acceptance-evidence.sh`'s
`cross_family_row` helper must flip from `failed` to `passed`.
The Java cleaner evidence must continue to surface `external-reference-verified-java=true`.

If the counted external lane reaches any further boundary
beyond authenticated SSU2 preflight (tunnel-build, NetDB lookup,
LeaseSet2 publication, raw destination delivery, Streaming),
the corresponding Plan 194 §5 layers resume; that becomes the
Plan 194 active scope, not a Plan 197 deliverable.

On Plan 197 pass (i.e. after the Plan 196 external Java
re-run records `session-established`), the status authority
becomes:

```text
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_196 = in-progress-resume-external-execution-against-landed-corrective
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194

m6_second_family_java              = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
milestone6_interoperable           = not-yet-claimed

next_executable_plan = 196 (re-run external lane)
remaining_sequence   = 196-execute -> resume-194 -> 195
```

Do not claim `milestone6_interoperable` from Plan 197 alone; the
final M6 closure requires the full Plan 194 external two-family
workflow pass per Plan 194 §12.
