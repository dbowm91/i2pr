# Plan 156 status — Milestone 8 SSU2 v2 handshake, token, and RouterInfo establishment

Status: **`passed-m8-ssu2-v2-handshake-token-and-routerinfo`**.

Registered: **2026-09-03**. Closed: **2026-09-04**.

Plan of record:
[`plans/156-m8-ssu2-v2-handshake-token-and-routerinfo.md`](156-m8-ssu2-v2-handshake-token-and-routerinfo.md).

## Current authority

```text
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo

milestone8_protocol = ssu2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_implementation = plan156-handshake-landed

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 157
next_product_layer = milestone8-ssu2-v2
```

## What this pass did

1. Implemented the SSU2-specific Noise XK transcript (plan §2) in
   `crates/i2pr-transport-ssu2/src/crypto.rs`: protocol identifier
   `Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256` (52 bytes,
   pinned), initial `SHA256(protocol_name)` chaining with
   null-prologue and responder-static mixes, consuming
   role-gated `e,es` / `e,ee` / `s,se` transitions (wrong role or
   stage is `WrongRole`/`InvalidState`), retained `es` cipher for
   the static-key frame (`n = 1`), `split()` into `k_ab`/`k_ba` via
   `HKDF(ck, ZEROLEN, "", 64)`, ChaCha20 header protection exactly
   per the Header Encryption KDF pseudocode, intro-key
   Retry/TokenRequest AEAD, and the `SessCreateHeader` /
   `SessionConfirmed` / `HKDFSSU2DataKeys` labeled derivations.
   X25519/ChaChaPoly/SHA-256/HMAC come from reviewed crates
   (directly or via `i2pr-crypto`); no home-grown primitive.
   Nonce/counter boundaries are checked (`2^64 - 1` never
   emitted); secret owners are zeroizing without
   `Debug`/`Clone`.
2. Implemented the strict establishment codecs and states (plan
   §3) in `handshake.rs`/`state_machine.rs`: TokenRequest, Retry
   (with Termination-form rejection), SessionRequest,
   SessionCreated, and fragmented SessionConfirmed, driven by
   consuming `Initiator`/`Responder` machines emitting only
   `WriteDatagram` / `ArmDeadline` / `Established` /
   `Terminate` / `DropSilently`. No sleep, socket, task, spawn,
   or wall-clock read exists in the protocol crate; all secrets,
   IDs, packet numbers, token bytes, and both clocks arrive as
   explicit parameters.
3. Implemented cheap prevalidation (plan §4):
   `prevalidate_long_datagram` performs length class,
   deprotection, exact header decode, version/network/type, and
   minimum-tail checks with symmetric operations only. Token,
   replay, and skew gates precede every DH. The 200-datagram
   cheap-flood integration test proves invalid traffic allocates
   no session state, invokes no DH, and leaves token/replay
   accounting bounded.
4. Implemented the bounded one-use token lifecycle (plan §5) in
   `token.rs`: exactly 8-byte nonzero values, caller-supplied
   randomness/time, exact source-endpoint binding (IP and port;
   v4/v6 separation via address comparison), 30 s default
   lifetime, 256-global / 4-per-source quotas with deterministic
   oldest-eviction, `rotate` restart invalidation, and the 3x
   Retry amplification budget (plus the 64-byte Retry padding
   cap). No stateless HMAC-cookie shortcut was taken.
5. Proved both establishment paths (plan §6): the tokenless
   TokenRequest → Retry → SessionRequest → SessionCreated →
   SessionConfirmed → Established trajectory and the cached-token
   trajectory both reach matching directional data keys in
   `tests/handshake.rs`. Retry is never retransmitted on timeout;
   handshake resends reuse identical bytes on the spec schedules
   (TokenRequest 3/6 s, Request 1.25/2.5/5 s, Created 1/2/4 s,
   Confirmed 1.25/2.5/5 s) under one 20 s terminal deadline with
   at most 4 attempts per message; no timer-per-packet API exists.
