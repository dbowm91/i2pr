# Plan 285 — M12 peer-testing I2NP surface and version declaration

Status: **retained-m12-peer-testing-implemented-and-declared-pending-mixed-router-evidence**

Classification: capability correction. Local implementation and the version-declaration decision are
complete; the one unmet checklist step is the mixed-router evidence that only the exact-pinned
external lane can produce.

## Outcome in one line

`i2pr` now implements the full I2NP message surface the pinned reference enumerates, including the
peer-testing type, and declares the version level that surface substantiates — which removes the
mechanical reason the reference refused floodfill eligibility, with the external validation named as
the remaining gap.

## Requirement-to-evidence matrix

| requirement | evidence | status |
| --- | --- | --- |
| `TunnelTest` type at wire code 231, not a range guess | `crates/i2pr-proto/src/i2np/header.rs`, pinned by `tunnel_test_identifier_is_pinned_to_231` | met |
| exactly 12-byte body, exact consumption | `TunnelTestMessage`, `TUNNEL_TEST_BODY_SIZE`, `tunnel_test_rejects_wrong_body_length` | met |
| reference-derived golden vector | `tunnel_test_standard_golden_round_trip`; checksum byte independently verified as `SHA-256(body)[0]` = 0x41 | met |
| short-transport path decodes the same body | `tunnel_test_short_transport_round_trip` | met |
| typed dispatch carrying authenticated identity | `RouterI2npOutcome::PeerTest`; `inbound_peer_test_yields_echo_of_unchanged_fields` | met |
| inbound probe gets a bounded, honest answer | `PeerTestEcho`, `PeerTestEcho::encode`, `echo_encodes_a_fresh_message_repeating_the_probe` | met |
| answer is not an echo oracle for other types | `non_peer_test_message_owes_no_echo` | met |
| originated probe matched or reported timed out | `PeerTestTracker`; `completed_probe_reports_local_elapsed_time`, `late_answer_is_reported_as_timed_out` | met |
| capacity bounded, refuses rather than evicts | `table_refuses_at_ceiling_and_does_not_evict` | met |
| retention observable | `expiry_releases_aged_entries_and_is_observable` | met |
| zero timeout rejected | `zero_timeout_is_rejected` | met |
| declaration tracks the implemented surface | `controlled_router_version_matches_the_implemented_i2np_surface` | met |
| transit owner does not route a peer test into tunnel tables | `RouterI2npOutcome::PeerTest` arm in `crates/i2pr-daemon/src/transit_owner.rs` | met |
| mixed-router validation of the changed claim | — | **not met; named gap** |

## The version decision, and what it rests on

The reference pairs the 0.9.62 level with peer testing: `NETDB_MIN_PEER_TEST_VERSION` is 0.9.62, and
`RouterInfo.cpp:468-473` clears a router's SSU2 peer-testing address caps below that level.
`RouterInfo::IsEligibleFloodfill` requires at least 0.9.62 with no high-bandwidth alternative.
`specs/protocols/02-i2np.md` defines `router.version` as an I2NP feature/API version, so the
declaration is substantiated by the implemented I2NP message surface rather than by a release string.

With `TunnelTest` implemented, the surface is the complete enumeration at
`libi2pd/I2NPProtocol.h:111-125`, so the declaration is 0.9.62. The consistency test fails if the
declaration is raised above the surface, and the constant is documented as never editable to satisfy
a peer's admission gate.

## Commands run, with outcomes

Local, on the closing tree:

