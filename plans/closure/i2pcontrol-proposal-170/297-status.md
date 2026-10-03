# Plan 297 status — Local TLS identity for server UseSSL

Status: **`passed-prop170-local-tls-identity-for-use-ssl`**.

Plan of record: [`297-local-tls-identity-for-use-ssl.md`](../../implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md).

Hard dependency closed: Plan 292 (`passed-prop170-tunnel-option-matrix-and-noncrypto-completion`)
with the exact `CorrectivePending{plan: 297}` residual set (3 cells).

## Implementation commits

Branch `plan-297-local-tls-identity`, on top of `7b0f3aa`
(Plan 296 closure):

- `7be60c0` — TLS policy owner, `use_ssl` spec field, daemon
  config section, composition install, per-connection TLS
  upgrade on generic and HTTP server paths, conditional GET
  state, matrix flip, live product tests, spec/support/arch
  updates (including the §Corrective restructure from the
  `dangerous()` custom verifier to pure-WebPKI anchors).

This closure commit (closure record + registry + roadmap) lands on
top with no production-code change.

## Outcome

All 3 Plan 292 residual cells are consumed by the real TLS
owner; the matrix leaves no `CorrectivePending` cell at all:

- `APPLY_CELLS = 269` (was 266), `NOT_APPLICABLE_CELLS = 37`,
  `INCOMPATIBLE_CELLS = 30`, `CORRECTIVE_296_CELLS = 0`,
  `CORRECTIVE_297_CELLS = 0`.

## Requirement-to-evidence matrix

| Plan 297 requirement | Evidence |
|---|---|
| Explicit certificate policy (provisioned identity, no silent self-signature) | `[service_tunnels.tls]` with optional PEM pair; unpaired halves rejected at load; `use_ssl` without any installed policy fails at staging before any allocation; no generation path exists anywhere |
| Target verification (pinning and/or explicit roots, never ambient) | Exact-certificate pins and explicit CA roots share one WebPKI trust store; ambient system roots never consulted; verifies-nothing rejected at load; verification failure fails typed/counted with no plaintext fallback |
| Rotation and expiry through control state | Restart-to-rotate (documented); provisioned identity X.509 expiry + verify mode + per-runtime handshake counters in the conditional GET `tls` object |
| `use_ssl=true` negotiates TLS to a loopback TLS target | Live pinned-TLS roundtrip through the real generic-server data path with `(1, 0)` handshake counters |
| `use_ssl=false` keeps plaintext | Full existing suite green unchanged; plaintext-against-TLS-target fails instead of downgrading |
| No interception/MITM capability | Endpoint acts only on the loopback target leg it already owns; no key escrow; no cross-tunnel identity reuse (one policy, per-dial configs, no shared sessions) |
| Secret handling | Restricted-permission precedent documented; redacted `Debug`; generation files carry only the boolean; control output carries mode/expiry/counters only; config errors are static |
| Dependency review | `x509-parser` 0.18 (existing workspace pin, also used by `i2pr-netdb`) newly consumed by `i2pr-daemon` for X.509 expiry parsing; `rustls`/`tokio-rustls`/`rustls-pki-types`/`rcgen` reuse existing pins with no version/feature change; no new crate |
| Matrix re-evaluation | Option 6 → `ServiceTunnelSpec.use_ssl into server TLS target dial` on all 3 server kinds; census tests pin 269/37/30/0/0 |
| Redaction tests | Generation-file scan (no PEM markers), control-output scan (no certificate bytes), policy `Debug` scan (no PEM) |
| No regression | Full serial workspace floor green (131 suites), all acceptance/boundary checkers green |

## Acceptance criteria disposition (§Acceptance criteria, plan of record)

1. `use_ssl` consumed by the explicit TLS policy owner on all
   three server kinds — yes (generic + HTTP server paths share
   the upgrade; bidirectional rides the HTTP server path; kind
   gate + matrix mask cover all three cells).
2. No `CorrectivePending{plan: 297}` cell remains — yes
   (tunnel-matrix census + final-matrix census green).
