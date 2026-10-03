# Plan 293 status — Signature, LeaseSet security, and provider-class option determinations

Status: **`passed-prop170-deep-tunnel-option-determinations`**.

Plan of record: [`293-signature-leaseset-security-and-provider-option-completion.md`](../../implementation/i2pcontrol-proposal-170/293-signature-leaseset-security-and-provider-option-completion.md).

Hard dependency closed: Plan 292
(`passed-prop170-tunnel-option-matrix-and-noncrypto-completion`).

## Implementation commit

- `b8b80ae` — `plan(293): deep tunnel options resolved as
  explicit incompatibilities` (matrix disposition flip, daemon
  rejection reasons, matrix/daemon tests, spec 14 dossier, support
  surface, spec 13 + arch notes).

This closure commit (closure record + registry + roadmap) lands on
top of `b8b80ae` with no production-code change.

## Outcome

Zero promotions, by determination — not by deferral. Every one of
the 30 deep cells is either backed by a real owner (none qualify)
or demonstrated impossible under the pinned Proposal plus
accepted project guardrails, with an explicit compatibility
limitation that Plan 295 carries into the final support claim
(plan acceptance criterion 2). The former `BlockedPrimitive`
disposition is removed: the matrix now records 227 apply, 37
not-applicable, 30 explicit incompatibilities, 39 corrective-296,
3 corrective-297 (336 total, unchanged).

The determinations match the pinned reference fork's own
zero-promotion outcomes at eggstack/emissary @ `6885a945`
(M121 Outcome C for SigType, M146 closed-as-blocked for the
outproxy provider, M152 terminal, M162 blocked for LeaseSet
security), researched read-only for this plan; no fork code was
imported and no new dependency was added.

## Requirement-to-evidence matrix

