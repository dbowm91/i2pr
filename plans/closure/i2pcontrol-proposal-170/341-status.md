# Plan 341 — Restart-safe non-echoing outbound proxy secret owner: closure record

Status: **passed-outbound-secret-owner-with-no-routing-and-no-outproxy-claim**

Plan of record:
[`plans/implementation/i2pcontrol-proposal-170/341-restart-safe-outbound-proxy-secret-owner.md`](../../implementation/i2pcontrol-proposal-170/341-restart-safe-outbound-proxy-secret-owner.md)

Registration commit: `328d201`. Implementation and records: this commit.

Date: 2026-10-05.

## Outcome

Plan 341 lands the prerequisite Plan 327's closure record named as its blocker: a
**restart-safe, non-echoing owner for outbound proxy secrets**.

`ProxyCredentials` stays exactly as it was — a one-way inbound verifier that
cannot reproduce a credential. The new owner is separate, has a separate stored
marker, and is the only path by which an outbound credential can be recovered
after a restart.

**Nothing routes anywhere and no outproxy capability is claimed.** The provider,
the HTTP/CONNECT/SOCKS request integration, and the canonical `ProxyList` /
`UseOutproxyPlugin` / `OutproxyAuth` / `OutproxyType` / `SSLProxies` semantics are
**Plan 342**, registered and not implemented. Plan 327 remains blocked, now
blocked on Plan 342 rather than on a missing secret owner. No tunnel option reads
this store yet, and `specs/support.toml` is unchanged.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| An outbound credential survives a restart | `restart_recovers_the_credential_from_the_same_identity` | Pass. Key derives from the persisted signing seed. |
| The stored form is opaque and carries no plaintext | `round_trip_recovers_the_credential`; `the_store_reports_availability_and_never_echoes` | Pass. |
| The credential cannot be printed, copied, or logged | `OutboundSecret` has no `Debug`/`Display`/`Clone`; `OutboundSecretKey` no `Clone`, private `as_key` | Pass, at the type level. |
| A fresh nonce per seal | `a_fresh_nonce_is_used_for_every_seal` | Pass, on the production `OsRng` path. |
| Tampering never yields plaintext | `tampering_is_detected_at_every_position`; `tampering_fails_on_authentication_not_on_an_incidental_check` | Pass. |
| A stolen stored form is inert on another router | `another_router_cannot_open_the_form` | Pass. |
| Inbound and outbound stored forms are not interchangeable | `plaintext_and_verifier_markers_are_not_interchangeable` | Pass. |
| "No owner installed" is a carried state, not a forgotten `Option` | `NoOutboundSecrets`; `the_default_store_fails_closed` | Pass. |
| Plaintext and stored forms are bounded before allocation | `an_empty_or_oversize_plaintext_is_refused_before_sealing`; `oversize_stored_form_is_rejected_without_work` | Pass. |
| Dependency direction preserved | `check-dependency-direction.sh` | Pass. No new crate edge: a trait in `i2pr-service-tunnels`, the AEAD in `i2pr-daemon`. |
| The inbound verifier path is untouched | `cargo test --locked -p i2pr-service-tunnels --all-targets` (313 rows) | Pass. |

## Invariants

1. Plaintext exists only inside a `Zeroizing<[u8; 512]>`, only at the point of
   header construction.
2. No `Debug`, `Display`, error, log, or control output carries plaintext, a
   stored form, or the derived key.
3. Malformed, tampered, and wrong-router values fail closed with no partial
   plaintext; there is no best-effort path.
4. The key is derived fresh from the router identity on every start and is never
   persisted.
5. No secret-bearing type is `Clone`.
6. No new task, timer, queue, listener, or socket.

## Failure, cancellation, migration, and security review

- **Failure.** Every path is fallible and typed. A poisoned or unusable store is
  `NoOutboundSecrets`, which refuses both directions; a compromised or absent key
  produces an authenticator rejection, never a wrong plaintext.
- **Cancellation / restart.** Restart is the design's core case and is covered by
  a row. Nothing durable, no listener, and no task is involved, so there is no
  drain or cancel path to get wrong.
- **Migration.** None. No existing stored form changes meaning; the new marker is
  disjoint from the inbound one. A router that has never sealed anything is
  unaffected.