```text
cargo fmt --all --check                                        ok
cargo check --locked --workspace --all-targets                  ok
cargo test --locked --workspace --all-targets -- --test-threads=1   3245 passed, 31 ignored, 0 failed
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   ok
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps   ok
cargo test --locked --workspace --doc                           ok
bash scripts/check-dependency-direction.sh                     ok
bash scripts/check-runtime-boundaries.sh                       ok
bash scripts/check-service-tunnel-boundaries.sh                ok
bash scripts/check-fixture-manifest.sh                         ok
bash scripts/check-ntcp2-vectors.sh                            ok
bash scripts/check-ssu2-vectors.sh                             ok
bash scripts/check-i2cp-vectors.sh                             ok
bash scripts/check-constrained-host-lane-boundary.sh           ok
bash scripts/check-m11-transit-boundaries.sh                   ok
bash scripts/check-m11-transit-qualification-evidence.sh       ok
bash scripts/check-sam-acceptance-evidence.sh                  ok
bash scripts/check-ssu2-acceptance-evidence.sh                 ok
bash scripts/check-i2cp-acceptance-evidence.sh                 ok
bash scripts/check-service-tunnel-acceptance-evidence.sh      ok
bash scripts/check-exploratory-tunnel-evidence.sh              ok
bash scripts/check-netdb-tunnel-evidence.sh                    ok
bash scripts/check-destination-tunnel-evidence.sh              ok
bash scripts/check-streaming-tunnel-evidence.sh                ok
bash scripts/check-m6-mixed-router-acceptance-evidence.sh      ok
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   18 tests, OK
cargo deny check advisories bans sources                        ok
```

Hosted ordinary CI: recorded on the closing commit in the registry row for this plan.

Not run: the exact-pinned i2pd external lane. It requires a bounded attempt against the frozen pin
and is the named remaining gap, not part of this local pass.

## Invariant, failure, migration, and security review

- **Invariant.** The declared version is pinned to the implemented I2NP surface by a test. A future
  removal of `TunnelTest` or a raise of the constant without the surface breaks the build, so the
  claim cannot silently outrun reality.
- **Failure semantics.** `PeerTestTracker` refuses at capacity rather than evicting, so a peer's
  answer can never be matched against a different probe. A late answer reports `TimedOut` rather than
  a healthy round trip, so an unanswered probe is never presented as working. `expire` is observable.
- **Migration.** None. No stored state changes; `CONTROLLED_ROUTER_VERSION` affects only controlled
  publication, which is loopback-only and non-advertised.
- **Security.** The probe timestamp is treated as an opaque value, never read as a local clock, so no
  duration is derived from peer-supplied bytes. No key material, identity, or payload crosses this
  surface; the responder answer is derived from the authenticated peer and link that delivered the
  probe, never from a caller-supplied identity. The probe identifier and timestamp are peer metadata
  and are counted, never recorded as evidence.

## Findings by severity

- **Low.** The I2NP `MessageType` table and the reference enumeration can drift if the specification
  adds types. The `Unknown` fallback and the pinned-identifier test bound the blast radius; a periodic
  re-audit against the pinned specification is the follow-up, not a code defect.

## Documentation

- `specs/protocols/02-i2np.md` already defines the `router.version` semantics; no edit was needed.
- `specs/support.toml` records the declaration and this plan.
- `plans/implementation/floodfill/285-*.md` carries the checklist result with its named gap.
- Plan 278's stop record and Plan 284's corrective were corrected to the two-gate analysis they
  actually support.

## Limitations and remaining risks

- No external interoperability row is claimed from this plan. The mechanical admission gates are
  cleared in code, but the reference has not yet been observed admitting the record and treating it
  as a floodfill.
- The mixed-router checklist step is unmet and is recorded as such rather than asserted.
- Plan 278 stays stopped and Plan 279 stays blocked.

## Unblock audit

`plans/registry.md` lists Plan 278 as the plan blocked behind this corrective, and Plan 279 as blocked
behind Plan 278. Plan 278's only hard dependency that this plan owned was the eligibility gate, which
is now closed locally. Plan 278's other dependencies are unchanged and were already satisfied when it
was registered, so Plan 278 moves to `ready` on the strength of this record, with its external matrix
still unexecuted and its own attempt budget untouched. Plan 279 remains blocked on Plan 278 and is
not unblocked here.

## Roadmap disposition

Plan 285 is retained pending the external matrix. Plan 278 is unblocked to `ready` and is the next
executable plan: one bounded exact-pinned attempt to observe whether the reference now admits the
controlled record and treats it as a floodfill.
