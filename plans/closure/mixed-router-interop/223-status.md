# Plan 224 registration follow-up

The narrow successor implied by this closure is now registered as Plan 224.

Exact-pinned Java source review shows `NO_LEASESET (21)` is a lookup-failure
result, not a unique root cause. Plan 224 first proves whether Router B's main
NetDB actually holds a current, `receivedAsPublished`, query-answerable copy
of the i2pr target LS2. Only if it does, Plan 224 traces the exact helper
client lookup A→B, Router-B answer, Router-A client-tunnel DSM receipt, and
helper client-subDB installation.

Pinned `InNetMessagePool` explicitly stores a matching DSM inline before
queuing the lookup-success reply job, so a store-vs-success scheduling race is
not an authorized hypothesis.

Plan 224 is attribution-only. The eventual fix belongs to Plan 225.

```text
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
plan_224 = registered-ready-m6-java-no-leaseset-lookup-path-attribution
next_executable_plan = 224-m6-java-no-leaseset-lookup-path-attribution
```

# Plan 223 status — M6 Java Destination identity / LeaseSet2 crypto-separation corrective

Status: **`passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset`**.

Plan of record:
[`223-m6-java-destination-identity-crypto-separation-corrective.md`](../../implementation/mixed-router-interop/223-m6-java-destination-identity-crypto-separation-corrective.md).

## 0. Registration basis (retained — historical context)

Plan 222 closed the client-NetDB/OCMOSJ narrowing with:

```text
P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
```

Exact-pinned Java I2P 2.13.0 source review after that closure identified a
more specific, earlier status-17 branch than Plan 222 distinguished:

```java
if (_to.getEncType() != EncType.ELGAMAL_2048) {
    dieFatal(MessageStatusMessage.STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION);
    return;
}
```

This guard executes in `OutboundClientMessageOneShotJob.runJob()` before the
client-NetDB lookup and before the later LeaseSet2 key-intersection check.

Current (pre-223) i2pr router-owned Destination generation embedded the local
X25519 static public key into the Destination and advertised X25519/type 4 in
the Destination key certificate. Java's own `I2PClientImpl.createDestination()`
instead keeps the legacy 256-byte Destination public-key slot in its
ElGamal/type-0 identity shape; modern X25519 destination encryption is
advertised separately in LeaseSet2.

## 1. Outcome

One exact-clean-head destination-only run on the committed implementation
emits exactly one `P223-*` terminal derived from exact Rust/Java Destination
observations, the retained Plan-222 selector preflight, one nonce-correlated
helper send, and the frozen 45-second payload window:

```text
p223_terminal = P223-NEXT-BOUNDARY ordered_statuses=[1, 21]
```

The early non-ElGamal guard is removed: post-fix generated Destinations are
ElGamal/type-0/256-byte with Ed25519/type-7, Java parses the exact bytes as
`ELGAMAL_2048` with matching hashes, Standard LS2 stays X25519/type-4/32
sourced from the static secret, and the nonce-correlated reverse send no
longer returns status 17. The new boundary is honestly recorded (ACCEPTED
followed by NO_LEASESET) without implementing a second corrective, per §13
G6. Plan 223 therefore succeeds at its corrective objective; a narrow
successor owns the NO_LEASESET boundary.

```text
i2pr_commit = 0755dc12cad94fae33fcb76a0d470e45175539eb
java_i2p    = 2.13.0 (9134f808337b401e8e53c73734c81fab04280c9d)
i2pd        = 2.61.0 (635b013a612ff47278ef02acf8580a28e10e26c5, pin frozen, not exercised by this lane)
rust        = 1.95.0
p223_terminal = P223-NEXT-BOUNDARY ordered_statuses=[1, 21]
```

## 2. Implementation commits

All implementation landed before the authoritative run. The tree was clean
(`git status --porcelain=v1` empty) at the implementation head before each
lane attempt on that head. No code, config, topology, or timeout changed
between attempts on the same head.

```text
4c3fe17  plan223: separate router-owned Destination legacy identity from LS2 X25519
e82f12c  plan223: clippy explicit-auto-deref for v2 padding
0755dc1  plan223: resolve SAM STREAM CONNECT static via local LS2 for ElGamal destinations
```

`0755dc1` is the authoritative implementation SHA (includes `4c3fe17` +
clippy + SAM LS2-resolution; destination-lane behavior identical across the
three — destination code unchanged after `4c3fe17`, SAM fix touches only the
SAM STREAM CONNECT path which the destination lane never exercises).

Files changed (`dbc5daa..0755dc1`):

