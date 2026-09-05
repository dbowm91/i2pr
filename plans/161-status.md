# Plan 161 status — Milestone 8 SSU2 independent IPv4 interop (IN PROGRESS)

Status: **`in-progress-direction-a-proven`**. Plan 161 is NOT closed:
direction B, the token/Retry matrix beyond the tokenless path, the
malformed/spoof rows, and the Java secondary lane remain open. Direction A
remains proven. The temporary routine-CI lane-selection corrective is closed by
Plan 162, so Plan 161 is the next executable plan.

Plan of record:
[`plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`](161-m8-ssu2-independent-ipv4-interop-and-final-closure.md).

Temporary corrective authority:
[`plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`](162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md).

```text
plan_161 = in-progress-direction-a-proven
plan_161_current_blocker = none (Plan 162 lane corrective passed)
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
next_executable_plan = 161
resume_after_plan162 = 161
milestone8_final_acceptance = not-yet-closed
```

Reference pins (unchanged):

```text
i2pd = 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5 (mandatory)
java = 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d (secondary, untouched)
```

## Direction A: proven (retained)

Against one ephemeral unprivileged i2pd 2.61.0 listener on
`127.0.0.1:43823` (`reservedrange=false`, loopback-only), the
`i2pr-runtime` driver
(`crates/i2pr-runtime/tests/ssu2_independent.rs`, env
`I2PD_ROUTER_INFO` / `I2PD_SSU2_ENDPOINT` / `I2PR_SSU2_BIND` /
`I2PR_SSU2_FLOODFILL` / `EVIDENCE_DIR`, all fail-closed) proves over
real loopback UDP:

- tokenless TokenRequest → Retry → SessionRequest → SessionCreated →
  SessionConfirmed establishment, mutually authenticated
  (`sessions_established: 1`, `used_cached_token: false`;
  i2pd logs `Session with 127.0.0.1:44001 (...) established`);
- one small (single-datagram) and one fragmented DatabaseStore
  i2pr → i2pd, both ingested (`RouterInfo added` on the i2pd side);
- one direct DeliveryStatus echo per store i2pd → i2pr over the same
  session (type 10, reply token echoed in the message body);
- graceful termination with the session/task baseline restored
  (`active_sessions: 0`, zero auth failures / cheap drops).

Historical direction-A command/result on the pre-Plan-162 tree
(2026-09-04):

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent
# test result: ok. 1 passed (3.45s)
```

Plan 162 changed only test-lane selection. The canonical external invocation
must explicitly select the ignored external test:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

That explicit command remains fail-closed when the required external
environment is absent and must be re-proven against the same exact-pinned i2pd
before Plan 162 closed. Hosted routine CI run `33941941145` passed Ubuntu,
macOS, MSRV, and dependency policy on implementation commit
`624e8cce177040674376163160cfbda47e6a60fe`.

Plan 162 gate validation on 2026-09-05 confirmed the three required lane
states: ordinary invocation reported `1 ignored` and exited 0; explicit
`--ignored --exact` invocation without external variables failed with
`missing required env I2PD_ROUTER_INFO`; and the explicit invocation with
the cached, verified i2pd 2.61.0 reference passed direction A in 3.47 s
with 24 sanitized evidence rows. The latter run used `I2PR_SSU2_FLOODFILL=1`
and retained the established small/fragmented DatabaseStore, DeliveryStatus,
and clean-resource-baseline assertions.

Evidence artifact (`EVIDENCE_DIR/driver-evidence.tsv`): sent/reply
lengths plus SHA-256 digests for both directions, peer RI length,
close/resource counters. No secret material is recorded.

## Protocol corrective (this pass)

Live comparison against the pinned i2pd implementation exposed three
transcript divergences that loopback tests could not see (both sides
were wrong identically). All are fixed in
`crates/i2pr-transport-ssu2`, with fixtures regenerated:

1. SessionCreated sealed/accepted mixed the request ciphertext a
   second time; i2pd mixes it once at the SessionRequest stage.
   Symptom: our initiator terminated with `AuthenticationFailed`
   against real SessionCreated bytes.
2. The first-fragment SessionConfirmed short header was never mixed
   before the static-key frame; i2pd mixes the 16 cleartext header
   bytes first. Symptom: i2pd rejected our SessionConfirmed part 1.
3. The static-key frame used the retained `es` cipher; i2pd (and Noise
   XK ordering) uses the post-`ee` cipher at `n = 1`. Same symptom as
   (2); the retained cipher is now taken after the `ee` MixKey and
   renamed `static_cipher`.

Regenerated vectors: `session-created-full.hex`,
`session-confirmed-frag.hex` (+ `manifest.tsv` hashes). The older
header-protection counter fix (ChaCha20 stream offset 64, i.e. block
counter 1) and its vectors predate this pass and are retained.

Plan 162 must not reopen or modify these protocol corrections unless its
external re-run demonstrates a new concrete protocol defect. Its expected code
change is only integration-test execution metadata/gating.

## Routine CI lane corrective closed by Plan 162

Routine CI run `33915994884` on exact head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both quality jobs because
ordinary workspace execution automatically ran the external integration test
without its required i2pd environment. Plan 162 added the descriptive
`#[ignore]` gate, retained all-target compilation, and restored routine CI;
the dedicated external invocation remains explicit and fail-closed:

```text
Quality (ubuntu-latest) = failure
Quality (macos-latest)  = failure
Dependency policy       = success
MSRV (Ubuntu)           = success

ssu2_independent_ipv4_interop ... FAILED
missing required env I2PD_ROUTER_INFO
```

All observed failure evidence points to test-lane selection. The external test
is correctly fail-closed when actually run; it simply must not be run by the
ordinary no-peer workspace lane. Plan 162 closed this correction. Do not weaken
`env_value()` or turn missing environment into an early success.

## Harness hazards (recorded for the remaining rows)

- Purge stale `netDb/r*/*.dat` (+ peer profiles) and restart the
  ephemeral i2pd before each run: i2pd redials previously learned
  RIs, and a pending outbound dial to our bind port swallows our
  inbound handshake by endpoint routing (`Unexpected message type
  ... instead 9`, dial timeout).
- A static-key mismatch on a published address makes i2pd ban the
  whole loopback address for ~30–39 minutes (`AddBan`), breaking all
  later runs until restart. Never present a mismatched RI on the
  live port.
- The `#[tokio::test]` runtime is single-threaded: any
  `std::thread::sleep` in the driver wedges transmit/receive for the
  whole poll. The NetDB poll is removed; all waits are cooperative.
- i2pd converts short-header seconds to milliseconds and enforces a
  (-60 s, +180 s) accept window: transport expirations must be near
  term (`+60 s` used), never tunnel-style horizons.
- i2pd `RouterInfo` ingest requires `router.version` digits clearing
  its minimum-allowed floor and a matching `netId` property; a
  missing `netId` marks the RI unreachable silently (no persist).
- i2pd randomizes the short-header message ID on send: match
  DeliveryStatus replies by the body token (offset 9), not the
  header message ID.
- i2pd persists only RIs it keeps connected and purges the rest at
  manage ticks: NetDB `.dat` polling is not a reliable delivery
  signal. DeliveryStatus echoes are the direction-A proof instead.

## Open rows (not claimed)

With the Plan 162 CI-lane blocker cleared, Plan 161 still owns:

- Direction B (i2pd initiator → i2pr responder).
- Token/Retry matrix beyond the tokenless path.
- Malformed/spoof/resource rows against the live peer.
- Java I2P secondary lane or exact documented nonblocking blocker.
- Final fail-closed SSU2 evidence ledger/checker and manual external workflow.
- `specs/support.toml` / `specs/CONFORMANCE.md` final closure only after all
  mandatory Plan 161 criteria pass.

No new support or advertisement claim is made by the direction-A pass or Plan
162.

## Quality state before Plan 162

Local validation with the external environment supplied was green:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1  # 60 suites ok,
    # including the live ssu2_independent driver (env-supplied ephemeral i2pd)
cargo test --locked --workspace --doc                              # ok
cargo test --locked -p i2pr-transport-ssu2 --all-targets        # 163 passed
cargo test --locked -p i2pr-transport --all-targets            # 44 passed
cargo test --locked -p i2pr-crypto --all-targets               # 52 passed
cargo test --locked -p i2pr-runtime --lib                      # 70 passed
cargo test --locked -p i2pr-runtime --test ssu2_local          # 9 passed
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay     # 7 passed
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-ssu2-vectors.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'  # 153 passed
cargo deny check advisories bans sources
```

Hosted routine CI is green on Plan 162 implementation commit
`624e8cce177040674376163160cfbda47e6a60fe` via run `33941941145`; Plan 161
now resumes for its remaining acceptance rows.
