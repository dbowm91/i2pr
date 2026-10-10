# Plan 434 — NTCP2 authenticated-link discrepancy recovery (Plan 099 successor)

Status: **active**. Plan 430 passed at `fd6b41ead53fdfad86f230d105e0e2977f92830a`. Activation follows Plan 432 closure commit `e518214`. New NTCP2 plan of record under plans/subsystems/ntcp2-transport-roadmap.md; historical Plans 030–101 remain closed without rewriting closure records.

## Objective and rationale

Determine whether Plan 099's exact noise_authenticated / reference-events-missing split reflects a real i2pr protocol/runtime defect, an observer ordering/correlation issue, or a reference-driver harness defect; correct the *localized* cause and prove two-way authenticated NTCP2 I2NP against pinned stock i2pd. Do not reconstruct the 70-plan NTCP2 apparatus. Plan 101 normal-daemon activation prohibition remains binding until Plan 435.

## Current evidence and ownership

Plan 099 records i2pd's authenticated Noise event, i2pr dialer's tcp_connected followed by reference-events-missing, and no reliable matching ntcp2_authenticated event. i2pr-transport-ntcp2 already owns Noise XK, AES-CBC obfuscation, SipHash frame-length masking, AEAD data-phase and block codecs. i2pr-runtime owns TCP sockets, handshake execution, link promotion and generic inbound I2NP sink. Review Plan 099 SHA/digests and the current Plan 099 minimal driver before running; discard no original evidence.

## Classification/invariants

**Capability:** controlled mixed-router bidirectional NTCP2 and I2NP. **Infrastructure:** precise consuming handshake state transitions and authenticated-link owner lifecycle, stage-based minimal observer. **Invariant:** preserve transcript binding, static key, fresh ephemeral randomness, replay windows, deadlines, no downgrade, full frame/length integrity and bounded outstanding sockets. **Polish:** redacted typed event attribution; retire any redundant diagnostic code introduced. Out: normal-daemon public activation, RouterInfo advertisement, Java-family proof, large harness architecture, global config defaults, non-NTCP2 subsystems.

## Ordered work packages

1. Freeze current official NTCP2 message framing and current i2pd/Java reference pins from specs/SOURCES.md; inspect canonical Plan 099 traces, shortest reproducer and observer contract. Choose one actual stock-i2pd listener and single loopback/isolated-network dialer, pinned versions, no reference source modification.
2. Emit at most one common per-connection correlation token derived entirely from local ephemeral observation metadata (never secret material) on both sides of i2pr-owned events. Record TCP, SessionRequest, SessionCreated, SessionConfirmed, authenticated-link handoff, frame RX/TX, first I2NP, terminal event; explicitly distinguish missing event from protocol rejection. No raw router hash/session secrets in logs.
3. Reproduce a forward connection with bounded attempt budget. Compare exact wire bytes and transitions to spec and stock-router logs. Test partial reads/writes and flush, transcript progression, RouterInfo/transport-key binding, SessionConfirmed, post-auth frame keys and SipHash masking. Record the **first authoritative divergence**; choose a protocol, runtime, or harness classification based on executed evidence.
4. Fix only that defect in owning source; add a deterministic regression with negative and partial-I/O variants. If the failure is observation-only, correct classification without misrepresenting link establishment; still require wire delivery evidence.
5. Execute forward i2pr→i2pd and reverse i2pd→i2pr real NTCP2 connections. Both peers must authenticate and deliver at least one bounded small and one fragmented I2NP message; require valid response or shared DeliveryStatus correlation. Validate teardown and resource baseline, malformed ciphertext/padding/replay and stalled-partial-handshake refusal.
6. Finish with a compact evidence ledger (actual observed stages, two directions, opaque handles, SHA/pin, no synthetic PASS). Update plans/subsystems/ntcp2-transport-roadmap.md and support evidence **without** declaring normal-public transport ready.

## Failure/restart/contention

Abort incomplete handshake within explicit deadline and release pre-auth permits; no public listener or capability is enabled. Exhaustion/backoff and duplicate-link resolution must preserve existing transport-resource budgets. A failed retry cannot reuse consumed nonce/frame key; all socket child scopes joined after failure. Classify environmental failures differently from protocol failures and preserve both.

## Compatibility and verification

No schema change. Do not alter old Plan 099/100 closure or reference runner; may introduce one minimal isolated successor runner. Commands: cargo test --locked -p i2pr-transport-ntcp2 --all-targets; cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1; bash scripts/check-ntcp2-vectors.sh; bash scripts/check-ntcp2-interoperability.sh; one new exact-pinned controlled i2pd forward/reverse invocation; plus full AGENTS.md floor. Actual command strings, pin/digests, stage ledger and CI outcome must be recorded.

## Acceptance, stops, closure

The plan passes **only** on both directions of genuine authenticated NTCP2 plus correlated I2NP delivery and bounded cleanup. Noise authentication at just one endpoint or a missing log line is not sufficient. Stop after a defensible, small number of attempts (freeze budget before running); on unresolved first divergence register one narrow corrective by owning layer, never a new general-purpose interoperability harness. If protocol is fixed but reference unavailable, classify blocked, not passed. Closure plans/closure/ntcp2-transport/434-status.md lists real evidence, risk, and readiness for Plan 435. Public activation remains explicitly forbidden by Plan 101 until later qualification.