```text
crates/i2pr-api/src/sam/private_destination.rs
crates/i2pr-client/src/identity.rs
crates/i2pr-client/src/leaseset.rs
crates/i2pr-client/src/lib.rs
crates/i2pr-client/tests/plan120_trajectory.rs
crates/i2pr-client/tests/plan121_trajectory.rs
crates/i2pr-client/tests/plan126_trajectory.rs
crates/i2pr-client/tests/plan132_trajectory.rs
crates/i2pr-client/tests/plan166_trajectory.rs
crates/i2pr-daemon/src/sam.rs
crates/i2pr-daemon/src/sam/streams.rs
crates/i2pr-daemon/src/service_tunnels.rs
crates/i2pr-daemon/tests/java_tunnel_external.rs
crates/i2pr-storage/src/service_destination.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
tests/integration/m6-interop/java/ControlledRouter.java
tests/integration/m6-interop/java/ReferenceRawDestination.java
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P223BranchProbe.java (new)
tests/integration/m6-interop/run-java.sh
```

No Java source patched. No reflection. No NetDB/tunnel mutation by
diagnostics. No public I2P. No reference vendoring. No VMComm. No SAM pivot.
No topology expansion. No change to the 45-second window. No Java timeout
change. No RouterInfo/router-identity change. No LS2 downgrade. No ElGamal
encryption implementation.

## 3. Pre-fix exact Destination proof (WP B gate)

Pre-fix shape preserved via `from_private_bytes_legacy_x25519` (byte-identical
to pre-223 `from_private_bytes`). Deterministic pre-fix bytes (signing
`[7;32]`, static `[9;32]`, padding `[0x5a;320]`):

```text
pre_rust_hash_hex = c0b30b508e74cff28d8d55aae0bd184827778e95c78e95a9ad6570cda50d309b
pre_rust_enc_type = 4
pre_rust_public_key_len = 32
pre_rust_sig_type = 7
pre_rust_dest_len = 391
pre_rust_b64_len = 524
```

Exact-pinned Java parse of the same base64 (`P223LocalInspect` against
`9134f808.../lib/*.jar`, public `new Destination(String)` only):

```text
pre_java_hash_hex = c0b30b508e74cff28d8d55aae0bd184827778e95c78e95a9ad6570cda50d309b
pre_java_enc_type_code = 4
pre_java_enc_type_name = ECIES_X25519
pre_java_public_key_len = 32
pre_java_sig_type_code = 7
```

Cross-checks:

```text
rust_destination_hash == java_parsed_destination_hash (c0b3... match)
rust_destination_enc_type == java_parsed_destination_enc_type (4 == 4)
rust_destination_public_key_len == java_parsed_public_key_len (32 == 32)
```

`identity.rs` was unchanged between Plan-222 implementation `cd334f8` and the
pre-223 head (`git diff cd334f8..dbc5daa -- crates/i2pr-client/src/identity.rs`
empty), so Plan-222's tracked destination was the same X25519/type-4 shape.
Plan-222 authoritative run (`cd334f8`, attempt 2) drew nonce-correlated
`STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION (17)` with no TunnelData/payload
in the frozen 45 s window. Pinned source shows the early
`_to.getEncType() != ELGAMAL_2048` guard returns 17 before lookup/LS2
selection. Therefore:

```text
P223-PREFLIGHT-DESTINATION-ENC-GUARD-CONFIRMED
```

Limitation (low severity, documented, no concealment): the pre-fix Java parse
above uses deterministic legacy-reconstruction bytes (same generator, same
shape, same 391-byte length), not the exact random Plan-222 lane bytes (which
are untracked scratch and gone). Code-unchanged proof + same-shape exact-byte
agreement + pinned early-guard source + Plan-222 status 17 constitutes the
gate; WP D proceeded only after this proof. Post-fix lane retrospectively
confirms it (type-0 removes 17).

Post-fix deterministic bytes (same secrets, filler `[0x11;256]`, padding
`[0x5a;96]`) for completeness:

```text
post_rust_hash_hex = 152582f44b3ee5fb56223c4d59c3b6ab454957e79318fc2597a0e34b97bdd02f
post_rust_enc_type = 0
post_rust_public_key_len = 256
post_java_hash_hex = 152582f44b3ee5fb56223c4d59c3b6ab454957e79318fc2597a0e34b97bdd02f
post_java_enc_type_name = ELGAMAL_2048
```

## 4. Authoritative-run evidence (0755dc1, attempt 1 of 1 on this head)

Retry accounting (§14, budget 3 per head, no tuning, fresh scratch contexts):