6. Implemented RouterInfo establishment and identity binding
   (plan §7): bounded reassembly (≤15 fragments, 32 KiB
   aggregate, duplicate/conflict detection, RouterInfo-first
   enforcement) and `validate_router_info` (structural decode,
   signature, expected-hash check, `v=2` SSU2 address presence,
   constant-time static-`s` binding, intro-`i` shape where
   required, caller-supplied publication freshness) without
   mutating NetDB. A valid signature with the wrong static key
   terminates without authenticated material.
7. Implemented retransmission/replay/timeout state (plan §8):
   bounded attempt counters, deterministic schedules with
   ceiling, duplicate Created/Confirmed handling, ephemeral
   replay cache (512 entries, 2*D retention), explicit
   handshake deadline, cancellation at every major phase, and
   state release on all failure paths. No `tokio::time`.
8. Narrowed the session output (plan §9) to
   `AuthenticatedSsu2Session`: authenticated peer material,
   directional data-phase ciphers, connection IDs, observed
   endpoint, and local MTU. Intermediate Noise secrets are
   released at `split()`.
9. Added the required tests and vectors (plan §10): 56 inline
   unit tests plus 20 integration trajectories in
   `tests/handshake.rs` covering the tokenless/cached-token
   trajectories, matching-keys both directions, token
   valid/expired/wrong-source/reuse/rotation/unknown matrix with
   pre-DH fail-closed evidence, identical-byte resends,
   duplicate Created/Confirmed handling, deadline exhaustion and
   per-phase cancellation, tag mutation isolation, the 6-case
   RouterInfo binding matrix plus RouterInfo-not-first
   rejection, the 200-datagram cheap flood, amplification
   budget, and secret redaction. Six committed handshake vectors
   (`handshake-initial`, `header-protection-request`,
   `token-request`, `token-retry`, `session-created-full`,
   `session-confirmed-frag`) are pinned in
   `tests/fixtures/ssu2/manifest.tsv` (11 rows total); the
   initial chain carries a raw-primitive independent derivation,
   and the Created/Confirmed vectors reproduce byte-for-byte
   through fixed-secret chains.
10. Documented (plan §11 file list): new modules `crypto.rs`,
    `handshake.rs`, `token.rs`, `state_machine.rs` and
    `tests/handshake.rs`; extended `constants.rs` (quotas,
    schedules, labels); rewrote
    `docs/architecture/i2pr-transport-ssu2.md`; updated
    `overview.md`, `dependency-graph.md`, `tooling.md`,
    `docs/protocol-support.md`, `specs/protocols/09-ssu2.md`,
    `specs/support.toml` (plan pointers plus the
    `ssu2.v2-handshake-token-routerinfo` surface, experimental,
    `advertised = false`), `README.md`, `AGENTS.md`,
    `plans/README.md`, both skill copies
    (`.agents`/`...` and `.opencode/...` are hardlinked), and
    this status record. Reused `i2pr-crypto` (checked DH, RFC
    5869 HKDF, RouterInfo signature verification; the NTCP2
    precedent) with transcript policy kept local; added the
    `i2pr-crypto` edge to
    `scripts/check-dependency-direction.sh` for the SSU2 crate.
    New third-party edges (`chacha20`, `hmac`, `rand_core`,
    `sha2`, `zeroize`, `chacha20poly1305` direct) are all
    already-reviewed workspace dependencies; `cargo deny`
    passes.

## Non-goals kept (plan §12)

No UDP sockets, no active transport registration, no data-phase
ACK/loss implementation, no I2NP fragmentation/reassembly
lifecycle, no path migration, no peer test/relay, no public
address publication, no independent-router execution.
`AuthenticatedSsu2Session.keys` is the exact handoff Plan 157
needs; nothing in this pass sends or receives a UDP datagram.

## Interpretation notes (not stop conditions)

Three specification-text ambiguities were resolved explicitly,
each documented in `crypto.rs` module docs and pinned by
vectors so a later interop plan can revisit them mechanically:

1. The SessionRequest/SessionCreated raw-contents sections
   annotate the 48-byte header-tail/ephemeral region with
   `n: 1`, while the normative Header Encryption KDF pseudocode
   encrypts `packet[16:63]` with the all-zero 12-byte IV under
   `k_header_2`. The pseudocode governs here.
