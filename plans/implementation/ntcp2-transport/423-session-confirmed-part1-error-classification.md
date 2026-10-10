# Plan 423 — classify reverse SessionConfirmed Part 1 failures precisely

Status: **stopped** — `stopped-reverse-session-confirmed-io-operation-unattributed`.
Status amendment: `plans/closure/ntcp2-transport/423-status-amendment-plan-424.md`.
Corrective successor to [Plan 422](../../closure/ntcp2-transport/422-status.md).

## Objective

Expose only a finite, secret-safe classification for the responder's bounded
SessionConfirmed Part 1 HandshakeError, preserve it through sanitized runner
evidence, and spend one final reverse-only attempt to identify the first
divergence.

## Why ready

- Plan 422 fixed reverse scenario identity metadata and reached a real TCP
  connection, then stopped at responder_session_confirmed_part1_failed.
- The current status mapping collapses multiple existing bounded errors into
  one reason. Source shows the relevant variants include invalid length,
  truncation, excessive padding, deobfuscation failure, AEAD authentication
  failure, transcript mismatch, and invalid key agreement.
- No raw bytes, keys, identities, endpoints, or payloads are needed to retain
  those existing finite error categories.

## Invariants

1. Change only non-production interop status/runner diagnostics; no protocol
   algorithm or production behavior change.
2. Add only closed enum categories derived from existing HandshakeError
   variants; never serialize error strings, peer bytes, or dynamic details.
3. Preserve the current network-ID-2 loopback-only profile and pristine i2pd
   pin 635b013a612ff47278ef02acf8580a28e10e26c5.
4. Spend exactly one reverse-only attempt; no forward rerun or retry.
5. Normal-daemon NTCP2 remains disabled and non-advertised.

## Work and acceptance

Refine await_confirmed classification so each existing bounded Part 1 failure
maps to a distinct closed status reason; keep Part 2 and RouterInfo identity
failures distinct. Add exhaustive no-process mapping tests and source-check
mutations against collapsing or introducing a dynamic reason. Extend evidence
projection to accept only those new reason codes.

Run focused Rust interop tests, runner/observer/checker self-tests, cargo fmt,
the pristine pinned helper build, NTCP2 vectors and historical boundary,
tooling/plan checks, planning tests, and git diff --check. Then run one
reverse-only attempt. The result must identify a bounded first failure or
reach authenticated session plus correlated DeliveryStatus; otherwise close
with the exact remaining blocker. No protocol fix is authorized without a
localized cause and its own plan-of-record.