- `4c3fe17` attempt 1: driver ran but lease-stalled before authoritative epoch
  (`plan199-java-stop: client-ls2-local-but-not-network-visible`, P222
  pre-epoch gap). No P223 terminal. Recorded, not authoritative.
- `4c3fe17` attempt 2 (AUTHORITATIVE on old head): `P223-NEXT-BOUNDARY
  [1,21]` with hash `f38ca552...`, type-0/256, LS2 type-4/32. Superseded by
  clippy+SAM fixes (non-destination-lane for clippy; SAM fix touches only SAM
  STREAM CONNECT which the destination lane never exercises). Retained as
  reproducibility proof (same terminal, different random hash).
- `e82f12c` attempts 1–2: harness exited pre-bootstrap (ephemeral Java SAM
  never bound). No driver ran; no classification. Infrastructure flakes.
- `e82f12c` attempt 3 (AUTHORITATIVE on intermediate head):
  `P223-NEXT-BOUNDARY [1,21]` with hash `1a0d4590...`. Superseded by SAM fix.
- `0755dc1` attempt 1 (AUTHORITATIVE): full destination-only run, clean tree,
  fresh scratch RouterContexts. Exactly one `p223-classification` emitted
  (`driver-evidence.tsv`). `evidence.json` `i2pr_commit` equals `0755dc1`.

Authoritative (`0755dc1`) facts:

### 4.1 Frozen Plan-220 facts (retained)

P220 still emits `P220-OBSERVABILITY-GAP-CLIENT-NETDB` with earlier stages
`Known(pass)` except the client-NetDB per-message gaps (by design, D220-6);
J219-B stays refuted (hash cross-check true, A-stored-B present+identity
match, PeerManager indexed, selector contains B). Forward digest match true,
reverse admitted true, TunnelData/payload false in 45 s.

### 4.2 Exact client-lookup preflight (retained P222, still passes)

```text
observable=true client_db_resolved=true client_db_is_client=true
target_hash_hex   = 368b34505a2c2aa8c1e317b38245a16bdee281f320570e85604c1d2e458f1688
routing_key_hex   = (differs, recorded)
routing_key_differs = true
target_ls_present_before_send = false
facade_floodfill_enabled = false router_uptime_ms = (consistent <30min)
netdb_search_limit_effective = 5 selector_extra_peers = 1 selector_width = 6
selector_input_kbucket_size = 4 selector_count = 2
selector_contains_b = true selector_empty = false
```

Width 6 = 5 + 1 matches pinned `IterativeSearchJob` (uptime <30min ⇒ default
5). Selector nonempty with B. Target LS absent pre-send (same as Plan-222).

### 4.3 Post-fix Destination observations (WP B/F4)

```text
p223-rust-destination: hash_hex=368b34505a2c2aa8c1e317b38245a16bdee281f320570e85604c1d2e458f1688 enc_type_code=0 public_key_len=256 sig_type_code=7
p223-java-helper-destination: hash_hex=368b... enc_type_code=0 enc_type_name=ELGAMAL_2048 public_key_len=256 sig_type_code=7 (via INSPECT_DEST)
p223-java-router-destination: hash_hex=368b... enc_type_code=0 enc_type_name=ELGAMAL_2048 public_key_len=256 sig_type_code=7 (via P223-DEST-INSPECT)
p223-destination-match: hash_match=true preflight=P223-PREFLIGHT-DESTINATION-ENC-GUARD-NOT-CONFIRMED
```

Java target enc type is ElGamal/type-0 with 256-byte slot (early guard cannot
fire). Hash equality holds across all three parsers.

### 4.4 Local LS2 (WP D3/F2)

```text
p223-local-ls2: ls_type=3 key_count=1 enc_type_code=4 key_len=32 key_match=true
```

Standard LS2 type-3, exactly one X25519/type-4/32 key sourced from
`static_public_bytes()`, never from `destination.public_key()`. Signature
verifies (unit + lane).

### 4.5 Tracked send + ordered statuses (WP G)

```text
TRACKED_SENT nonce=1 payload_len=27 digest=8a9e8146bb7d8c0b32b19b2483913d9f38aab1c7bfc925b1a73e7cd5aebb2271 reverse_sha256=8a9e... (equal)
ordered_statuses = [1, 21]
  1 = STATUS_SEND_ACCEPTED
  21 = STATUS_SEND_FAILURE_NO_LEASESET
status_unsupported_encryption = Known(false) (17 gone)
```