| Plan 293 requirement | Evidence |
|---|---|
| SigType (§A): audit value set vs i2pr support; algorithm-aware identity or explicit-unsupported establishment | Ed25519-only by ADR 0004 (`DestinationPublic` rejects non-7, crypto funnels Ed25519-only, no legacy provider in workspace); accepting the lone generatable value would be inert (selects nothing), matching reference M121; all 12 cells `ExplicitIncompatibility`; any supplied value (Java spelling, numeric, canonical, legacy) rejected across all 12 types |
| LeaseSet security (§B): blinded/encrypted publication, lookup/password/blinding inputs, PSK/DH client-auth through canonical owners — or precise blocker | Precise blocker: no blinded-publication owner (BLINDED deferred end-to-end), no type-5 framing owner (Deferred/rejected), no PSK/DH verifier, no lookup-secret derivation, no rotation owner; all 16 cells `ExplicitIncompatibility`; explicit disable also rejected (omission selects ordinary type-3 publication), matching reference M162 |
| Outproxy provider (§C): safe semantic equivalent or explicit named incompatibility | Explicit incompatibility: no provider/registry/ABI exists, i2pr has no outproxy at all, and a provider would need a forbidden clearnet subsystem; no dummy provider built (reference M146 closed blocked on the same ground); both cells `ExplicitIncompatibility` |
| Atomicity: whole security configuration validated before durable/runtime effect; identity-replacing changes never masquerade as in-place edits | `normalize_definition` rejects incompatible keys before the definition mirror, store, listener/session allocation, or task spawn (unchanged transaction path); incompatible keys never reach diffing, so no diff class applies |
| No local crypto primitives | No new crates, no new dependencies (`git show` has no `Cargo.toml` change), `cargo deny` clean |
| Exact matrix re-evaluation with no inert applicable cell | Census 227/37/30/39/3 asserted; 30-cell sweep pins the exact class limitation per cell and asserts no other cell carries one |
| Secret discipline (password/blinding/auth companions) | Keys stay Secret-classified; rejected before storage with messages naming the key but never the value (pinned); no `Debug`/`Display`/serialization added; zeroization N/A (no secret is ever held — rejection precedes handling) |
| Positive/negative vectors per mode; mismatch tests | Negative matrix: 4 SigType spellings × 12 types, 6 LeaseSet values × 4 publishing kinds, 3 outproxy values × 2 proxy kinds, plus out-of-mask pairs — all fail with the named limitation. Positive vectors N/A: no mode was added (criterion-2 closure); ordinary Ed25519/type-3/direct-routing behavior is covered by the unchanged Plans 289–292 suites |
| Restart/persistence/rotation | N/A by design: incompatible keys are never stored, so no persistence/rotation surface exists; the 292 restart test passes unmodified |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK (also clean on MSRV 1.88.0)
cargo test --locked --workspace --all-targets -- --test-threads=1          fully green, zero failures (no flakes this run)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-i2pcontrol --test tunnel_matrix                 7 passed (incl. new 30-cell limitation sweep)
cargo test --locked -p i2pr-daemon --lib i2pcontrol_tunnels                31 passed (incl. new per-key-per-type rejection matrix)
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels                11 passed
bash scripts/check-dependency-direction.sh                                 dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                   runtime boundary checks passed
bash scripts/check-service-tunnel-boundaries.sh                            service-tunnel boundary checks passed
bash scripts/check-ntcp2-interoperability.sh                               OK
bash scripts/check-constrained-host-lane-boundary.sh                       OK
bash scripts/check-m11-transit-boundaries.sh                               passed
bash scripts/check-m11-transit-qualification-evidence.sh                   175 guarded rows green
bash scripts/check-sam-acceptance-evidence.sh                              22 rows green
bash scripts/check-ssu2-acceptance-evidence.sh                             15 rows green
bash scripts/check-i2cp-acceptance-evidence.sh                             24 rows green
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                   11 labels green
bash scripts/check-streaming-tunnel-evidence.sh                           macOS-blocked (bash 4+, see below)
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK (18 tests)
cargo deny check advisories bans sources                                   advisories ok, bans sources ok
```

Not run locally (pre-existing macOS environment limit, identical on
unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`,
`check-service-tunnel-acceptance-evidence.sh`,
`check-streaming-tunnel-evidence.sh` require bash 4+ (`declare -A`
or newer quoting this host's bash 3.2.57 rejects at parse time).
Linux CI is authoritative for those six. (The streaming checker
was verified blocked-only, not listed, in the 292 record; it is
listed here for completeness.)

## New dependencies (reviewed)

None. No `Cargo.toml` change; `cargo deny` clean.

## Security review

- The six keys fail before any secret handling: the three
  LeaseSet companions never reach the definition mirror, store,
  logs, or error values (key named, value absent — pinned).
- No `Debug`/`Display`/unrestricted serialization added; no new
  `Clone` on secrets; no listener/socket/behavior change beyond
  rejection reason strings.
- No outproxy/clearnet path opened; no TLS implied; no legacy
  crypto generation introduced.

## Failure / migration review

- Rejection precedes allocation on every path (create/edit,
  every applicable type, every value including explicit
  disable/false/empty). The stage/publish/verify transaction is
  untouched; a rejected request leaves the previous generation
  and running destination untouched.
- No migration concern: incompatible keys were never storable,
  so no stored definition can contain them.

## Documentation / operational evidence

- `specs/protocols/14-tunnel-deep-option-determinations.md`: the
  Plan 293 determination dossier (new).
- `specs/protocols/13-tunnel-option-matrix.md`: census updated
  to the 293-resolved state with a pointer to spec 14.
- `specs/support.toml`: new
  `prop170.deep-tunnel-option-determinations` surface
  (`experimental`, `advertised = false`).
- `docs/architecture/i2pr-i2pcontrol.md`: `tunnel_matrix` row
  updated (census + `INCOMPATIBLE_CELLS` export).

## Items carried to Plan 295 (wire-shape divergences found)

Recorded in spec 14 §Items-carried; Plan 295 must adjudicate them
against the pinned Proposal before any final support claim.
Summary:

1. `encrypt_lease_set`: inventory Boolean vs reference ten-mode
   string enum.
2. `leaseset_client_auth` (+ password/blinding companions):
   split Secret strings vs reference `LeaseSetClientAuths`
   array-of-objects + `OptionalLookup` (absent from inventory).
3. `sig_type`: inventory String-only vs reference
   text-or-integer.
4. `use_outproxy_plugin`: inventory String vs reference boolean;
   applicability two families (i2pr mask) vs four (reference
   M146).
5. `AddressBook` `SetConfig`: frozen thirteen vs reference M096
   thirteen (disjoint sets; recorded for Plan 294, which builds
   against the frozen inventory, and Plan 295 adjudication).

None changes a Plan 293 disposition: every value of these keys
is rejected regardless of shape.

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six
  bash-4-only checkers (environment debt, unchanged); the five
  wire-shape divergences above (recorded for 295, no disposition
  impact).

## Roadmap disposition

Plan 293 is **closed** (`passed-*`). Plans 294, 296, 297 stay
`ready` (unaffected). Plan 295 stays blocked (294 still open).
No other Proposal 170 capability is claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 293 (hard dep: Plan 292, closed) — moves to `closed`
  in this commit.
- Plan 294 (hard dep: Plan 287, closed) — already `ready`;
  unaffected.
- Plan 296 (hard dep: Plan 292, closed) — already `ready`;
  unaffected.
- Plan 297 (hard dep: Plan 292, closed) — already `ready`;
  unaffected.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked
  (294 still open).
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: the only 293 work product is the
determination set above, and every deep cell carries its named
limitation with executed rejection evidence.
