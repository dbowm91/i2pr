# Plan 281 status — passed type-5 support-floor correction

Status: **passed-m12-record-floor-corrected-type5-deferred**

- Plan: `plans/implementation/floodfill/281-m12-encrypted-leaseset-floor-correction.md`
- Parent: Plan 280 provider review, stopped without an acceptable candidate
  (`plans/closure/floodfill/280-status.md`,
  `stopped-no-acceptable-maintained-i2p-red25519-provider`).
- Disposition: **passed**. ADR 0027 and Plan 272 now defer EncryptedLeaseSet type 5 until a
  future provider plan passes. No crypto, codec, wire, storage, runtime, or advertisement change.

## Implementation commits

| Commit | Content |
|---|---|
| `9f76f52` — `plans: defer encrypted LeaseSet pending Red25519 provider` | The Plan 281 correction itself: ADR 0027 §4 floor narrowed to RouterInfo plus DatabaseStore types 1, 3, and 7; Plan 272 hard dependencies re-pointed to Plans 271 + 281; `specs/support.toml` deferral rows; `specs/SOURCES.md` Plan 280 source findings; Plan 280/281 plan and closure scaffolding; registry and roadmap updates. Zero `crates/*` files touched. |
| This closure commit | `specs/CONFORMANCE.md` M12 vocabulary stale-dependency fix (Plan 272 "depends on Plan 280" → "proceeds on Plan 271 plus the Plan 281 type-5-deferred floor; Plan 280 stopped") plus this closure-record expansion to the `plans/closure/README.md` required content. No production code or dependency change. |

## Requirement-to-evidence

| Plan 281 requirement | Evidence |
|---|---|
| Correct ADR 0027 to the type-5-deferred floor | `docs/adr/0027-floodfill-role-provenance-and-advertisement.md` §4: floor is RouterInfo plus DatabaseStore types 1, 3, and 7; type 5 deferred/unsupported, never retained in server-authority storage, served, persisted, or replicated; type-11 profile unsupported; Plan 280 found no compatible verifier (commit `9f76f52`). |
| Correct the downstream Plan 272 record-validation floor | `plans/implementation/floodfill/272-m12-floodfill-record-validation-storage.md`: hard dependencies read "Plan 271 passed and Plan 281 passed"; §4C keeps type-5 validation out of scope with no outer validator without Red25519; §9 forbids a type-5 decoder/fuzz target while Red25519 is unavailable; §12 acceptance keeps type 5 unsupported outside server-authority storage (commit `9f76f52`). |
| Correct canonical support inventory | `specs/support.toml`: `plan_280_status = "stopped-..."`, `plan_281_status = "passed-m12-record-floor-corrected-type5-deferred"`, `m12_type11_red25519 = "type5-deferred; ..."`, record-floor notes on the LeaseSet/DatabaseStore surfaces (commit `9f76f52`; `tomllib` parse passes on the closing head). |
| Correct canonical conformance direction | `specs/CONFORMANCE.md` M12 floodfill vocabulary: Plan 272 proceeds on Plan 271 plus the Plan 281 floor; Plan 280 stopped; type 5 deferred pending a separately reviewed provider plan (this closure commit; previously stale "depends on Plan 280"). |
| Record the provider-review provenance | `specs/SOURCES.md` Plan 280 section: `reddsa`/`zakura-reddsa`/registry-search/Go-FFI findings and the exact `I2P_Red25519H(x)` domain requirement; "Plan 281 defers type-5 support" (commit `9f76f52`). `plans/closure/floodfill/280-status.md` retains the full candidate table. |
| Make no crypto or wire implementation change | `git show --name-only 9f76f52` lists only `docs/adr/`, `plans/`, and `specs/` files — zero `crates/*` files. This closure commit touches only `specs/CONFORMANCE.md` and this record. |
| Enforce the deferral statically | `scripts/check-m12-floodfill-boundaries.sh` rejects `DatabaseStoreData::EncryptedLeaseSet`, `ValidatedEncryptedLeaseSet`, and `ServerEncryptedLeaseSet` inside `crates/i2pr-netdb/src` — passes on the closing head, so no server-authority type-5 representation exists. |

## Verification (all run locally on the closing head)