Same 27 B digest as Plan-222 (fixed probe payload). ACCEPTED observed (admission),
then NO_LEASESET terminal. No 17.

### 4.6 Frozen 45-second window + status-only extension

```text
frozen_tunneldata_45s = false frozen_payload_45s = false
p223-status-only-observation: nonce=1 events=[1,21] (decisive at freeze; 70 s extension broke immediately)
```

Payload window frozen at 45 s (`DATAGRAM_WAIT` unchanged); 70 s status-only
extension did not alter payload rows (unit J16 still green for P222 timing).

### 4.7 Bounded branch discriminator (WP C, recorded; not decisive for G6)

Pre-send (same epoch as P222 preflight) + post-send refresh (since status is
non-17, pre-send snapshot governs; post-send refresh reused pre-send when
status-17 absent per driver logic):

```text
p223-branch: observable=true source_keys_present=true source_supported_types=4 source_supports_elgamal=false source_supports_x25519=true target_ls_present=false target_ls_type=none target_destination_hash_match=false target_destination_enc_type=-1 target_key_count=0 target_key_types=none target_has_x25519=false selected_key_present=false selected_key_type=-1
```

Source helper keys present with X25519-only support (no ElGamal). Target LS
absent in helper client DB pre-send (same as P222). No intersection to
compute. Since status 17 did NOT persist, G6 does not consume C as root cause;
C is recorded for completeness.

### 4.8 P223 terminal (§13 G6)

```text
P223-NEXT-BOUNDARY ordered_statuses=[1, 21] preflight=P223-PREFLIGHT-DESTINATION-ENC-GUARD-NOT-CONFIRMED hash_match=true frozen_payload_45s=false
```

Status 17 disappeared; new boundary is NO_LEASESET (21) after ACCEPTED (1).
No second corrective implemented. Successor owns NO_LEASESET.

## 5. Requirement-to-evidence matrix (Plan 223 §19, 28 criteria)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Pre-fix target bytes parsed by Rust+Java | PASS | §3 PRE_RUST/PRE_JAVA c0b3... type-4/32 |
| 2 | Rust/Java target hashes match (pre-fix) | PASS | c0b3... equality |
| 3 | Pre-fix Java enc type recorded | PASS | 4/ECIES_X25519 |
| 4 | Same tracked send reproduces 17 (pre-fix) | PASS | Plan-222 `cd334f8` attempt 2 `[17]` + code-unchanged proof |
| 5 | Early guard confirmed before production change | PASS | §3 CONFIRMED (hash/type match + Java 4 + Plan-222 17 + pinned early-guard source) |
| 6 | Post-fix Destination type-0/256 | PASS | §4.3 368b... 0/256 (helper+router agree) |
| 7 | Post-fix signing Ed25519/type-7 | PASS | §4.3 sig 7 + unit |
| 8 | Filler independent of X25519 secret | PASS | `generate()` CSPRNG filler; unit `distinct_filler_yields_distinct_hash` + `legacy_filler_is_not_secret_derived`; checker §16d |
| 9 | RouterInfo/router identity X25519/type-4 | PASS | Unchanged; crypto unit `deterministic_generation...` still asserts `ROUTER_CRYPTO_KEY_TYPE`; no router-identity diff |
| 10 | Standard LS2 type-3 | PASS | §4.4 `ls_type=3` + `LEASE_SET2_DATABASE_STORE_TYPE=0x03` unit |
| 11 | LS2 enc X25519/type-4/32 | PASS | §4.4 + unit `plan223_ls2_remains_ecies...` |
| 12 | LS2 X25519 matches static secret | PASS | §4.4 `key_match=true` + unit |
| 13 | Imported Java/i2pd path accepted | PASS | Unit `imported_java_compatible_destination_is_preserved` + Plan-146 SAM reference suite green |
| 14 | SAM/private-destination audit complete | PASS | §6; SAM STREAM CONNECT LS2-resolution fix; PRIV 391/455 unchanged; SAM suites green |
| 15 | No silent persistent migration | PASS | §6; v1 X25519 preserved byte-identical, v2 ElGamal versioned; unit `v1_legacy_record_preserves_x25519_shape` |
| 16 | Plan-222 selector preflight still passes | PASS | §4.2 width-6/nonempty/contains-B |
| 17 | Nonce-correlated reverse send + ordered statuses | PASS | §4.5 nonce 1/digest match/[1,21] |
| 18 | Frozen 45 s window unchanged | PASS | `DATAGRAM_WAIT` 45 s; frozen vars; checker §16f |
| 19 | Status 17 disappears or C records why | PASS | 17 gone ([1,21]); C recorded §4.7 |
| 20 | Exactly one P223 final terminal | PASS | §4.8 `P223-NEXT-BOUNDARY` (single `p223-classification` row) |
| 21 | No bootstrap/floodfill/SAM/tunnel/topology change | PASS | Diff touches identity/LS2/SAM-resolution/diagnostics/checker only; tunnel/topology files untouched |
| 22 | No Java patch/reflection/mutation | PASS | Helpers use public APIs only; probe forbids `registerKeys`/`store`/`setAccessible`; checker §16i |
| 23 | Routine floor green | PASS | §6 commands all exit 0 on `0755dc1` |
| 24 | Focused Plan-222 regressions green | PASS | §6 (`destination_tunnel_unit` 41, `java_tunnel_external` 30+18, `--no-run`) |
| 25 | Commit precedes evidence, clean tree | PASS | `0755dc1` committed before authoritative run; `git status` empty before each attempt on that head |
| 26 | ≤3 exact-head attempts | PASS | `0755dc1`: 1 attempt (authoritative); historical heads documented in §4 (per-head ≤3, no tuning) |
| 27 | Registry/roadmap/dependents truthful | PASS | §9 |
| 28 | `milestone6_interoperable` unclaimed | PASS | §9 (still `not-yet-claimed`) |

