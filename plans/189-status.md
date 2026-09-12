# Plan 189 status — M6 Java I2P second-family qualification and mixed-router closure

Status: **`registered-blocked-by-plan188-and-plan190`** (the i2pd
first-family destination lookup row is still blocked; no Java
second-family qualification has started yet; the cross-family
ledger/checker/workflow scaffold is landed and routed through the
existing per-layer evidence scripts). Plan 190 isolates and corrects
the inbound NetDB reply-path metadata defect that kept the
destination LeaseSet2 lookup row blocked after the Plan 188 installs;
see [`plans/190-status.md`](190-status.md).

Plan of record:
[`plans/189-m6-java-i2p-second-family-qualification-and-closure.md`](189-m6-java-i2p-second-family-qualification-and-closure.md).

> Numbering note (as of Plan 190). The deferred
> `plans/188-m6-mixed-router-streaming-with-i2pd.md` is the
> Streaming pass that Plan 188 names as the next executable after
> the i2pd destination rows go green. No file renumbering is
> planned unless the deferred Streaming pass actually exercises:
> when (and only when) it does, it will be renumbered to the next
> free plan slot (likely `191`) so the in-flight references in
> `plans/README.md`, `docs/architecture/i2pr-daemon.md`, and the
> agent skills remain accurate. Plan 190 was added as a new file
> for the inbound NetDB reply-path corrective rather than via the
> 189→190 renumber originally promised here; the present file
> stays at `189` and Plan 190 stays at `190`.

## Current authority

```text
plan_192 = registered-m6-i2cp-wire-format-corrective (next executable; inbound-delivery boundary E I2CP-style Data body)
plan_191 = stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked; 2 ordering rows flipped passed)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (local rows passed; remote lane proves 3 destination rows flipped blocked -> passed)
plan_189 = registered-blocked-by-plan188-and-plan190-and-plan191-and-plan192 (cross-family scaffold only; Java qualification not started)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; 5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 inbound-delivery rows blocked on plan192; 2 ordering rows passed)
plan_187 = blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_183 = registered-m6-mixed-router-streaming-interop-program
m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-blocked-plan191-then-plan192
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed InboundGatewayRoute + daemon-owned adapter; remote lane flips 3 destination rows blocked -> passed)
m6_inbound_destination_delivery_boundary_E = stopped-pending-plan192 (i2pd-compatible I2CP-style Data body wire-format)
m6_inbound_destination_delivery = blocked-pending-plan192
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 192
m6_second_family_java = not-yet-started
```

## What landed

Plan 189 §8 fail-closed M6 mixed-router evidence checker/ledger
infrastructure only. No M6 wire change, no daemon code change,
no runtime change, no interop qualification executed. The
scaffold reuses the four existing per-layer harnesses
(`run-preflight.sh`, `run-tunnels.sh`, `run-netdb.sh`,
`run-destination.sh`) plus their dedicated static evidence
checkers; Plan 189 adds the cross-family aggregator that the
two-family claim requires.

```text
scripts/check-m6-mixed-router-acceptance-evidence.sh (new)
  Plan 189 §8 fail-closed structural checker. Walks the four
  per-layer harnesses plus their static checkers, enforces that
  the i2pd pin (`635b013a...`) and the Java I2P pin
  (`9134f808...`) are referenced by every per-layer static
  checker, enforces that the cross-family aggregator harness
  exists and references both pins through the existing
  record_guarded / m6_row / ref_row / blocked_row helpers, and
  rejects hard-coded `passed` records for the two-family
  guards. No required row is recorded `passed` except via a
  command/test exit code.

tests/integration/m6-interop/run-m6-mixed-router.sh (new)
  Plan 189 §8 cross-family harness scaffolding. Loops over the
  per-layer harnesses in the same order the four Plans 184–188
  ran them, captures each layer's `results.tsv` and `evidence.json`
  into a unified cross-family `evidence.json` written only when
  every per-layer exit code is zero. Fails closed (exit 1) if any
  required per-layer harness is missing or fails, or if the Java
  I2P pin is not recorded in any per-layer run. Does not start a
  Java router; the second-family Java qualification harness
  belongs to a follow-up plan after Plan 188 closes.

.github/workflows/m6-mixed-router-external.yml (new)
  Plan 189 §8 manual external lane (workflow_dispatch only,
  Ubuntu, timeout 60 min). Installs the build dependencies the
  Java I2P reference build needs (ant, default-jdk-headless,
  gettext-base) alongside the existing i2pd Boost/OpenSSL
  stack. Steps run the structural checker, the i2pd-only
  per-layer harnesses (matching the Plan 187 lane), and the
  cross-family aggregator, then upload sanitized evidence.
  The Java router is **not** started by this workflow yet —
  the lane is intentionally fail-closed until the Java
  qualification harness lands.
```

The existing Plan 161/162/184/185/186/187 evidence scripts are
unchanged; Plan 189 only reuses them and adds the cross-family
aggregator. No `crates/` or `Cargo.lock` changes.

## Evidence

Local lane (no external process):

```text
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
# exit 0; per-layer static checkers and the cross-family
# aggregator scaffolding are wired to both pins

bash tests/integration/m6-interop/run-m6-mixed-router.sh
# exit 0; per-layer runs reuse the Plan 184–187 evidence
# directories, and the cross-family evidence.json is emitted
# under target/interop/m6-mixed-router-evidence/
```

External lane (manual workflow, exact-pinned i2pd only):

```text
# Workflow .github/workflows/m6-mixed-router-external.yml is
# added in this plan; the Java second-family invocation is
# registered as a follow-up after Plan 188 closes.
```

## Stop conditions

This status keeps `milestone6_interoperable = not-yet-claimed`
because Plan 189 cannot pass until Plan 188's five blocked
destination rows flip to passed and the deferred
`188-m6-mixed-router-streaming-with-i2pd.md` Streaming pass
also lands. Per the plan §11 stop rule, the Java second-family
qualification is never marked `passed` from static source
inspection or from a self-composed substitute; if the exact-pinned
Java I2P lane cannot be orchestrated on this host, the precise
blocker is captured and a follow-up corrective plan is registered
without flipping any second-family row.

## Handoff

Plan 189 waits for:

1. Plan 188's five blocked destination rows to flip to passed
   through the existing per-layer harness (the cross-family
   aggregator is already wired to surface them);
2. the deferred `188-m6-mixed-router-streaming-with-i2pd.md`
   Streaming pass to land (still blocked on the same
   destination/lookup/Streaming gap);
3. a follow-up plan that lands the second-family Java I2P
   qualification harness under
   `tests/integration/m6-interop/run-java.sh`, registers the
   exact Java pin through `scripts/interop/fetch-m6-java.sh`
   (reusing the Plan 170 `fetch-i2cp-clients.sh` Java I2P
   fetch), and reruns the cross-family aggregator to bind the
   two families to the same `evidence.json`.

Until then:

```text
plan_192 = registered-m6-i2cp-wire-format-corrective (next executable; inbound-delivery boundary E I2CP-style Data body)
plan_191 = stopped-by-inbound-delivery-boundary-E (retained)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (retained)
plan_189 = registered-blocked-by-plan188-and-plan190-and-plan191-and-plan192 (cross-family scaffold only; Java qualification not started)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed)
m6_second_family_java = not-yet-started
next_executable_plan = 192 (resolve inbound-delivery boundary E wire-format)
```