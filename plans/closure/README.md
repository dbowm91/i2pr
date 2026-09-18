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

A milestone MUST NOT be marked closed when only compilation/formatting was verified, required
tests were not run, a user-visible capability has only internal infrastructure, or a known
high-severity defect remains. Historical records MUST NOT be rewritten to conceal
predecessor defects or failed verification.