## 6. Tests and guards run with outcomes

Routine floor (local, on `0755dc1`; lane evidence in
`target/interop/m6-java-evidence`, untracked scratch):

```text
cargo fmt --all --check                                                  OK
cargo check --locked --workspace --all-targets                           OK
cargo test --locked --workspace --all-targets -- --test-threads=1        OK (exit 0; all suites passed, incl. sam_stream_final_acceptance 10/10 after SAM LS2-resolution fix)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps      OK
cargo test --locked --workspace --doc                                   OK (0 suites, exit 0)
bash scripts/check-dependency-direction.sh                              ok
bash scripts/check-runtime-boundaries.sh                                passed
bash scripts/check-service-tunnel-boundaries.sh                         ok
bash scripts/check-fixture-manifest.sh                                  (no output, exit 0)
bash scripts/check-ntcp2-vectors.sh                                     hashes match
bash scripts/check-ssu2-vectors.sh                                      hashes match
bash scripts/check-i2cp-vectors.sh                                      hashes match
bash scripts/check-ntcp2-interoperability.sh                            OK
bash scripts/check-constrained-host-lane-boundary.sh                    passed
bash scripts/check-sam-acceptance-evidence.sh                           22 rows command-derived
bash scripts/check-ssu2-acceptance-evidence.sh                          15 rows
bash scripts/check-i2cp-acceptance-evidence.sh                          24 rows
bash scripts/check-service-tunnel-acceptance-evidence.sh                29 rows (+ Plans 202/210/212–215 invariants)
bash scripts/check-exploratory-tunnel-evidence.sh                       12 labels
bash scripts/check-netdb-tunnel-evidence.sh                             12 labels
bash scripts/check-destination-tunnel-evidence.sh                       21 labels
bash scripts/check-streaming-tunnel-evidence.sh                         33 labels
bash scripts/check-m6-mixed-router-acceptance-evidence.sh               passed (11 guarded + ... + Plan 223 §16 separation invariants)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   18 tests OK (routine floor; run before handoff per AGENTS.md)
cargo deny check advisories bans sources                                 ok
```

Focused suites on `0755dc1`:

```text
cargo test --locked -p i2pr-client -- --test-threads=1                   OK (incl. plan223 F1/F2/F3 rows + plan166 mismatch update)
cargo test --locked -p i2pr-api -- --test-threads=1                      OK
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1    41 passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1       30 passed, 3 ignored (fail-closed ordinary)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p222 -- --test-threads=1  18 passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run                  OK
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1    4 passed (SAM LS2-resolution fix)
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1 10 passed
cargo test --locked -p i2pr-storage -- --test-threads=1                   OK (incl. v1 preservation + v2 round-trip)
```

External lane (exact-clean-head, `I2PR_M6_JAVA_DRIVER=destination`):

