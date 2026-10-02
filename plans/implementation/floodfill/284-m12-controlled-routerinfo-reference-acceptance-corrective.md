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

An earlier revision claimed the version gate was unsatisfiable. That was wrong and is
withdrawn: `MAKE_VERSION_NUMBER` is decimal (`version.h:18`,
`((a*100+b)*100+c)`), so `NETDB_MIN_ALLOWED_VERSION` is 958 and i2pd's digit-stripping
`router.version` parse is consistent with it. The gate is satisfiable.

Three separate gates matter. The first is the proximate cause of the Plan 278 rejection and is
mechanical. The second is the one that cannot be cleared honestly. The third is a path that
looks promising but is not.

1. **Unreachable-by-omission, the proximate cause.** `RouterInfo.cpp:507-508` ends address and
   property parsing with
   `if (!m_SupportedTransports || !isNetId || !m_Version) SetUnreachable (true);`.
   i2pr's record advertises neither `netId` nor `router.version`, so it is marked unreachable
   and loader condition 2 (`!r->IsUnreachable ()`, `NetDb.cpp:531-536`) fails, and the file is
   deleted. This is why all 64 seeded copies were removed. A `netId` that disagrees with the
   reference's own netId (2) also sets unreachable (`RouterInfo.cpp:480-489`).
2. **Floodfill eligibility, the gate that cannot be cleared.**
   `RouterInfo::IsEligibleFloodfill` (`RouterInfo.cpp:1022-1029`) requires
   `m_Version >= NETDB_MIN_FLOODFILL_VERSION`, which is 962, i.e. `router.version >= 0.9.62`,
   and **offers no high-bandwidth alternative**. Every peer-side `m_Floodfills.Insert`
   consults it (`NetDb.cpp:296`, `338`, `473`, `541`) as does `SetUnreachable`
   (`NetDb.cpp:476-478`). The only unconditional insert, `NetDb.cpp:86`, is i2pd's own record.
3. **The wire path does not help.** `NetDb::AddRouterInfo` (`NetDb.cpp:311-352`) is what a
   SessionRequest- or DatabaseStore-learned record goes through. It verifies the signature and
   applies no version or bandwidth check of its own, so an honestly signed record can enter
   `m_RouterInfos`. But the `m_Floodfills.Insert` on that same path still requires
   `IsEligibleFloodfill()`, so gate 2 applies unchanged.

**Consequence: i2pd can only treat i2pr as a floodfill if i2pr advertises
`router.version >= 0.9.62`. No injection path avoids this.**

i2pr must not simply declare it. `O` is a high-bandwidth claim and is false for an
experimental loopback router with a single-bitness pool. `router.version = 0.9.62` asserts
conformance to the I2P 0.9.62 feature set, which `specs/CONFORMANCE.md` restricts to a
reviewed, tested subset and which i2pr does not implement (NTCP2 experimental and
non-advertised, no SSU1, SAM/I2CP and service-tunnels disabled and non-advertised).
`specs/support.toml` records i2pr's real level, below the reference minimum.

This is a conformance boundary, not a code defect.

## 4. Required work

- Record the boundary above as the outcome and close this corrective without changing
  i2pr's RouterInfo content, capability set, transport, or advertisement gates. There is no
  honest code-level fix, and inventing one would be a false claim.
- Do not add `O`. Do not add `router.version = 0.9.62` (or any version at or above it)
  without first passing the `specs/CONFORMANCE.md` capability-advertisement checklist and
  ADR 0027 §9, which i2pr's current support level does not satisfy.
- Publish the boundary so the next plan does not re-derive it. The shortest falsifiable form:
  "stock i2pd 2.61.0 requires a peer to advertise `router.version >= 0.9.62` before it will
  use that peer as a floodfill, and offers no bandwidth-based alternative for eligibility."
- The Plan 278 §9.4 residue is closed: the probe varied `router.version` while leaving
  `netId` absent, so it still failed gate 1. No further diagnostic run is required to decide
  this plan. If the next plan runs a probe to separate gates 1 and 2, it must set both `netId`
  and `router.version >= 0.9.62` together, and the expected outcome is a loaded record that
  i2pd still refuses to use as a floodfill.
- Hand the version-claim decision to a new plan that routes through the conformance gate. The
  options for that plan are a reviewed version claim, a different qualification topology that
  does not require i2pd to treat i2pr as a floodfill, or accepting that one-family stock-i2pd
  floodfill qualification is not reachable. That plan, not this one, makes the call.

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

The honest-path question is already answered, so this corrective closes at the recorded
boundary. Stop immediately if any step would require the `O` capability, a `router.version` at
or above 0.9.62, a patched reference, a raised budget, or broadened network access. Stop on
any reproducible i2pr defect outside the controlled-publication surface and register a further
narrow corrective.

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