- **Security.** This is the plan where that matters most, so: the key is
  HKDF-separated from the router signing key; the stored form is inert without
  the router's private identity; plaintext is never heap-allocated, never
  formatted, and zeroized on drop; the AEAD is authenticated with the format
  marker as associated data; the nonce is CSPRNG-fresh per seal. No new external
  dependency was introduced — `chacha20poly1305` was already pinned in the
  workspace, so this is a manifest line and not a supply-chain change. No
  `unsafe`, and the crate keeps `#![forbid(unsafe_code)]`.

## Teeth

Every new row that isolates a decision was verified to fail when that decision is
inverted. All sources were restored from backup afterwards and re-verified green.

| # | Inversion | Rows failed |
|---|---|---|
| T1 | key derived from a constant instead of the router identity | 1 (`another_router_cannot_open_the_form`) |
| T2 | authenticator failure ignored (best-effort open) | 1 (`tampering_fails_on_authentication_not_on_an_incidental_check`) |
| T3 | constant nonce reused for every seal | 1 (`a_fresh_nonce_is_used_for_every_seal`) |
| T4 | stored-form framing pre-check removed | **0** |
| T5 | AEAD associated data dropped | **0** |

T4 and T5 make no row fail because each is redundant with an independent check:
the strict hex decoder rejects everything the framing pre-check rejects, and the
AAD binds a constant that the framing check already enforces. That is recorded as
defense in depth without isolated evidence in the reference dossier, rather than
presented as verified behaviour.

T2 is the row that was **missing** when the first teeth pass ran: the original
tampering rows asserted only that a bad frame errors, which a best-effort open
also satisfies, because the resulting empty value is independently rejected as an
empty credential. The authentication-specific row was added so the rejection
reason is asserted, not just its existence, and T2 then failed as it should.

## Findings by severity

- Critical: 0; high: 0; medium: 0; low: 0.
- Two evidence gaps were found and closed during this plan (the T2 row above);
  neither was a product defect.
- Noted, out of scope, and not fixed here: `i2pr-daemon` has a second
  `rand_chacha` pin (`0.9` and `0.10` aliased) that predates this work.

## Limitations

- **No routing.** Nothing consumes this store. Plan 341 makes a credential
  recoverable; it does not make an outproxy work.
- **No outproxy claim.** Plan 327 stays blocked; `specs/support.toml` unchanged.
- **The framing pre-check and the AEAD associated data have no isolated evidence
  row** (see T4/T5 above).
- **Verification is local.** No external or interoperability lane was run, and
  none is claimed. In particular, this plan does not establish that any I2P
  outproxy accepts these credentials, because no outproxy was contacted.
- **Java's at-rest scheme was not verified**, so no interoperability claim about
  credential storage format is made or implied.

## Verification

Local, on this branch, at the closure state:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --lib outbound_secret
cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection -- --test-threads=1
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-i2pcontrol-acceptance-evidence.sh
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 scripts/check-global-plan-number-uniqueness.py
```

Full serial workspace floor on this branch:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
-> 4 018 passed / 0 failed / 35 ignored across 147 suites
```

That is **+15 rows and +0 suites** against the 4 003 / 147 baseline recorded by
Plan 340: 4 rows in `i2pr-service-tunnels` and 11 in `i2pr-daemon`. It was run
after the two Clippy fixes (`useless_format`, `manual_is_multiple_of`) and the
`cargo fmt` they triggered, so it is the floor at the implementation state. The
record set and the dependency-graph mirror were written afterwards and touch no
`crates/*/src`.

## Docs

- New: `specs/references/proposal-170-outbound-secret-owner.md` (normative).
- Updated: `specs/CONFORMANCE.md`, "Outbound proxy secret owner (Plan 341)".
- Updated: `plans/registry.md` and
  `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`.

## Unblock audit

Plan 341 was registered to clear the prerequisite Plan 327 named. Per the audit:

- **Plan 327** is *not* unblocked. It listed two requirements: the routed
  provider and the secret owner. Only the second now exists, so 327 correctly
  stays blocked and its reopen condition is narrowed to the provider and the
  canonical field semantics.
- **Plan 342** is the new registered plan carrying that remaining scope. Its
  hard dependency (Plan 341) is now closed, so it is the only item that moves.
- Nothing else in the registry depends on Plan 341.

## Roadmap disposition

Plan 341 closed `passed` on 2026-10-05. Plan 327's blocker list shrinks from two
items to one. Plan 342 carries the provider, the canonical fields, and the
HTTP/CONNECT/SOCKS integration, and is registered as the next executable step
with no ownership claim.