2. `keydata[0:31]` / `[32:63]` and the `packet[len-24:len-13]` /
   `[len-12:len-1]` IV windows use inclusive end indices (32-
   and 12-byte spans); the NTCP2 `MixKey` precedent confirms the
   HKDF construction.
3. Retransmission prose mentions SessionConfirmed packet number
   1, while the header-layout section mandates all-zero packet
   numbers; the header layout governs and retransmissions resend
   identical bytes.

No stop condition fired: no unexplained KDF divergence appeared
(the full chain reproduces byte-for-byte across both roles), no
shared RouterInfo semantic changed, and every required primitive
is represented by a reviewed crate.

## Validation record

Starting SHA: `ae5919408d6d0f3d9115e0a8f4fa1f732b0b7d92`.

Local validation on the closing tree (all green):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
  (1364 passed, 54 suites; Plan 155 baseline was 1315, delta is +49)
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets (76 passed, 3 suites)
cargo test --locked -p i2pr-crypto --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh (11 rows pinned, hashes match)
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh (22 rows command-derived)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' (153 tests, OK)
cargo deny check advisories bans sources (ok)
```

`Cargo.lock` changed only to record the new direct edges onto
already-reviewed workspace dependencies (no new crates
introduced); the temporary fixture-generator example used to
mint the committed vectors was deleted before closure.

Hosted lanes: routine CI and the manual SAM external workflow
must pass on the exact closing commit; the run IDs are recorded
in the handoff commit message and below once available.

## Acceptance criteria (all true, plan §14)

1. v2 Noise transcript/KDF/header protection is vector-backed
   (6 committed vectors; initial chain independently derived).
2. No secret-bearing type leaks through `Debug`/logs (redaction
   tests for keys, tokens, replay tokens, datagram bytes).
3. TokenRequest/Retry/SessionRequest/Created/Confirmed codecs
   are strict and bounded (exact sizes, ceilings, ordering).
4. Full initiator/responder tokenless trajectory reaches
   matching session keys (integration test, both directions).
5. Cached valid-token trajectory reaches matching session keys.
6. Invalid/expired/reused/wrong-source tokens fail before
   avoidable expensive session work (DropSilently + empty
   replay/token-delta evidence).
7. Token storage is bounded at exact-capacity/max+1 and cleans
   up on expiration/consumption (per-source and global eviction
   tests, rotation test).
8. Prevalidation has deterministic cheap-drop tests (unit
   matrix + 200-datagram flood with bounded-state assertions).
9. Handshake resend/deadline state is bounded and no
   timer-per-packet API is introduced (schedules, 4-attempt cap,
   20 s deadline, `ArmDeadline` actions only).
10. RouterInfo fragments are bounded and reassembled exactly
    (15-fragment / 32 KiB ceilings, duplicate/conflict/gap
    tests, multi-fragment trajectory).
11. RouterInfo signature/identity/static-SSU2-key binding is
    enforced (6-case matrix: wrong static with valid signature
    fails).
12. Malformed/tag-mutated/wrong-key handshakes never produce
    authenticated material (mutation test asserts no
    `Established`).
13. Successful output contains only needed post-handshake state;
    intermediate secrets are released/zeroized (narrow session
    struct; zeroizing owners).
14. No UDP/socket/Tokio dependency enters the protocol crate
    (runtime-boundary script green; `std::net` appears only as
    data carriers without the banned literal).
15. Full quality/vector/boundary floor passes (see validation
    record).
16. This record carries exact tests/results and advances only to
    Plan 157.

## Handoff

Plan 156 is closed. Execute Plans **157 → 158 → 159 → 160 →
161** in order under the Plan 154 roadmap authority. Plan 157
owns the authenticated data phase: packet-number/replay window,
ACK scheduling/ranges, bounded loss/congestion/retransmission,
I2NP fragmentation/reassembly, duplicate suppression, and
termination/rekey — consuming `AuthenticatedSsu2Session.keys`.
