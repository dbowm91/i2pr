# Plan 308 — HTTP proxy differential qualification without hostile Streaming control

Status at registration: **blocked-on-plan307**

Classification: application-profile evidence + corrective capability.

Hard dependencies: Plan 307 passed; retained Plan 304 Ubuntu reference artifacts; ADR 0030.

## 1. Objective

Build an Ubuntu-only controlled HTTP topology using ordinary server Destinations, capture the exact request bytes emitted by i2pr, pinned i2pd 2.61.0, and pinned Java I2P 2.13.0 client proxies, then converge i2pr's qualified HTTP profile without depending on a hostile raw Streaming adapter.

## 2. Why this plan exists

Plan 297 coupled HTTP qualification to a larger three-family topology. Plan 304 then stopped because hostile Streaming packet control is unavailable inside stock references. HTTP rewriting does not need that interface. This plan separates the application-layer evidence from Streaming.

## 3. Current evidence

Plan 304 already retained the Ubuntu preflight, exact reference source/artifact manifests, topology contract, and `http-corpus.toml`. Plan 296 already fixed explicit product branding and local-alias Host leakage.

## 4. Invariants

Exact reference pins, loopback/private topology, no public reseed, identical synthetic corpus, sanitized captures, Plan 307 router-unlinkability gate, no TLS MITM, and no random header mutation.

## 5. Scope

Use the retained Plan 304 preflight/reference manifests and HTTP corpus. Add family-specific HTTP proxy runners, one controlled remote HTTP endpoint, canonical capture serialization, a differential classifier, and only evidence-driven production changes.

## 6. Required production changes

Production HTTP changes are allowed only after captures. Prefer behavior common to Java+i2pd. Where they materially diverge, use i2pd as the coherent compatibility target unless doing so would violate a stronger protocol/security invariant. Do not combine isolated traits into a unique hybrid.

Candidate dimensions include request-target form, Host, User-Agent, header filtering, casing/order/multiplicity, connection semantics, Referer/From, Via/Forwarded/X-Forwarded, Accept-Language/Encoding, Client Hints, Fetch Metadata, Priority, and custom headers.

## 7. Ordered work packages

WP1 controlled ordinary Destination/readiness; WP2 i2pr/i2pd/Java HTTP runners; WP3 sanitized capture schema; WP4 fixed-corpus execution; WP5 disposition matrix; WP6 narrow convergence edits; WP7 repeat captures and evidence checker.

## 8. Failure, cancellation, restart, and contention

Fresh datadir per family. Hard startup/capture deadlines. Missing family is unexecuted, not pass. Cleanup owns every child PID. No retry-until-green. Production edits are not attempted until at least valid i2pr+i2pd captures exist.

## 9. Compatibility and migration

No config migration. Raw/keep expert behavior may remain explicit. Qualified default remains one named profile rather than many independent toggles.

## 10. Required tests

Capture canonicalization, duplicate/order preservation, redaction, corpus completeness, reference-pin mismatch, missing-family failure, cleanup, and before/after differential fixtures.

## 11. Exact verification commands

Run the full Plan 307 floor plus canonical equivalents of:

~~~bash
bash scripts/interop/anonymity/preflight-ubuntu.sh
bash scripts/interop/anonymity/run-http-profile.sh --family i2pr
bash scripts/interop/anonymity/run-http-profile.sh --family i2pd
bash scripts/interop/anonymity/run-http-profile.sh --family java
bash scripts/check-http-anonymity-evidence.sh
~~~

## 12. Documentation updates

Record the observed per-family matrix and exact selected profile. Mark the old Plan 297/304 HTTP topology requirement as superseded by this narrower corrective without rewriting their closure records.

## 13. Acceptance criteria

All three ordinary HTTP captures execute on exact pins; i2pr has no unresolved i2pr-only controllable request fingerprint in the qualified profile; every intentional divergence is documented; Plan 307 leak/sanitation checks remain green.

## 14. Stop conditions

Stop if ordinary controlled HTTP cannot be exercised without patching a reference, or if a required difference is below encrypted Streaming and therefore belongs to Plans 312/313.

## 15. Closure evidence required

Exact pins/artifact hashes, capture corpus and hashes, differential matrix, production diff, repeated post-change captures, full verification floor.

## 16. Handoff

A pass closes the HTTP branch needed by the future integrated anonymity successor.
