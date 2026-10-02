# Plan 298 closure — Streaming active-fingerprint differential harness

Status: stopped-three-family-hostile-destination-capture-runners-unavailable

Implementation commits: none yet. This closure records completed local harness infrastructure, not a Java/i2pd/i2pr differential result.

## Requirement disposition

| Requirement | Result |
| --- | --- |
| Bounded sanitized trace schema | Implemented in `i2pr-testkit::streaming_fingerprint`: fixed scenario/direction/terminal vocabularies, normalized sequence/ack deltas, 10 ms buckets, payload length only, 4096-event ceiling, 600,000 ms deadline, canonical TSV parser/serializer. |
| Deterministic scenario contract | Added 11 named scenarios, five repetitions for timing-sensitive cases, fixed timing tolerance, and exact Java 2.13.0/i2pd 2.61.0 source pins. |
| Classification fixture | Tests cover reference-common, i2pr-versus-reference matches, reference disagreement, i2pr neither, and harness limitation classification. |
| Evidence leakage guard | `scripts/check-streaming-fingerprint-evidence.sh` checks reference pins, fixed scenario inventory, bounded trace ceiling, forbidden payload/identity field names, and a seeded negative case. |
| Exact family runners and hostile Destination | Not implemented or executed. Existing M6 runners qualify normal interoperability; they do not drive this hostile packet-stimulus matrix. The existing Java controlled launcher is SAM-only. No three-family hostile-Destination topology is available. |
| Captures and differential report | Not produced. Local seeded classification fixtures are tests of the classifier, not captured router evidence. |
| Production changes | None. No Streaming defaults were changed. |

## Verification

- `cargo test --locked -p i2pr-testkit --all-targets -- --test-threads=1`: passed, 29 tests across library and integration targets.
- `cargo test --locked -p i2pr-client --all-targets -- --test-threads=1`: passed, including 80 unit tests and all trajectory suites.
- `bash -n scripts/check-streaming-fingerprint-evidence.sh`: passed.
- `bash scripts/check-streaming-fingerprint-evidence.sh`: passed, including seeded forbidden-marker rejection.
- `git diff --check`: passed.
- The Plan's three `run-streaming-fingerprint-{i2pr,i2pd,java}.sh` commands and preflight are absent; no router captures or external differential execution occurred.

## Limitations and disposition

No production Streaming fingerprint, family equivalence, or anonymity claim is established. Timing and active-probe dimensions remain unmeasured. The local artifacts are infrastructure only and cannot authorize Plan 299 tuning.

## Unblock audit

Plan 299 remains blocked because its hard dependency requires executed three-family differential evidence. Plan 300 remains independently ready because its Destination, tunnel, NetDB, and service-tunnel authorities are closed and it does not consume Plan 298. Plan 301 remains blocked on successful Plans 297, 299, and 300. Continue with Plan 300 under the user's instruction to pursue other eligible work.
