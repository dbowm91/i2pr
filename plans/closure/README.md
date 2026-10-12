# Closure and Verification Records

Evidence-based completion records, grouped by subsystem. A closure record is the gate that
determines whether a milestone is complete — a commit message saying "closed" is not
closure evidence.

## Layout and naming

```text
closure/<subsystem>/NNN-status.md        (authoritative; same global NNN as the plan)
closure/<subsystem>/NNN-closure.md       (early-era closure record, Plans 001–100)
closure/<subsystem>/NNN-candidate.md     (closure-candidate record)
closure/<subsystem>/*-amendment-*.md     (status-amending authority transitions)
```

Also kept here: supporting completion narratives (`*-corrective-closure.md`,
`*-final-closure.md`, `*-completion-correction.md`, `*-terminal-cleanup.md`,
completion `*-emissary-*` / `*-terminal-native-*` records). A few narratives carry stale
`registered` headers from their drafting era — the newest `*-status.md` always wins.

Non-standard but authoritative: `closure/mixed-router-interop/193-streaming-status.md`.

## Required closure-record content

New records MUST contain: implementation commits, requirement-to-evidence matrix, tests and
guards run with outcomes (label local vs CI truthfully), migration/compatibility evidence,
security and contention evidence where applicable, documentation/operational evidence, known
limitations, unresolved findings by severity (critical/high/medium/low), and the roadmap
disposition (closed, conditionally closed, corrective pass required, or blocked).

## Causal attempts, partial evidence, and when to close

Do not finalize a `blocked` closure merely because the *first* fixture attempt
used a wrong datadir, filtered the stock's actual log spelling, lacked known
floodfills, failed to induce a stock-owned send, or selected a different eligible
peer. Correct the exact defect and rerun the affected gate **within the active
plan** using finite timeouts and regression. Record one run receipt per material
source/config change with the first missing transition and owner. An unchanged
failure may be stopped without pretending it is a protocol defect.

When a coherent plan really exhausts its authorized causal corrective scope,
write an authoritative `NNN-status.md` specifying its **scoped** pass/blocked
evidence, prerequisite, source-level cause (or precise unresolved causal gap),
and downstream unblock audit. Only a new, separately scoped successor gets a
new plan number. Past status records remain untouched, and lower-tier successes
cannot be silently promoted or erased. See
[the execution policy](../README.md#bounded-execution-and-causal-corrective-policy).

A milestone MUST NOT be marked closed when only compilation/formatting was verified, required
tests were not run, a user-visible capability has only internal infrastructure, or a known
high-severity defect remains. Historical records MUST NOT be rewritten to conceal
predecessor defects or failed verification.
