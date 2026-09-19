# Plan 225 — M6 Java NO_LEASESET lookup-path observability corrective

Status: **registered-ready-m6-java-no-leaseset-lookup-path-observability-corrective**

## 1. Bounded objective

Turn the Plan-224 lookup-path observability gap into one exact, causal
attribution for the Java I2P 2.13.0 `ACCEPTED -> NO_LEASESET (21)` boundary.

Plan 224 proved that the exact target LeaseSet2 was current and
`receivedAsPublished` in Router B's main NetDB, while Router A's helper client
sub-DB remained empty before and after the tracked send. The disposable,
class-scoped logger configuration was installed, but no trustworthy
target-correlated lookup facts were emitted or parseable. Plan 225 owns only
the diagnostic/observability correction needed to distinguish:

```text
A query dispatch -> B lookup receipt -> B published-LS answer
-> A inbound-client-tunnel DSM receipt -> A client-subDB installation
```

It must stop at the first proven stage and must not infer a stage from status
21, selector membership, or absence from an unavailable log.

## 2. Current authority

```text
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = registered-ready-m6-java-no-leaseset-lookup-path-observability-corrective

plan_201 = blocked-pending-plan225-no-leaseset-lookup-path-observability-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan225-observability
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 225-m6-java-no-leaseset-lookup-path-observability-corrective
```

The M6 Java second-family capability remains unclaimed. Plan 225 is a
diagnostic corrective, not a requalification or production protocol fix.

## 3. Evidence boundary inherited from Plan 224

Plan 224's authoritative implementation SHA is:

```text
96824f8e5e2cd56c90bb94ebb12aedad68435e66
```

Its exact destination-only lane established:

- Router B main NetDB: raw and validated target LS2 present, current,
  `receivedAsPublished=true`, one lease, one type-4 key;
- Router A helper client DB: resolved and client-scoped, target absent before
  and after the tracked send;
- one nonce-correlated send: nonce 1, ordered statuses `[1, 21]`;
- no matching reverse TunnelData or payload in the frozen 45-second window;
- no reliable exact lookup-path trace, despite the targeted scratch logger
  configuration being installed.

Plan 225 must preserve the exact target, publication target, helper, selector
semantics, destination/LS2 split, and frozen acceptance window. It must not
reinterpret the P224 gap as proof that A did not query B.

## 4. Allowed work

Plan 225 may change only test/diagnostic machinery:

- validate the pinned Java logger configuration's effective activation before
  the tracked send;
- add bounded, read-only, target-correlated diagnostic probes that do not
  initiate or prime a LeaseSet lookup;
- improve whitelist-only sanitization and typed fact correlation;
- retain raw Java output only in disposable scratch directories;
- rerun the existing one-send destination lane against the exact Java pin.

It may not change production i2pr behavior, Java reference source, router
topology, tunnel quantities/lengths, helper options, floodfill roles,
publication policy, selector logic, SAM, timeout/window values, or protocol
advertisement. Reflection, `setAccessible`, Java NetDB mutation, direct LS2
copying, standalone lookup probes, and publication retries are forbidden.

If exact target correlation cannot be made reliable through supported,
read-only diagnostics, Plan 225 must close with a typed observability stop and
register a narrower plan rather than asserting a lookup-stage root cause.

## 5. Required terminal and handoff

Emit exactly one `P225-*` terminal per authoritative attempt. The terminal
must be the earliest proven stage among the existing Plan-224 lookup stages,
or an explicit observability gap if the evidence remains incomplete.

The successor must hand Plan 201 and the M6 roadmap one of:

```text
P225-ATTRIBUTION-<earliest-proven-stage>
P225-OBSERVABILITY-GAP-LOOKUP-PATH
```

Only a later plan may implement a protocol/runtime correction. Plan 201 stays
blocked until that result and the resulting Java-family closure are complete;
Plan 204 stays blocked on the same M6 dependency; Plan 205 remains off-path.

## 6. Execution contract

All implementation is committed before counted execution. Use the exact
Java/I2P pins and the existing `I2PR_M6_JAVA_DRIVER=destination` lane. Allow
at most three fresh-scratch attempts per implementation SHA, with no timing,
topology, or configuration tuning between attempts. Preserve sanitized
evidence only and run the repository routine/focused verification floor before
closure.
