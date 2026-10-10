# Plan 368 status — SAM 3.3 PRIMARY/subsession shared-Destination profile

Status: **passed-sam33-primary-subsession-shared-destination-profile**

Date: 2026-10-10

Implementation revision: worktree based on `299dca08`; implementation and
closeout changes are present in the current worktree. No new commit was created
for this closeout.

Normative references: current official SAM V3 specification as refreshed in
`specs/SOURCES.md`; pinned Java I2P `2.13.0`, revision
`9134f808337b401e8e53c73734c81fab04280c9d`. Pinned i2pd `2.61.0`, revision
`635b013a612ff47278ef02acf8580a28e10e26c5`, is diagnostic only.

## Requirement-to-evidence matrix

| Plan 368 requirement | Result and evidence |
| --- | --- |
| Negotiate and represent the qualified SAM 3.3 profile | Passed. Production range is `[3.1, 3.3]`; version tests cover overlap, disjoint ranges, the `1.0–3.3` Java range, and 3.1 regression. The SAM transcript lane covers negotiated 3.1, 3.2, and 3.3 ranges. |
| One long-lived PRIMARY owns one Destination and tunnel set | Passed. `sam::tests::staged_33_profile_runs_primary_child_commands_over_loopback_tcp` exercises PRIMARY creation, child operations, and owner teardown; the shared-primary datagram and Java wire tests verify the common Destination identity. |
| STREAM, DATAGRAM1/2/3, and RAW children share that Destination | Passed locally and in the Java wire matrix. `sam::tests::primary_datagram_children_share_destination_and_route_protocols_17_through_20` covers protocol 17/19/20, RAW 18, and custom RAW 42 on the ordinary loopback path. `sam::tests::pinned_java_213_primary_datagrams_and_stream_share_one_destination` covers all child styles and DATAGRAM1/2/3 receive with a same-PRIMARY STREAM round trip. |
| Reuse canonical Streaming and protocol 17–20 owners | Passed. STREAM uses the existing `StreamingManager`; ordinary datagrams route through the existing `DatagramManager` and RAW path. Local same-primary routing and Java receive evidence passed. No parallel router data plane or managed-app host UDP endpoint was added. |
| Current unversioned send controls and Proposal 167 naming behavior | Passed. Unsupported send controls are rejected before enqueue; `NAMING LOOKUP OPTIONS=true` returns the bounded supported option set. SAM parser/state/naming tests and the complete SAM acceptance lane passed. |
| Child removal preserves siblings; PRIMARY loss closes children deterministically | Passed. SAM registry/state lifecycle tests, the loopback primary/child test, the shared-primary datagram fixture, and Java child removal/primary-control teardown passed. |
| Private managed-app SAM has the selected semantics | Passed within its private boundary. `app_gateway::tests::private_sam_primary_subsessions_are_scoped_to_the_app_instance` covers PRIMARY, STREAM child lookup, and primary teardown. `app_manager_bridge::tests::sam_datagram_service_is_typed_and_scoped_to_private_gateway` and the private datagram fixture cover bounded Send/Receive for protocols 17–20 without a host UDP endpoint. |
| Cross-app child/session attachment is impossible | Passed. The private gateway test verifies that a child ID resolves within one app instance and fails in another instance; manager bridge isolation tests and managed-app private-client boundary checkers passed. |
| Private datagrams do not grant host UDP authority | Passed. The private API is a bounded manager-protocol service; private gateway tests and `check-managed-app-private-client-seams.py`, `check-managed-app-gateway-boundary.py`, and `check-managed-app-manager-boundary.py` passed. Ordinary SAM UDP remains loopback-only. |
| Pinned Java interoperability is recorded | Passed at Java I2P 2.13.0, exact revision above. The unmodified `SAMStreamSink` row passed. The Java SAM wire probe passed PRIMARY/child matrix, DATAGRAM1/2/3 receive, and same-primary STREAM round trip. Sanitized evidence is in `target/interop/sam-368-java-evidence/evidence.tsv`; `scripts/check-sam368-java-evidence.sh` passed. |
| Pinned i2pd diagnostics and PRIMARY/MASTER disposition are recorded | Passed as non-gating diagnostic evidence at i2pd 2.61.0, exact revision above. Runtime HELLO reported 3.3 and PRIMARY creation was unsupported. Pinned-source diagnostics found MASTER spelling, STREAM-only subsession add, master-owned removal, and standalone datagram/RAW styles. PRIMARY remains normative; MASTER remains its compatibility alias. i2pd does not narrow the profile. Sanitized evidence is in `target/interop/sam-368-i2pd-evidence/evidence.tsv`; `scripts/check-sam368-i2pd-evidence.sh` passed. |
| SAM 3.1 regression behavior remains green | Passed. SAM parser/state/version regressions, the Plan 151 independent-client lane, and SAM loopback/forward/naming/stream acceptance suites passed. |
| Port-aware STREAM works on ordinary and private origins | Passed. `sam::tests::staged_33_stream_children_route_nonzero_and_maximum_ports_over_tcp` covers ports 110 and 65535 through same-primary children and proves omission of `TO_PORT` does not reach the nonzero-port listener. The private managed-app origin covers the same port and omitted-port behavior. |
| Routine, guard, and documentation floor | Passed on 2026-10-10. Exact commands and outcomes are recorded below. |

## PRIMARY and child lifecycle