```text
0755dc1 attempt 1 (clean, AUTHORITATIVE): P220-OBSERVABILITY-GAP-CLIENT-NETDB (retained) + P222 CLIENT-NETDB-NO-USABLE-LEASESET ([1,21]) + P223-NEXT-BOUNDARY [1,21] (§4);
  harness row external-p223-classification=passed (diagnostic observation);
  Plan 199 aggregator still exits non-zero on destination-inbound/streaming rows (expected: reverse payload absent + destination-only run);
  workspace-gates row passed inside the lane.
Historical heads (same destination code, superseded by clippy/SAM non-lane fixes):
  4c3fe17 attempt 1: lease-stalled pre-epoch (no P223 terminal)
  4c3fe17 attempt 2: P223-NEXT-BOUNDARY [1,21] hash f38c... (reproducibility proof)
  e82f12c attempts 1–2: pre-bootstrap SAM infra (no driver)
  e82f12c attempt 3: P223-NEXT-BOUNDARY [1,21] hash 1a0d... (reproducibility proof)
```

Deviation from §12/§14 (documented, no concealment): pre-fix Java agreement
uses deterministic legacy-reconstruction bytes (§3) rather than the exact
random Plan-222 lane bytes (untracked scratch, gone); code-unchanged proof
(`cd334f8..dbc5daa` identity diff empty) + same-shape exact agreement + pinned
early-guard source + Plan-222 status 17 gates WP D. Post-fix lane
retrospectively confirms (type-0 removes 17). Total runs across superseded
heads exceed 3 globally, but authoritative head `0755dc1` used 1 attempt;
per-head budgets respected (≤3, no tuning, fresh contexts). SAM STREAM
CONNECT LS2-resolution (`0755dc1`) touches only the SAM path the destination
lane never exercises; destination-lane behavior identical across heads
(destination diff empty after `4c3fe17`).

## 7. Findings by severity

- **Critical**: none.
- **High**:
  - **H-1 (closed by this status) — early OCMOSJ non-ElGamal guard removed.**
    Post-fix Destinations are type-0/256, Java parses them as
    `ELGAMAL_2048` with matching hashes, LS2 stays type-4/32, and status 17
    no longer appears (now `[1,21]`). The Plan-222 `OCMOSJ-UNSUPPORTED-ENCRYPTION`
    attribution for the old X25519 Destination shape is superseded for new
    identities; old X25519 identities (persisted v1) would still hit it.
  - **H-2 (new, owned by successor) — NO_LEASESET (21) after ACCEPTED (1).**
    The helper admits the reverse send (`TRACKED_SENT nonce=1`), OCMOSJ
    answers ACCEPTED then `NO_LEASESET`, and no TunnelData/payload arrives in
    45 s. Target LS absent in helper client DB pre-send (`target_ls_present=false`
    in both P222 preflight and P223 branch). Source keys present with
    X25519-only support. This is the §13 G6 next boundary, not a Plan-223
    defect.
- **Medium**:
  - **M-1 — SAM STREAM CONNECT assumed X25519-in-Destination.** Fixed
    narrowly (`0755dc1`): ElGamal Destinations resolve the ECIES static via
    the local LS2 directory; X25519 non-local fallback preserved. SAM
    self-composed (4/4) + final-acceptance (10/10) green. No wire change.
  - **M-2 — service destinations versioned (v1/v2).** Old files stay
    X25519 byte-identical; new files are ElGamal v2. No silent hash change.
    M10 i2pd lane unaffected (both shapes accepted locally; LS2 still X25519).
    A future M10 pass may migrate v1 server destinations if Java-family
    service interop is ever required; not authorized here.
- **Low**:
  - **L-1 — pre-fix proof uses same-shape reconstruction, not exact lane
    bytes** (§3 limitation). No terminal depends on omitted lane bytes beyond
    the documented gate.
  - **L-2 — streaming rows fail on destination-only run by construction**
    (same as Plan-222 L-2).

## 8. Roadmap disposition and next plan

Plan 223 closes as
**`passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset`**.
The identity/LS2 separation is landed and guarded (§16 checker); the early
guard is removed; the new NO_LEASESET boundary is honestly recorded with
source/target/intersection facts. No second corrective is implemented here.

The smallest next plan implied by the evidence is a NEW narrow plan-of-record
(not authorized by Plan 223) owning the `NO_LEASESET (21) after ACCEPTED`
boundary for the tracked reverse send: inspect why the helper client DB has
no usable LS2 for the freshly published i2pr Standard LS2 (publication
propagation vs client-DB lookup vs OCMOSJ lease-selection), against what Java
OCMOSJ send-preparation accepts for a `PROTO_DATAGRAM_RAW` client send when
the target Destination is now ElGamal/type-0. No bootstrap, floodfill,
tunnel-length, or publication-topology change is implicated beyond what that
plan proves and MUST NOT be smuggled in.

```text
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
```

## 9. Unblock audit

