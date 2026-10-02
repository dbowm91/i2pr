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
are not jointly satisfied by a version-only fix, because a probe carrying
`router.version = 0.9.58` with a masked static key and `reservedrange = false` was still
rejected 64/64.

## 3. Required work

- Bisect the conjunction per condition. For each condition, produce one diagnostic probe that
  varies exactly that condition against a fresh reference client, and record the accepted or
  rejected outcome as `diagnostic-only` evidence. Do not promote a probe to a matrix row.
- Fix only the proven cause, at the narrowest layer that owns it.
- If the proven cause is the capability declaration, define the implemented support level as a
  single source of truth, publish exactly that value, and satisfy the `specs/CONFORMANCE.md`
  "Capability advertisement" checklist: feature implications, mandatory behavior verified,
  peer-downgrade tests, and an updated support matrix. Do not copy another router's release
  string, and do not claim a level `specs/support.toml` has not recorded.
- If the proven cause is the address block, fix the address options/keys and keep
  `is_qualified_ssu2_address` honest.
- Land at least one local regression test that pins the fixed property, plus one negative test
  that fails when the property regresses. A production bug is not closed without one.

## 4. Evidence principles

Inherited from Plan 278 §2 unchanged: exact pin, fresh datadirs, no reference patching or
vendoring, no fake peer injection, no production protocol relaxation, no retry-until-green,
no budget increase, reference logs diagnostic-only, sanitized retained evidence.

## 5. Harness scope

Reuse `tests/integration/floodfill/run-i2pd.sh` and
`crates/i2pr-daemon/tests/floodfill_i2pd_external.rs` as landed. Extend the existing
`#[ignore]`-gated diagnostic tests; do not create a second lane, a second Python orchestration
framework, or a second netDb seeding implementation. Environment-gated tests stay
`#[ignore]`-gated and fail closed on explicit selection without the required environment.

## 6. Verification before external execution

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

## 7. Stop conditions

Stop on any reproducible i2pr defect outside the RouterInfo content surface and register a
further narrow corrective. Stop if the bisection proves the reference gate is unsatisfiable
without an unreviewed capability claim; record the exact boundary instead. Do not patch the
reference, raise the budget, or broaden network access.

## 8. Documentation

Update `specs/support.toml`, `specs/CONFORMANCE.md`, the affected architecture deep-dive, and
the floodfill roadmap to state the proven cause and fix. `caps=f` remains non-advertised and
Plan 279 remains blocked; this plan changes no advertisement authority.

## 9. Closure evidence

The per-condition bisection table, the exact source lines of the satisfied gate, the fixed
layer, the regression and negative tests, the retained sanitized evidence, the full routine
floor, and exact-head ordinary CI. On pass, re-open Plan 278 for its matrix.

## 10. Handoff

Plan 278 resumes its A–I matrix. Plan 279 stays blocked on Plan 278. No normal opt-in
`caps=f` is authorized by this plan.