```text
HELLO negotiates one connection-local version
  SESSION CREATE STYLE=PRIMARY
    -> one primary Destination / tunnel owner
       SESSION ADD STYLE=STREAM | DATAGRAM | DATAGRAM2 | DATAGRAM3 | RAW
         -> child ID and selected protocol/port policy; no new Destination
       SESSION REMOVE ID=<child>
         -> remove that child while siblings remain available
    primary control close / teardown
      -> cancel child operations and remove all children and primary resources
```

Each child is bounded by the existing SAM registry. The implementation caps a
primary at 64 child sessions (`MAX_SAM_SUBSESSIONS_PER_PRIMARY`), the SAM server
at the existing 1,024-session ceiling (default configured session/client limits
remain 16), and SAM UDP packets at 65,507 bytes with a header bounded by the
existing SAM option-value ceiling. Private manager-protocol datagrams cap IDs at
256 bytes, options at 1,024 bytes, and payloads below the 60 KiB frame ceiling.
Existing SAM line/token/value bounds remain in force. Shutdown is owner-driven;
child and primary teardown releases their owned tasks and state.

## Verification run

The complete repository routine floor from `AGENTS.md` passed locally after the
test corrections listed below. Commands were run with `rtk proxy`:

- `cargo fmt --all --check`
- `cargo check --locked --workspace --all-targets`
- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl`
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed; 2 environment-gated tests ignored as specified.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`
- `cargo test --locked --workspace --doc`
- All AGENTS.md dependency, plan/ADR uniqueness, portable API/consumer, planning unit, runtime/console, tooling/license, managed-app, service-tunnel, M11, anonymity, fixture/vector, NTCP2, SAM, SSU2, I2CP, I2PControl, ELS2, outproxy, workflow, floodfill, exploratory tunnel, NetDB, destination, Streaming, M6 mixed-router, and M12 checks passed, including their prescribed `--self-test` modes.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed (51 tests).
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — passed (15 tests).
- `cargo deny check advisories bans sources` — advisories, bans, and sources passed.

Plan-specific lanes and regressions passed:

- `bash tests/integration/sam/run-java-368.sh`
- `bash scripts/check-sam368-java-evidence.sh`
- `bash tests/integration/sam/run-i2pd-368-diagnostic.sh`
- `bash scripts/check-sam368-i2pd-evidence.sh`
- `bash tests/integration/sam/run-independent.sh`
- `bash scripts/check-sam-acceptance-evidence.sh`
- Focused SAM version, registry, datagram, private-gateway, same-primary STREAM/DATAGRAM/RAW, port-boundary, and pinned-Java tests; the SAM listener/stream acceptance suites passed serially.
- The first full-floor attempt exposed one state test still expecting the old
  maximum version after offering only 3.1; the assertion now checks the actual
  negotiated 3.1 overlap. Its focused test passed.
- The first full-floor attempt also exposed an order-sensitive managed-app
  qualification assertion; the test now sorts observed instance IDs before
  comparing them. The persistent-autostart restart case and two-instance
  isolation case both passed focused reruns with the required sibling binary
  build completed first. A subsequent complete serial workspace run passed.

The final full-floor log was `target/interop/plan368-final-floor.log` (local,
not committed evidence). The final run reached the last `cargo deny` command
and completed successfully.

## Compatibility, security, migration, and documentation

- Production SAM version negotiation moved from maximum 3.1 to 3.3 only after
  the Java and local profile matrix passed. Clients restricted to 3.1 continue
  to negotiate 3.1. Java's `MIN=1.0 MAX=3.3` range negotiates 3.3. MASTER is
  retained as a compatibility alias; PRIMARY is the normative model.
- SAM remains loopback-only, disabled by default, experimental, and
  non-advertised as a public or remote-router capability. No live remote I2P
  tunnel or router-to-router compatibility is claimed.
- Ordinary SAM UDP binds only to loopback. Private managed-app datagrams use
  the bounded inherited manager protocol and have no listener, host UDP
  endpoint, or direct socket authority.
- Unsupported 3.3 send controls are refused before enqueue; they are not
  silently treated as honored. Request/payload sizes and per-primary children
  remain bounded, and raw payloads are not written into sanitized evidence.
- README, API/daemon/overview/tooling architecture pages, the protocol support
  table, conformance inventory, `specs/support.toml`, the SAM dossier, and the
  SAM integration README now describe the qualified local profile and its
  limitations. Historical SAM 3.1 closure records remain unchanged.
- No dependency or license change was introduced for this plan.

## Findings and downstream unblock audit

Findings: **critical 0; high 0; medium 0; low 0**.

The SAM roadmap and registry name two external consumers: `dbowm91/i2pr-tc`
C003 and `dbowm91/i2pr-mail` M006/M012. The SAM 3.3/port-aware interface
prerequisite is now closed, so those consumers are unblocked with respect to
this dependency. Their repositories are not present in this checkout, so no
external plan status was changed or inferred. No registered SAM implementation
plan in this repository lists SAM/368 as a remaining hard dependency. Plan 385
and the managed-app sandbox line do not depend on SAM/368; they were already
ready and remain so. No additional local successor became eligible.

Roadmap disposition: **closed**. SAM 3.1 authority remains Plan 151. Plan 368
closes only its experimental SAM 3.3 local profile; it does not create a public
advertisement or remote interoperability claim.