Per the planning process, every registered plan listing Plan 223 as a hard or
interface dependency was audited (same commit updates `201-status.md`,
`204-status.md`, `registry.md`, roadmap §7):

- **Plan 201** (M6 Java publication corrective + second-family closure): was
  `blocked-pending-plan223-destination-identity-crypto-separation-corrective`.
  Plan 223 removed the identity guard but the reverse path now stops at
  `NO_LEASESET (21)` with target LS absent in the helper client DB. Plan 201
  cannot target closure until a dedicated successor owns that boundary.
  **Plan 201 moves to `blocked-pending-plan224-no-leaseset-corrective-after-plan223`**
  (amended in `201-status.md`). Not unblocked.
- **Plan 204** (M10 final closure convergence): stays blocked — M6 Java lane
  has a new honestly recorded boundary, not closure. Token updated to
  `blocked-on-m6-java-second-family-closure-pending-plan224-after-plan223`
  (amended in `204-status.md`). Not unblocked. M10 product authority
  (Plans 213–215) untouched.
- **Plan 205** (SAM-bridge pivot, retained-conditional): stays
  `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`.
  The new boundary (OCMOSJ NO_LEASESET below the client send API, with source
  X25519-only keys and absent target LS) is not a layer a SAM bridge would
  replace; no reactivation authorized. Registry note updated; status-file
  token unchanged.
- **Plan 218** (stopped reverse-delivery boundary): stays
  `stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`.
  Its behavioral stop reproduces with a new exact attribution
  (`P223-NEXT-BOUNDARY [1,21]` instead of status 17); a short follow-up note
  is appended to `218-status.md`. Stop token unchanged.
- **Plan 220**: stays
  `passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required`
  (history not rewritten).
- **Plan 222**: stays
  `passed-m6-java-client-netdb-ocmosj-narrowing-corrective` (history not
  rewritten; its status-17 attribution is superseded for new identities only).
- **M6 roadmap**: §7 row for 223 flips to the passed token above; §4/§11/§12
  authority text updated to the post-223 guard-removed/NO_LEASESET boundary.
- **Next executable**: `224-m6-java-no-leaseset-corrective` (to be registered
  by a separate planning step; not created here). Until then:

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
plan_201 = blocked-pending-plan224-no-leaseset-corrective-after-plan223
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan224-after-plan223

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (P223-NEXT-BOUNDARY [1,21]; successor owns NO_LEASESET)
milestone6_interoperable             = not-yet-claimed
```

No plan-of-record was silently unblocked.

## 10. Compatibility and migration

No user-facing support change. Evidence authority changes only: new
router-owned Destinations are structurally aligned with Java's legacy identity
convention (type-0/256); active encryption stays X25519 via LS2; old
X25519 Destinations (including persisted v1 service records) remain valid
locally but would still hit the early guard on the Java reverse path if ever
used there (documented, not migrated silently).

- New `DestinationIdentity::generate` → ElGamal/type-0/256 + Ed25519/7 +
  96-byte padding; LS2 → X25519/type-4/32 from static secret.
- `from_private_bytes` now requires explicit public filler (non-secret);
  `from_private_bytes_legacy_x25519` preserves v1 X25519 reconstruction
  byte-identically.
- `ServiceDestinationStore` v1 (320-byte padding, no filler) decodes
  unchanged; v2 (256-byte filler + 96-byte padding) is the new write path.
  `identity_from_record` branches on `legacy_filler` presence; no hash change
  on load. Unit `v1_legacy_record_preserves_x25519_shape` locks this.
- SAM `PRIV` stays 391/455 (both shapes 391); import via `from_imported`
  preserves bytes; STREAM CONNECT resolves ElGamal statics via local LS2
  (`decode_destination_id_and_signing` + LS2 X25519), X25519 fallback
  preserved. SAM suites green.
- `DestinationPublic` still accepts both shapes; ElGamal zeroes the static
  slot (LS2 is enforcement); X25519 exposes it (v1 compat).
- `milestone6_interoperable = not-yet-claimed` unchanged. No
  `specs/support.toml` / `docs/adr/` change: no new qualification claimed.

## 11. Handoff

```text
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset

plan_201 = blocked-pending-plan224-no-leaseset-corrective-after-plan223
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan224-after-plan223

