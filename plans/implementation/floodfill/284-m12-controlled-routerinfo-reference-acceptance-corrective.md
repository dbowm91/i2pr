# Plan 284 — M12 controlled RouterInfo content corrective for reference acceptance

Status at registration:
**registered-m12-controlled-routerinfo-reference-acceptance-corrective**

Classification: narrow external-interoperability corrective.

Hard dependency: Plan 278 stopped at
`stopped-m12-reference-client-rejects-the-controlled-routerinfo-before-any-matrix-row`.
Plans 270–276, 281, 283 stay as closed. Plan 279 stays blocked.

## 1. Objective

Make the i2pr controlled floodfill RouterInfo acceptable to the exact-pinned i2pd 2.61.0
NetDB loader, so the Plan 278 qualification matrix can execute. This plan owns the bisection
of the reference gate and the single narrow fix it proves. It does not qualify anything,
does not add a matrix row, and does not weaken any guard.

## 2. The boundary being cleared

`libi2pd/NetDb.cpp:533-536` (`NetDb::LoadRouterInfo`) admits a record only when all hold:

1. `GetRouterIdentity()` is non-null;
2. `!IsUnreachable()`;
3. `HasValidAddresses()` — for SSU2, `transportStyle == eTransportSSU2 && isV2 &&
   isStaticKey && isIntroKey`, with `isStaticKey` also requiring `!(s[31] & 0x80)`;
4. the offline age bound;
5. `GetVersion() >= NETDB_MIN_ALLOWED_VERSION (0.9.58) || IsHighBandwidth()`.

Plan 278 proved the seeding mechanism is correct (the reference's own record seeds and loads
cleanly through the same ident derivation and file placement) and proved that conditions 1–4
are satisfied by the controlled record, including with a masked static key,
`reservedrange = false`, and an explicit `router.version`.

## 3. Root cause, already localized

Condition 5 cannot be satisfied through `router.version`. i2pd parses that option by
stripping non-digits (`RouterInfo.cpp:457-462`) while the threshold is the packed component
number 2362 (`NetDb.hpp:58`, `MAKE_VERSION_NUMBER(0, 9, 58)`). Real values parse to
`0.9.58 → 958`, `0.9.69 → 969`, `0.9.70 → 970`, `1.0.0 → 100`; none reach 2362. The only
satisfiable branch is `IsHighBandwidth()`, which requires the `O` cap letter
(`RouterInfo.cpp:514-536`). The same rule governs the runtime sweep at `NetDb.cpp:711`.

**Therefore the corrective must not add `O` to i2pr's published capabilities.** `O` is a
high-bandwidth claim; i2pr is an experimental loopback router with a single-bitness pool, so
claiming it would be a false capability advertisement under `specs/CONFORMANCE.md`, ADR 0027
§9, and the repository guardrails. This is a hard constraint on this plan, not a preference.

## 4. Required work

- Replace netDb file seeding of the controlled RouterInfo with an injection path the reference
  admits without a capability claim. `NetDb.cpp:728-730` re-admits a record once its peer is
  connected, and the SSU2 SessionRequest carries the initiator's RouterInfo, so an
  authenticated session is the expected route. Verify this against a fresh reference client.
- If the handshake route does not admit the record, record that exact boundary and stop. Do not
  fall back to a capability claim, a relaxed reference, or a wider network.
- Prove the reference actually treats the controlled router as a floodfill before depending on
  it, rather than inferring it from a single log line.
- Land at least one local regression test that pins whatever invariant makes the reference
  accept the controlled record, plus one negative test that fails when it regresses.

## 5. Evidence principles

Inherited from Plan 278 §2 unchanged: exact pin, fresh datadirs, no reference patching or
vendoring, no fake peer injection, no production protocol relaxation, no retry-until-green,
no budget increase, reference logs diagnostic-only, sanitized retained evidence.

## 6. Harness scope

Reuse `tests/integration/floodfill/run-i2pd.sh` and
`crates/i2pr-daemon/tests/floodfill_i2pd_external.rs` as landed. Extend the existing
`#[ignore]`-gated diagnostic tests; do not create a second lane, a second Python orchestration
framework, or a second netDb seeding implementation. Environment-gated tests stay
`#[ignore]`-gated and fail closed on explicit selection without the required environment.

## 7. Verification before external execution

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
```

Then execute the exact ignored driver commands and re-run the lane to the first matrix row.
Exact-head ordinary CI must be green.

## 8. Stop conditions

Stop on any reproducible i2pr defect outside the controlled-publication surface and register a
further narrow corrective. Stop if the reference admits the record through no honest path;
record the exact boundary instead. Never add the `O` capability or any other unreviewed
capability to make the reference accept the record. Do not patch the reference, raise the
budget, or broaden network access.

## 9. Documentation

Update `specs/support.toml`, `specs/CONFORMANCE.md`, the affected architecture deep-dive, and
the floodfill roadmap to state the proven cause and fix. `caps=f` remains non-advertised and
Plan 279 remains blocked; this plan changes no advertisement authority.

## 10. Closure evidence

The per-condition bisection table, the exact source lines of the satisfied gate, the fixed
layer, the regression and negative tests, the retained sanitized evidence, the full routine
floor, and exact-head ordinary CI. On pass, re-open Plan 278 for its matrix.

## 11. Handoff

Plan 278 resumes its A–I matrix. Plan 279 stays blocked on Plan 278. No normal opt-in
`caps=f` is authorized by this plan.
