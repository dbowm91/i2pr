# Plan 297 — Local TLS identity for server UseSSL

Status: registered-prop170-tls-identity-blocked-on-plan292

Classification: capability.

Hard dependency: Plan 292 closed with the exact `CorrectivePending{plan: 297}` residual set.

## Objective

Close the Plan 292 `use_ssl` residual (server families) with an
explicit local TLS identity and trust policy. Plan 292 rejects the key
before allocation; this plan makes it a real owner. The pinned PR6
reference maps server `UseSSL` onto TLS between the tunnel endpoint
and the local target; client-side `UseSSL` is outside the frozen
inventory and stays unclaimed.

## Requirements

- Explicit certificate policy: where the tunnel endpoint's TLS
  identity comes from (local provisioning, never silent
  self-signature without operator consent), how the local target's
  certificate is verified (pinning and/or explicit trust roots owned
  by the daemon configuration, never ambient system roots for
  loopback targets without explicit opt-in), and how rotation and
  expiry surface through control state.
- `use_ssl=true` on a server tunnel negotiates TLS to the configured
  loopback target before proxying application bytes; verification
  failure fails the connection (typed, counted) and never falls back
  to plaintext. `use_ssl=false` keeps current plaintext behavior.
- TLS options do not imply interception or MITM capability: the
  endpoint terminates or originates TLS only on the loopback target
  leg it already owns, with no key escrow and no cross-tunnel
  identity reuse.
- Secret handling: private key material follows the existing storage
  precedent (restricted file permissions, redacted wrappers, never in
  control output, logs, or errors).
- Dependency review for any TLS crate use beyond the existing
  workspace `rustls` pin (purpose, features, no ambient root loading
  without explicit configuration).

## Non-goals

No signature agility, LeaseSet security, or outproxy provider work
(Plan 293). No pool shaping work (Plan 296). No new wire keys. No
client-side TLS origination (outside the frozen inventory).

## Evidence

- Positive tests: `use_ssl=true` negotiates TLS to a loopback TLS
  target (pinned identity) through the real server data path;
  `false` stays plaintext.
- Negative tests: verification failure fails closed without
  plaintext fallback; non-TLS target with `true` fails typed;
  `use_ssl` on non-server kinds stays rejected.
- Matrix re-evaluation: the 3 Plan 297 cells flip to apply;
  `CORRECTIVE_297_CELLS` goes to zero.
- Redaction tests: no key material in control output, logs, or
  generation files beyond the restricted secret store.
- No regression of Plans 289–294 control and product paths.

## Acceptance criteria

Plan 297 closes when `use_ssl` is consumed by the explicit TLS
policy owner on all three server kinds, the matrix leaves no
`CorrectivePending{plan: 297}` cell, and the routine floor is green
on the closing commit.