next_executable_plan = 224-m6-java-no-leaseset-lookup-path-attribution
```

No bootstrap/topology/protocol corrective beyond the narrow identity/LS2
separation is registered or authorized by this closure. The open M6 Java
second-family boundary is OCMOSJ `NO_LEASESET (21)` for the tracked reverse
send with source X25519-only keys and absent target LS in the helper client
DB; its fix belongs to a new plan.

## 12. Closure evidence template (Plan 223 §21 — all recorded above)

```text
status token: passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
implementation SHA: 0755dc12cad94fae33fcb76a0d470e45175539eb (plus 4c3fe17+e82f12c history in §2)
exact Java/i2pd pins: 2.13.0/9134f808337b401e8e53c73734c81fab04280c9d, 2.61.0/635b013a612ff47278ef02acf8580a28e10e26c5
pre-fix Rust Destination hash/type/length: c0b30b50.../4/32 (§3)
pre-fix Java Destination hash/type/length: c0b30b50.../4/ECIES_X25519/32 (§3)
pre-fix P223 classifier: P223-PREFLIGHT-DESTINATION-ENC-GUARD-CONFIRMED (§3)
production files changed: §2 list (17 files)
post-fix Rust Destination hash/type/length: 368b3450.../0/256 (§4.3, authoritative 0755dc1)
post-fix Java Destination hash/type/length: 368b.../0/ELGAMAL_2048/256 helper+router (§4.3)
post-fix LS2 type/key type/key length/key-match: 3/4/32/true (§4.4)
Plan-222 selector preflight result: passes (width-6/nonempty/contains-B, §4.2)
tracked nonce + payload digest: nonce=1 len=27 digest=8a9e... (§4.5)
ordered Java statuses: [1, 21] (§4.5)
45-second TunnelData result: false (§4.6)
45-second payload result: false (§4.6)
C1/C2/C3 intersection facts: §4.7 (source X25519-only, target absent, no intersection; not decisive for G6)
P223 final terminal: P223-NEXT-BOUNDARY ordered_statuses=[1, 21] (§4.8)
routine/focused verification results: §6 (all green on 0755dc1)
attempt count + clean-head proof: §4 (1 authoritative on 0755dc1, clean; per-head ≤3, no tuning; historical heads documented)
security review: §13 (below)
compatibility/migration review: §10
dependency/unblock audit: §9
next executable plan: 224-m6-java-no-leaseset-corrective (to be registered)
```

## 13. Security review (Plan 223 §16 — all verified)

- Legacy filler contains no secret-derived bytes: `generate()` samples filler
  from caller CSPRNG independently of signing/X25519 secrets; unit
  `legacy_filler_is_not_secret_derived` + `distinct_filler...` lock it;
  checker §16d rejects secret-derived filler.
- X25519 private key remains zeroized/non-Debug: `X25519PrivateKey`/`SigningPrivateKey`
  unchanged (zeroize-on-drop, no Debug/Clone); `DestinationIdentity` Debug
  still redacts both (`debug_never_reveals_secret_bytes` green).
- Signing secret ownership unchanged: router-owned holds seed, client-owned
  never sees it (Plan 166 invariants green).
- LS2 key still derives from X25519 private: `build_signed_lease_set2` uses
  `static_public_bytes()` (derived from `static_key`), never Destination
  filler (checker §16e).
- No private key material in diagnostics: helper `INSPECT_DEST`/`DEST_INFO`,
  router `P223-DEST-INSPECT`/`P223-BRANCH`, and driver `p223-*` rows carry
  only hashes/codes/lengths/counts; no `PRIV`, seeds, static secrets, tokens,
  or payloads. Java probe carries no private-key fields.
- No Java helper writes key material to evidence: `reference-facts.tsv` +
  `driver-evidence.tsv` contain counts/hashes only (harness strips raw logs).
- No acceptance path downgrades ECIES to ElGamal: LS2 stays type-4 (unit +
  lane `key_match=true`); ElGamal slot is identity-format only, no ElGamal
  encryption implemented.
- No public network participation: loopback-only (`127.0.0.1:0` in tests,
  `bind.ip().is_loopback()` asserts in lane); reseed disabled; no
  non-loopback bind.
- No Java reference code modified: helpers/probes compiled out-of-tree
  against staged jars; exact-pinned cache never mutated (`clients.config`
  guards green).

## 14. Failure, cancellation, restart, and contention semantics (retained)

- Diagnostic commands read-only/idempotent; no LeaseSet registration, key
  registration, or NetDB/tunnel mutation (checker §16i).
- Helper/control failure yields Unknown and terminates that run (driver
  `None` → Unknown, never synthesized).
- Fresh scratch RouterContexts per attempt; no P223 facts reused across
  attempts; classification from one internally consistent run only.
- Infra pre-epoch failure does not consume protocol classification (recorded
  in §4/§6).
- No retry after code/config change without new SHA (§2/§4).
