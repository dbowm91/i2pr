# Plan 295 — Full Proposal 170 source completion and cross-router conformance

Status: registered-prop170-final-conformance-blocked-on-plan288-plan293-plan294

Classification: evidence/closure + residual capability completion.

Hard dependencies: Plans 288, 293, and 294 closed. Plans 289–292/291 are therefore transitively closed.

## Objective

Close remaining truthful RouterInfo source gaps, requalify the complete secured control plane on the current repository head, perform differential compatibility against pinned external references, and establish the exact final Proposal 170 support claim.

This is not a frontend plan.

## Re-freeze before qualification

At execution start:
- re-fetch Proposal 170 and base I2PControl documentation;
- compare them to the Plan 286 frozen revision;
- if the public proposal changed materially, stop and register a spec-reconciliation plan rather than silently changing wire behavior;
- pin the exact current i2pr implementation head;
- pin the donor/reference heads actually used in the final comparison.

## Complete RouterInfo sources

Resolve every remaining Plan 288 unavailable row that is required for full support.

Expected source work may include:
- bounded redacted router log ring fed from tracing without unbounded/cardinality-sensitive fields;
- authenticated/signed router-news refresh/cache with explicit stale/unavailable policy;
- request-independent transit bandwidth rolling windows;
- tunnel success-rate total/recent metrics;
- build and TBM queue depth snapshots;
- clock-skew nullable/neutral semantics where allowed;
- v4/v6 reachability/testing/error projections;
- peer/ban capability finalization;
- AddressBook source rows from Plan 294.

Every source has a dedicated owner and bounded snapshot. Expensive whole-router scans on each JSON-RPC request are not acceptable.

## Final public contract matrix

Produce an authoritative machine-readable matrix covering at least:
- base Authenticate behavior;
- every Proposal RouterInfo selector;
- AddressBook operation/config key;
- ClientServicesInfo selector;
- every TunnelManager action/type;
- every type×option applicability cell.

For each cell record wire/source/runtime/persistence/isolation/security/evidence status.

The matrix is generated/validated by tests and summarized in docs. No prose-only counting.

## Differential references

### eggstack/emissary

Use the project-owned fork as the strongest Proposal 170 behavioral and adversarial oracle. Differentially compare literal request/response shapes, error behavior, authentication edge cases, option applicability, secret filtering, restart semantics, and source truthfulness where both routers expose analogous state.

Do not require implementation-internal parity.

### Java Proposal 170 PR 6

Use the exact pinned open PR head for proposed Java method/field semantics and contract fixtures. It is a proposal implementation reference, not a released standard. Differences must be classified as:
- Proposal text controls;
- Java behavior is an intended clarification adopted by i2pr;
- intentional i2pr stricter behavior;
- unresolved ambiguity requiring a new plan/proposal reconciliation.

### i2pd

Compare only the base/adopted RouterInfo and ClientServicesInfo semantics i2pd actually implements. Do not treat absence of AddressBook/TunnelManager as a failure.

## External test driver

Add a repository-owned I2PControl client fixture that can run the same bounded request corpus against:
- i2pr;
- eggstack/emissary;
- Java PR build where reproducible;
- i2pd for adopted subset.

The driver stores sanitized result classifications and hashes rather than passwords/tokens/private destination material. It must not require a frontend.

## Security requalification

Audit the complete current head for:
- authentication bypass;
- token expiry/race behavior;
- request/batch/connection exhaustion;
- slowloris/read/write deadlines;
- non-loopback TLS policy;
- path traversal/symlink/special-file attacks;
- persistent-state corruption/recovery;
- secret values in rawConfig/errors/logs/Debug;
- control/startup tunnel ownership confusion;
- runtime/persistence split-brain after injected failures;
- address-book resolver/control divergence;
- unsupported option allocation;
- direct-clearnet/provider escape;
- cancellation/task leaks.

No high or medium finding may remain for closure.

## Support and documentation reconciliation

Update:
- specs/CONFORMANCE.md;
- specs/support.toml;
- README status/workspace if new crates are now real;
- architecture docs for i2pr-i2pcontrol and i2pr-addressbook;
- control-plane security/operator documentation;
- the Proposal 170 roadmap and registry;
- closure record with exact commands, pins, matrix counts, residual limitations.

Do not state "full Proposal 170 support" unless the final matrix has no applicable unsupported/blocked cell and every source/runtime/persistence/security dimension is evidenced.

If Proposal text permits implementation-specific unsupported algorithms/providers, state the exact qualified support profile rather than using an ambiguous full label.

## Acceptance criteria

Plan 295 passes only when:
1. all required current Proposal 170 methods/selectors/actions/types/options have an exact tested wire disposition;
2. all claimed sources are truthful current owners;
3. all claimed mutations have real runtime effect and recoverable persistence;
4. all twelve tunnel families have real data paths;
5. ordinary resolver and AddressBook control state are coherent;
6. disabled/default isolation passes;
7. final differential corpus has no unexplained contract mismatch on overlapping supported surfaces;
8. security/adversarial review has no unresolved high/medium issue;
9. routine CI and focused control-plane CI are green at the exact closure head;
10. no frontend implementation or dependency entered this workstream.

If any criterion fails, close with a precise residual and register one narrow corrective plan; do not expand Plan 295 into an open-ended implementation loop.
