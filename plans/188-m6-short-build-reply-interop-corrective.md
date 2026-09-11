# Plan 188 — M6 short-build-reply interop corrective

Status: **registered executable corrective** (next after Plan 187).

## 1. Goal

Resolve the `m6-build-reply-interop-gap` that blocks Plan 187
(see `plans/187-status.md` §11): exact-pinned i2pd 2.61.0 accepts
our one-hop short tunnel builds (transit endpoint + gateway
created in its structured log) but emits no consumable
`ShortTunnelBuildReply` (or any reply body) within the bounded
window, so no real tunnel material installs and the seven
install-dependent Plan 187 rows stay blocked.

Plan 188 passes only when one real outbound and one real inbound
one-hop build install through **consumed reference replies** and
the seven blocked Plan 187 rows flip to passed with no other
change to the Plan 187 evidence model.

## 2. Starting evidence (do not re-derive from prose)

- `plans/187-status.md` §11 (six lane runs + manual reproductions).
- `install-pump-summary` shape: `kind_reply=0`,
  `unsupported={11: 6..9, 19: 1}`, `msgid_match=1`,
  `session-pump-delta` lossless (`datagrams_received=27`,
  `i2np_received=26`, zero drops).
- `gateway-frame-detail`: `tunnel=38146 inner=11` (one direct
  TunnelGateway frame per run; routing metadata only).
- Reference accepts both directions every run
  (`transit-endpoint-created >= 1`,
  `transit-gateway-created >= 1`); reference publishes its own
  DATAGRAM LeaseSet2 (`reference-leaseset-updated >= 1`).

## 3. Hypotheses in test order (stop at the first that explains)

1. **Record-count padding.** Our requests always carry 4
   records (Plan 110 multirecord policy) for a 1-hop path. The
   reference parses them (creates transit) but may fail to
   construct a 4-record reply. Experiment: a sender-side
   record-count option (1 record for 1 hop) behind a narrowly
   named constructor/flag; no wire-format change, no decoder
   change. If 1-record builds elicit replies, keep the option
   minimal and document the reference behavior.
2. **Reply correlation.** `route_build_outcome` matches
   (peer, transport message id), which holds only because the
   i2pr responder echoes the request id. If a reply arrives
   with a reference-assigned id (watch `msgid_match` and any
   new `kind_reply`), correlate by tunnel/creator id instead.
   Narrow `exploratory_build.rs` change with unit rows.
3. **Reply addressing.** Inspect what reply router/tunnel our
   request names (OBEP `next_router`/`next_tunnel` from the
   decrypted-plaintext unit seam) against what the reference
   needs for a direct reply in this loopback topology.
4. **Reply timing/fragmentation.** Only after 1–3 are
   excluded: extend the install window as a diagnostic (not as
   acceptance), and compare reply sizes against the SSU2
   fragmentation seam with the lossless session counters as
   the control.

## 4. Constraints

- No authentication weakening, no unsupported crypto, no
  unconfirmed install (creator-known keys are never installed
  without a consumed reply — the Plan 187 stop stands until a
  reply is consumed).
- No `LocalZeroHop`, fake lease, or direct-transport
  substitution for any counted row.
- No production wire-format change to make a test convenient;
  sender-side options must stay within the pinned spec.
- Evidence hygiene from Plan 187 is retained: raw i2pd log
  never evidence, PRIV in-memory only, counts/metadata only,
  stale-file hygiene at startup, 21-label checker green.

## 5. Acceptance criteria

Plan 188 passes only when:

1. `install-pump-summary` shows `installed_ob=1`,
   `installed_ib=1` with `kind_reply >= 2` through consumed
   reference replies (no log-correlation install);
2. the seven `blocked` Plan 187 rows flip to passed in a fresh
   `run-destination.sh` execution with no checker change other
   than additive labels if new rows are genuinely needed;
3. the Plan 187 local suites (27 unit + 9 live + liveness)
   remain green unchanged in behavior;
4. full workspace/static floor and exact-head routine CI pass;
5. `plans/188-status.md` records the root cause, the narrow
   fix, and `next_executable_plan` resuming the Plan 187 §12
   handoff (destination rows green → Streaming layers on top).

## 6. Stop conditions

Stop for a deeper program (do not stretch this corrective) if
the reference demonstrably cannot reply to short builds in any
sender shaping (e.g., it requires classic/variable builds, or
replies only to published floodfills). Capture the negative
evidence with the same sanitized counters and escalate to the
M6 program (Plan 183) rather than weakening install
correlation.
