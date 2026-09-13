# Plan 193 status — M6 i2pd mixed-router Streaming qualification

Status: **passed-m6-i2pd-mixed-router-streaming**.

Plan of record:
[`plans/193-m6-i2pd-mixed-router-streaming-qualification.md`](193-m6-i2pd-mixed-router-streaming-qualification.md).

Execution log:
[`plans/193-streaming-status.md`](193-streaming-status.md).

This status is the newest handoff authority for the retained M6/M10 blocker line. It intentionally resolves the duplicate-numbering defect created when both the short-build corrective and the deferred Streaming pass were left under Plan 188. Historical files remain in place for evidence provenance, but their execution roles are superseded as follows:

```text
historical plans/188-m6-mixed-router-streaming-with-i2pd.md -> superseded-by-plan193
historical plans/189-m6-java-i2p-second-family-qualification-and-closure.md -> superseded-for-execution-by-plan194
plan_189 cross-family ledger/checker/workflow scaffold -> retained
```

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187_local_product = retained-passed
plan_188_short_build_corrective = retained-passed
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-boundary-diagnosis-retained
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = registered-executable (unblocked by this pass)
plan_195 = registered-blocked-by-plan194

m6_destination_remote_interop_i2pd = passed-through-raw-destination-message-plane-via-plan192
milestone6_i2pd_streaming_interop = passed-via-plan193
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 194
remaining_sequence = 194 -> 195
```

## Source floor

Registration source floor:

```text
i2pr main = 05d6d52870a81eda8891dc492d43c6d4b79bbae8
plan_192 = passed-m6-i2cp-wire-format-corrective
routine CI = 34668461179 (success)
workspace = 2283 passed, 6 ignored
```

Plan 192 proved the complete i2pd destination message plane through real SSU2, real one-hop tunnels, live NetDB, Standard LeaseSet2, ECIES/Garlic, and the corrected short-transport/I2CP-style Data envelope in both directions. Streaming and Java second-family qualification were explicitly not yet run.

## Handoff rule

Plan 193 is closed. Plan 194 (Java I2P second-family qualification)
is the current executable plan; do not begin Plan 195 until Plan 194
closes the two-family M6 criterion.

## Closure evidence (exact-head 3687189)

Two complete external passes on the closing head (`PASS1=0`,
`PASS2=0` via `bash tests/integration/m6-interop/run-streaming.sh`;
evidence.json `i2pr_commit = 3687189...`, `m6_streaming =
passed-via-i2pd-2.61.0`, 33/33 rows passed), plus the full workspace
floor green on the same head (fmt, check, 2312 passed / 0 failed /
95 suites, clippy, doc, doctests, all static boundary scripts,
ntcp2 python harness, cargo deny).

Direction A (i2pr -> i2pd STREAM, real one-hop tunnels both ways):
SYN-accepted + Established; 25 B digest `6f574c...c60d68`;
8192 B / 8-fragment digest `25df24...dacb2f80`; reverse 23 B digest
`79bb58...172c763e`; reverse 4096 B digest `b88349...e467b27a7`
(after live loss-recovery of one dropped packet via NACK +
retransmit, byte-exact); sibling stream established + 23 B digest
`7bc5bc...034ed` on its own ACCEPT socket; orderly close
(`Closed` + reference socket EOF); sibling still delivers after the
first close (23 B digest `2d5103...0530b13c`).

Direction B (i2pd STREAM CONNECT -> i2pr wildcard-0 listener through
the normal backlog/accept/SYN-response path): established at first
attempt; 17 B digest `6f28af...1429a5b139` i2pd -> i2pr; 2048 B /
2-fragment digest `3b5bfe...2f83e` i2pr -> i2pd; orderly close
(`Closed` + reference socket EOF).

Reference-side counts: transit endpoint + gateway created, LS2
stored, 8 tunnel tests ok, 2 `Incoming stream` acceptances.
Cleanup: manager `queued=0 delivered=0`, SSU2 baselines zero,
liveness first-test green alongside streaming.

§14.9 robustness disposition: drop-data-retransmit, reorder, and
duplicate recovery were exercised LIVE against the reference (the
4096 B reverse transfer lost sequence 2 in transit; the pump's
NACK/retransmit path recovered it with digest equality; reordered
3,4 buffered then delivered in order; reference retransmits
deduplicated). Stalled-reader boundedness stays covered by the
retained local deterministic suites (Plan 152 over-cap snooze rows);
reference-disconnect boundedness is proven by the both-direction
close-EOF rows; clean baseline by the manager-cleanup +
shutdown-baseline rows. No packet-manipulating harness was built
and no reference was patched, per §8.

Narrow wire-compatibility correctives landed inside this plan (no
new plan needed; no wire bytes changed, only acceptance/runtime
fidelity): per-turn `poll_acks`/`poll_retransmits` drain in the
external pump (a quiet peer awaiting our delayed ACK deadlocked the
pump); fresh SAM sockets for ACCEPT and CONNECT (reference rejects
both on the bound session socket); `StreamingReceiveLimit::
destination_path()` receive bound (reference emits 1812-byte
payloads above our 1730 advertisement; bounded by the I2CP Data
body ceiling; send path unchanged); per-delivery RNG; 4 KiB SAM
read chunks (64 KiB stack arrays overflowed the test thread).
Stop provenance for all nine is recorded in the execution log.
