# Plan 312 closure — pinned i2pd directional handshake fingerprint baseline

Status: **passed-pinned-i2pd-directional-handshake-fingerprint-baseline**

Plan: `plans/implementation/anonymity/312-i2pd-streaming-directional-fingerprint-baseline.md`  
Authority: ADR 0030; Plan 315 passed.  
Reference: i2pd 2.61.0, revision `635b013a612ff47278ef02acf8580a28e10e26c5`.

## Result

The exact-pinned Plan 193 Streaming lane completed against an unmodified i2pd reference. The i2pr-controlled integration driver captured one initial handshake packet from each endpoint role and wrote a sanitized comparison matrix. Client and server roles agree on handshake flags and FROM inclusion. The maximum packet payload differs in both roles: i2pr advertises 1730 bytes and i2pd advertises 1812 bytes. Initial payload length is zero for all four captures.

The registered scenario is `clean_handshake_default_port`. Registered dimensions are exactly `flags`, `from_included`, `max_payload`, and `payload_length`. The capture retains neither raw packet bytes nor Destination or stream identifiers. The generated, ignored evidence directory is `target/interop/anonymity/plan312-streaming`; `evidence.json` records the reference pin, i2pr source commit, scenario, redaction properties, and SHA-256 hashes for the manifest, traces, and matrix.

## Requirement disposition

| Requirement | Result | Evidence |
|---|---|---|
| Exact i2pd pin and no reference modification | passed | Plan 193 preflight verified version 2.61.0 and exact revision; full external lane passed |
| i2pr client vs i2pd client observation | passed | `fingerprint-i2pr-client.tsv`, `fingerprint-i2pd-client.tsv` |
| i2pr server vs i2pd server observation | passed | `fingerprint-i2pr-server.tsv`, `fingerprint-i2pd-server.tsv` |
| Frozen handshake dimension comparison | passed | `fingerprint-matrix.tsv`: 8 role/dimension cells; flags/FROM/payload length match, max payload differs in both roles |
| Sanitized metadata only | passed | manifest says `raw_packet_bytes=0`, `destination_or_stream_ids=0`; sanitized evidence manifest says both retained flags are false |
| Fail-closed matrix checker | passed | valid matrix accepted; missing role, pin drift, and extra column rejected by unit tests |
| Production Streaming tuning | none | all changes are test/evidence infrastructure and planning records |

Other source-reviewed candidates—window/choke, ACK/NACK, RTO/retransmission, loss/reorder, close/reset, and terminal state—are not observed by this clean-handshake capture and are classified `NotReliablyObservable` for this run. They cannot justify a tuning decision. Timing is not registered; no timing conclusion is made.

## Implementation and verification

The ignored integration target reuses the canonical testkit fingerprint schema with a local source include. This avoids introducing a daemon-to-testkit Cargo dependency, which violates the repository runtime-boundary check. The capture decodes packets only to extract registered handshake metadata. The Plan 312 runner wraps the existing exact-pinned Plan 193 external lane, clears stale trace artifacts before each attempt, checks the four-role matrix, and emits a sanitized hash manifest.

Commands and results:

- `rtk cargo fmt --all` — passed.
- `rtk cargo check --locked -p i2pr-daemon --test streaming_tunnel_external` — passed.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- `rtk python3 -m unittest discover -s tests/integration/anonymity -p 'test_streaming_fingerprint.py'` — 4 tests passed.
- `rtk tests/integration/anonymity/run-plan312-streaming.sh` — passed; exact-pinned external lane and Plan 312 evidence checker both passed.
- `rtk git diff --check` — passed after documentation updates.

The broad repository routine floor was not rerun for this test-only evidence change. The merged base `2b0ca6b231987f8c2e6665bc9d45a9cd25ddc804` had already passed the full serial workspace suite, all-feature Clippy, rustdoc, doctests, and the service-tunnel evidence checks during its main integration. No production crate behavior changed in Plan 312.

## Unblock audit and handoff

Plan 313 is now **ready**. Its tuning scope is limited to measured handshake differences, beginning with the maximum-payload difference. Unobserved dimensions require a new source-observability review and registered scenario before they can enter scope.

Plan 311 remains **blocked** on the normal-daemon Plan 315 group lifecycle and pre-shutdown ownership gap. Plan 316 is registered as the ready corrective prerequisite. Plan 308 remains independently blocked on its ordinary-HTTP topology and three-family captures. Plan 312 does not change either blocker. No other eligible plan was newly unblocked by this evidence.

No conformance or production anonymity claim is made. No dependency was added.