3. Routine floor green on the closing commit — yes (see below;
   six bash-4 scripts remain Linux-CI-authoritative on this
   bash-3.2 host, as at Plans 294–296).

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK (normalized)
cargo check --locked --workspace --all-targets                             OK (also clean on MSRV 1.88.0)
cargo test --locked --workspace --all-targets -- --test-threads=1          131 suites green, exit 0
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-daemon --test service_tunnel_tls_product -- --test-threads=1   4 passed
cargo test --locked -p i2pr-daemon --lib service_tunnels_tls               7 passed
cargo test --locked -p i2pr-daemon --lib config                            config suite green
cargo test --locked -p i2pr-daemon --lib i2pcontrol_tunnels                32 passed
cargo test --locked -p i2pr-i2pcontrol --all-targets                       contract+matrix+final green
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
bash scripts/check-i2pcontrol-acceptance-evidence.sh                       ok
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                   11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK (18 tests)
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical
on unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`,
`check-service-tunnel-acceptance-evidence.sh` (bash 3.2.57
parse-rejects line 1952; file untouched since Plan 215),
`check-streaming-tunnel-evidence.sh` require bash 4+ on this
host's bash 3.2.57. Linux CI is authoritative for those six.

## New dependencies (reviewed)

`x509-parser` 0.18 newly consumed by `i2pr-daemon` (existing
workspace pin, same version/features as `i2pr-netdb`'s use for
SU3 verification; purpose: X.509 end-entity expiry extraction
for control-state surfacing; no `unsafe` exposure beyond the
already-reviewed crate; license unchanged). `rustls 0.23` /
`tokio-rustls 0.26` / `rustls-pki-types` / `rcgen 0.13` reuse
existing workspace pins with no version or feature change; no
new crate; no `Cargo.lock` version churn beyond the added edge.
`cargo deny` (advisories/bans/sources) is clean.

## Security review

- No new sockets, tasks, files, channels, or listeners; one TLS
  upgrade on an already-established loopback stream per
  connection; per-dial client configs (no shared sessions, no
  cross-tunnel identity reuse).
- No `Debug`/`Display`/unrestricted serialization on secret
  types; no new `Clone` on secrets (private key moves through
  `PrivateKeyDer`, zeroized on drop by `rustls-pki-types`);
  redacted `Debug`; static error strings; counters only in
  control state.
- Verification is always the standard WebPKI path over
  operator anchors; there is no bypass hook, no ambient roots,
  no TOFU, and no opt-out (see §Corrective).
- Unix targets and non-server kinds reject `use_ssl` by kind
  gate and matrix mask; the IRC server path is untouched (out
  of mask).
- Client-side `UseSSL` stays unclaimed (outside the frozen
  inventory); no new wire keys.

No high/medium/low findings beyond the noted environment debt.

## Failure / migration review

- No persisted-state shape change: the control generation files
  carry the same options-map schema (`use_ssl` flows as an
  ordinary boolean entry); daemon TLS configuration lives in
  operator-owned config files outside generations; no migration
  exists or is needed.
- TLS policy is memory-only, loaded fail-fast at configuration
  time; rotation is configuration change plus restart
  (documented; no hot-reload path to break).
- `use_ssl` edits are `MutableInPlace` (per-connection
  committed reads); a removed policy fails staged `use_ssl`
  tunnels at the next prepare, never mid-connection.
- Whole-request validation and disabled-isolation semantics are
  unchanged from Plans 287–296.

## Corrective applied during execution

The first implementation verified SPKI hash pins through a
`dangerous()` custom certificate verifier. The
CI-enforced runtime boundary (`check-runtime-boundaries.sh`)
forbids verification-bypass primitives in production daemon
source without exception (fix code, never weaken scripts), and
carving a lexical exception would evade the guardrail rather
than honor it. Restructured to pure-WebPKI verification:
pinning is exact end-entity certificates as trust anchors (same
operator trust decision, standard path), the custom verifier,
hash-pin parsing, SPKI hashing, and the dummy-anchor
scaffolding were all removed, and the unauthenticated opt-in
was dropped (fail-closed beats permissive: with no opt-out
path, ambient roots can never engage). The plan's core
requirement (pinning and/or explicit roots, never ambient) is
fully implemented; the opt-in was a permissive extra in
tension with a hard boundary, and its removal is recorded here
rather than hidden.

## Documentation / operational evidence

- `specs/protocols/13-tunnel-option-matrix.md`: census
  269/37/30/0/0 plus the server-TLS section (policy shape,
  verification, rotation/expiry, edit class, PR6 mapping,
  client-side non-claim).
- `specs/support.toml`: new `prop170.local-tls-identity-for-use-ssl`
  surface (`experimental`, `advertised = false`); prior plan rows
  untouched.
- `docs/architecture/i2pr-i2pcontrol.md`,
  `docs/architecture/i2pr-daemon.md`: matrix counts, new TLS
  module row, dial-path extension.
- Plan 292–296 closure records and the Plan 295 dossier keep
  their historical counts (no rewrite).

## Known limitations

- Client-side `UseSSL` is unclaimed (outside the frozen
  inventory); the endpoint identity is offered as a client
  certificate but never required by i2pr.
- Mutual-TLS (target requesting client authentication) is
  implemented in the handshake but untested live (the product
  fixture server does not request client auth); the provisioned
  identity path is covered at load/parse/expiry level.
- Trust anchors do not hot-reload; rotation is configuration
  change plus restart (documented).
- I2CP has no TLS keys (separate API surface, out of scope).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the
  `dangerous()`-verifier corrective above (restructured with
  regression coverage before closure); the six bash-4-only
  checkers (environment debt, unchanged since Plan 286).

## Roadmap disposition

Plan 297 is **closed** (`passed-*`). The Proposal 170
workstream is fully closed (Plans 286–297 passed); no further
Proposal 170 capability is claimed. Final matrix:
336 cells at 269/37/30/0/0.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 297 (hard dep: Plan 292, closed) — moves to `closed` in
  this commit.
- No registered plan lists Plan 297 (or any Proposal 170 plan)
  as a hard or interface dependency; nothing newly unblocks.
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 297 acceptance
criterion passes with executed evidence above. The deferred
items (live mutual-TLS proof, hot reload) are explicit
documented boundaries, not hidden gaps.
