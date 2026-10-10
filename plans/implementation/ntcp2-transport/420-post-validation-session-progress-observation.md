# Plan 420 — observe post-validation current-pin NTCP2 session progress

Status: **ready** — `registered-post-validation-session-progress-observation`.
Corrective successor to [Plan 419](../../closure/ntcp2-transport/419-status.md).

## Objective

Count the pinned stock `SessionConfirmed from` marker, which follows
SessionConfirmed part decryption and RouterInfo construction, plus the fixed
session-termination marker. Preserve counts only and spend at most one fresh
forward loopback attempt; reverse is allowed only if forward passes.

## Why ready

- Plan 419 recorded the SessionConfirmed receive marker but zero selected
  validation-rejection markers, zero I2NP blocks, and zero DeliveryStatus.
- Pinned i2pd source emits `SessionConfirmed from` after the SessionConfirmed
  part processing and RouterInfo verification path. `NTCP2Session::Terminate`
  emits a session termination line, which can distinguish a later close.
- The current parser already uses baseline filtering and count-only evidence;
  the private helper already enables stock debug logging.

## Invariants

1. Use pristine i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`; never patch or instrument its
   source or binary.
2. Match only fixed stock marker prefixes; never retain endpoints, identities,
   raw lines, message IDs, paths, or digests.
3. Use `127.0.0.1`, network ID 2, and `current-network-loopback` only.
4. One forward attempt maximum; reverse only after a complete forward pass.
5. Normal-daemon NTCP2 remains disabled and non-advertised.

## Scope and work

Add closed counters for the post-validation marker and session termination.
Source-map exact log sites, test each marker plus baseline exclusion and
redaction, update the source checker, rebuild, and run the same focused gates
as Plan 419. Then spend one forward attempt. Stop on any failure and preserve
only sanitized evidence.

Out of scope: production protocol changes, reference source changes, public
peers, retries, daemon activation, and support promotion.

## Acceptance and closure

Acceptance requires one correlated DeliveryStatus and successful cleanup;
otherwise close as stopped with the observed bounded stage counts and blocker.
Record exact commands, source mapping, mutation outcomes, attempt result, and
fresh dependency audit. Plan 434 remains blocked until authenticated two-way
I2NP evidence exists.
