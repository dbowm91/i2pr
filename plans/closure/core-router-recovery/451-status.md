# Plan 451 status: blocked — production SAM has no remote product fabric

Closure token: `blocked-normal-sam-remote-product-fabric-not-composed`

Plan: [`451-real-process-ssu2-netdb-multihop-application-acceptance.md`](../../implementation/core-router-recovery/451-real-process-ssu2-netdb-multihop-application-acceptance.md)

## Outcome

Plan 450 Gate A remains passed. The current normal daemon still cannot meet
Plan 451 Gate B/C because its SAM listener has no route into the normal
SSU2-backed destination-group product. This is a production composition gap,
not an external-host dependency. No Gate B or Gate C success is claimed.

The source trace found two distinct owners:

- `register_sam_service` constructs `SamServiceState::new`, publishes it to
  inspection, attaches the addressbook handle, and serves the loopback listener.
- `SamServiceState::execute_session_create` creates a `SamLocalProductFabric`
  and prepares local LeaseSet2/product material for each session.
- Separately, `register_ssu2_service` starts
  `ServiceProduct::start_over_existing_daemon` only when service groups are
  active, and that product receives the existing SSU2 handle and validated
  bootstrap RouterInfos. No handle or production product factory is injected
  into the SAM owner or its session transaction.

The SAM process handshake/session path therefore does not demonstrate remote
NetDB lookup, nonzero-hop tunnels, or Streaming to an independent destination.
The existing M6 script results remain lower-layer fixture evidence and do not
substitute for the missing ordinary-daemon product owner.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Real-process SAM Gate A | Passed independently by Plan 450 (`run_lifecycle_readiness`); this closure does not revoke it. |
| Production SAM session uses the daemon's router product | Not satisfied. `crates/i2pr-daemon/src/lib.rs::register_sam_service` creates the local `SamServiceState`; `crates/i2pr-daemon/src/sam.rs::execute_session_create` constructs `SamLocalProductFabric` for session product material. |
| Normal daemon has an SSU2/NetDB group owner | Partially available for configured service groups: `register_ssu2_service` snapshots validated bootstrap RouterInfos and calls `ServiceProduct::start_over_existing_daemon`. The group owner is not passed to `SamServiceState` or its session transaction. |
| Gate B authenticated I2NP and live NetDB request/reply | Not run. Without a production SAM consumer connected to the normal router/group product, the required black-box SAM request cannot exercise the intended route. Existing M6 controls do not provide normal-process evidence. |
| Gate C nonzero-hop tunnels and external SAM/I2CP application exchange | Not run; no normal-process SAM remote product path is available. |
| M6 regression baselines | Prior local runs: `run-preflight.sh` and `run-netdb.sh` passed. `run-streaming.sh` failed during external i2pd SAM session creation (`SAM session create read`); all local rows passed. This remains a separate fixture failure and is not attributed to the normal daemon product gap. |
| Public/external qualification | Not claimed. Plan 431 remains stopped and Plan 433 remains gated. |

## Commands and outcomes

The following outcomes are local Linux evidence. The full workspace floor was
run after Plans 450/453 edits and includes the process lifecycle and SAM
integration test binaries.

```text
rtk cargo test --locked --workspace --all-targets -- --test-threads=1
  passed (4,780 passed, 39 ignored; 186 suites)
rtk bash tests/integration/m6-interop/run-preflight.sh
  passed (prior bounded local baseline)
rtk bash tests/integration/m6-interop/run-netdb.sh
  passed (prior bounded local baseline)
rtk bash tests/integration/m6-interop/run-streaming.sh
  failed at external SAM SESSION CREATE read; all local rows passed
```

No full Plan 451 Gate B/C process lane was run because source inspection shows
that its required production SAM product seam does not exist. No protocol,
listener, or reference-router code was changed for this closure.

## Security and limitations

SAM remains loopback-only, disabled by default, and experimental. Existing
local session creation remains owned by the local fabric; no direct socket or
clearnet path was introduced. The bounded M6 reference processes and scripts
were not modified. No public capabilities, RouterInfo claims, or support
inventory were promoted.

Findings: critical — none; high — none; medium — ordinary SAM session creation
is not composed over the normal daemon's SSU2/NetDB/destination-group owner;
low — independent two-host and external application evidence remains open.

## Unblock audit and disposition

Plan 451 remains **blocked** on a new normal-process SAM product composition
that preserves the single SSU2 identity/socket owner, uses validated NetDB and
the daemon's bounded destination/tunnel managers, and exposes only the existing
loopback SAM listener. Then Gate B/C must be exercised through the real process
and independent reference router. Plan 433 remains gated on Plan 431 and this
Plan 451 evidence. Plans 452 and 454 are independent; Plan 452 remains blocked
at its own stock no-dial-trigger result, and Plan 454 remains ready. No earlier
Plan 443/447/450 closure is overwritten.
