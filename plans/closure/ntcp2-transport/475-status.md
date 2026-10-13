# Plan 475 status — blocked at Java message correlation and product effect

**Status token:** `blocked-java-send-and-authenticated-session-lack-two-way-product-correlation`  
**Disposition:** source-pinned stock send/TCP and inbound responder phase evidence retained; Java-owned message-to-product path and two-way I2NP acceptance remain open.  
**Implementation commit:** `0cd526f` (`diag(plan475): trace Java send and inbound handshake stages`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Freeze exact Java source and selector preconditions | Exact `2.13.0@9134f808337b401e8e53c73734c81fab04280c9d`; selector preflight reports signed candidate admitted, selected, not banned/unreachable, actual Store size 477 bytes, and NTCP2 bid available | Passed |
| Observe normal Java-owned send and fresh TCP attempt | Pinned normal facade flood run: `java_stock_transport_send=1`, `java_outbound_tcp_connected=1`, two stock transport send attempts, two NTCP send attempts; no private transport call was introduced | Lower-tier event observed; the concrete `OutNetMessage` target-to-send correlation remains unproven |
| Localize Java inbound EOF independently | i2pr emitted 25 SessionRequests and observed EOF on all 25 Java SessionCreated reads; separate i2pr responder trace observed three completed SessionCreated reads and one `noise_authenticated` / SessionConfirmed write completion | Both event families observed; not correlated to a single product transaction |
| Prove authenticated Java↔i2pr fresh initiation and real two-way product response | No daemon lookup submission, promoted outbound link, I2NP product effect, or correlated reply was observed; first missing stage remains `daemon_lookup_submitted` | Not established |
| Correct a proven stock fixture or i2pr defect and rerun product acceptance | No source-level protocol defect was established by the available trace | Not established |

The source-owned phase observer records only bounded stage names, I/O result, byte counts, and elapsed time. The latest exact pinned lane exited nonzero at `daemon_lookup_submitted`; this is an evidence boundary, not a successful Java interoperability result. The successful inbound Noise/SessionConfirmed observation does not erase the separate i2pr-initiated SessionCreated EOFs and does not prove I2NP consumption.

## Verification

| Command | Outcome |
| --- | --- |
| `cargo check --locked -p i2pr-runtime -p i2pr-daemon --all-targets` | Passed |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-daemon` | Passed before focused daemon qualification |
| `cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed (35 tests) |
| `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed (135 tests, 1 ignored) |
| `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed (9 tests) |
| `python3 -m py_compile tests/integration/ntcp2/plan470_java_ntcp2.py` | Passed |
| `python3 tests/integration/ntcp2/plan470_java_ntcp2.py` | Executed exact pinned Java 2.13.0 lane; exited 1 at first missing stage `daemon_lookup_submitted` |
| `bash scripts/check-ntcp2-vectors.sh` | Passed |
| `bash scripts/check-ntcp2-interoperability.sh` | Passed (historical harness static guard only) |

The supplied AGENTS.md routine floor also passed: format/check/build, 4,853 workspace tests (39 ignored), Clippy, rustdoc/doc-tests, repository boundary/evidence scripts, planning suite (51 tests), and `cargo deny check advisories bans sources`.

The operator-provided repository floor remains required for handoff. Reference logs remained in the private temporary profile; emitted evidence contains counts and bounded stage enums only. No Java or i2pd source/jar was patched.

## Invariant, compatibility, and security review

No NTCP2 wire behavior, RouterInfo acceptance rule, authentication condition, or production capability advertisement changed. Responder observation does not promote a connection before the ordinary handshake owner returns success. Diagnostics contain no peer identity, endpoint, handshake bytes, or raw log text. The reference pin and ordinary Java facade path remain unchanged.

## Limitations and next executable step

The next in-plan step is to correlate the selected candidate and actual normal `OutNetMessage` through the stock GetBidsJob/send queue to the observed TCP owner, then correlate the independent inbound SessionRequest's Java-side accept/read outcome. Correct only a source-attributed defect; rerun both initiating roles and prove normal product I2NP receipt/reply before closing. No new plan is registered because this plan's source-owned investigation remains active and its required causal correction has not been exhausted.

**Unresolved findings:** high — normal Java two-way NTCP2 router I2NP remains unqualified; medium — stock send attempt and i2pr responder observation are not tied to a correlated product transaction.

**Roadmap disposition:** blocked; no Java-family or full operational capability is unblocked.