- `git diff --check` — passed.
- Python `tomllib` parse of `specs/support.toml` — passed.
- `bash scripts/check-m12-floodfill-boundaries.sh` — passed (`M12 floodfill boundaries passed`).
- `bash scripts/check-dependency-direction.sh` — passed (`dependency direction: ok`).
- `bash scripts/check-runtime-boundaries.sh` — passed (`runtime boundary checks passed`).
- `git show --name-only 9f76f52 | rg '^crates/'` — no match, confirming the correction commit carried zero production files.

No `#[ignore]`-gated external lane applies to this documentation-only correction; no
reference-router evidence is claimed. CI truthfulness: hosted CI verification is not
claimed by this record — the commands above are local-only.

## Invariant review

The type-5-deferred floor is preserved end to end: ADR 0027 §4, the Plan 272
implementation constraints, `support.toml`, `CONFORMANCE.md`, and the M12 boundary script
agree that only RouterInfo plus DatabaseStore types 1, 3, and 7 enter validated
server-authority storage, and that type 5 can never be served, persisted, or replicated
without a new vetted-provider plan. No invariant was weakened to obtain this pass.

## Failure / migration / compatibility review

There is no runtime behavior change, so there is no failure-mode, migration, or
compatibility delta. Plan 272's implementation (commit `4b4e767`) was built after this
correction on the deferred floor and its closure record confirms the same posture. No
stored data, wire format, config surface, or API changes shape in this plan.

## Security review

No new cryptographic code, dependency, parser, or network-reachable surface. The security
effect is strictly narrowing: type-5 payloads stay outside server-authority trust, and the
boundary script statically bars the three server-side EncryptedLeaseSet representations.
No secret material is involved; nothing is logged or serialized beyond the existing
plan/support prose.

## Documentation / operational evidence

Authoritative surfaces agree on the closing head: ADR 0027 §4, `specs/support.toml`
(`plan_281_status`, `m12_type11_red25519`), `specs/CONFORMANCE.md` M12 vocabulary,
`specs/SOURCES.md` Plan 280 section, `specs/protocols/04-reseed-netdb.md` (types 1/3/7
with type 5 deferred under Plan 281), the Plan 272 handoff, the floodfill roadmap §6/§7/§10
(281 `passed`; dependency chain `280 -> 281 -> 272`), and `plans/registry.md` (281 in
recently closed as `passed`). No operator action follows; floodfill remains
non-advertised with `m12_caps_f_advertised = false`.

## Known limitations

- This is an architecture/support-floor correction only: no EncryptedLeaseSet validation,
  serving, persistence, replication, qualification, or advertisement is implemented or claimed.
- Type-5 support requires a future, separately registered vetted-provider plan with exact I2P
  Red25519 official vectors and supply-chain review; no such plan is registered.
- The Plan 281 implementation narrative (`plans/implementation/floodfill/281-...md`) is a
  30-line bounded correction note, not the full handoff template; it is retained verbatim as
  history and this status record carries the authoritative evidence.

## Findings by severity

- Critical: none.
- High: none.
- Medium: one, now closed by this commit — `specs/CONFORMANCE.md` still described Plan 272
  as depending on Plan 280 after the correction landed, contradicting ADR 0027 and the Plan
  272 handoff. Fixed here; no other stale Plan-280-dependency reference was found
  (`support.toml`, roadmap, registry, and protocol dossier already agree on the deferred floor).
- Low: none.
- Note (not a finding): the original 26-line closure scaffolding from `9f76f52` lacked the
  `plans/closure/README.md` required sections; this expansion cures that without rewriting
  any historical verdict.

## Roadmap disposition

Closed. The floodfill roadmap §7 row for Plan 281 stays `passed`
(`passed-m12-record-floor-corrected-type5-deferred`); no roadmap or registry state change
is required in this commit beyond recording the CONFORMANCE fix and this expanded evidence.

## Unblock audit

Audited `plans/registry.md` blocked work plus the floodfill roadmap §6 dependency graph
against the just-closed milestone:

- Plan 272 (hard deps: Plan 271 + Plan 281) already passed on this floor, as did Plans
  273–276 downstream; no state change needed or made.
- Plan 280 remains `stopped` (provider unavailable); future type-5 support still requires a
  separately registered vetted-provider plan — none is registered by this closure.
- Plans 278–279 remain `blocked` behind the active Plan 282 corrective, in declared
  sequence; Plan 277 remains `stopped` with its partial work retained by Plan 282.
- No other registered plan lists Plan 281 as a hard or interface dependency with all
  remaining deps satisfied.

Result: no plan becomes newly ready from this closure. The only executable M12 plan
remains Plan 282; `next_executable_plan` is unchanged.
